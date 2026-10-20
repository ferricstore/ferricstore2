//! Independent query backing and prospectively charged, same-revision owners.

use std::collections::{BTreeMap, BTreeSet};

use wf_domain::claim::ClaimRecord;
use wf_domain::policy::InstalledRetryPolicy;
use wf_domain::policy::complete::{
    PolicySchemaCharge, original_record_batch_policy_projection_bytes,
    project_original_record_batch_policy, project_record_batch_policy,
    record_batch_policy_projection_bytes,
};
use wf_domain::policy::definition::PolicySnapshot;
use wf_domain::workflow::Operation;
use wf_domain::workflow::request::RequestScope;
use wf_ports::consensus::PreparedBatchItem;
use wf_ports::log::{CodecError, DecodedRequest, RecordBatchApplyError};
use wf_ports::state::{
    GroupApplyTransaction, StateTransactionError, StoredPolicyValue, StoredWorkflowRecord,
};
use wf_types::identity::{GroupId, PartitionId, RecordId};

use super::{RecordBatchTransactionError, RecordBatchTransactionLimits};

pub(super) struct LoadedRecordBatch {
    pub(super) queries: Vec<(PartitionId, RecordId)>,
    pub(super) records: Vec<Option<StoredWorkflowRecord>>,
    pub(super) placements: BTreeMap<PartitionId, GroupId>,
    pub(super) policy_types: Vec<Vec<u8>>,
    pub(super) policies: Vec<Option<StoredPolicyValue>>,
    pub(super) projections: Vec<Option<(PolicySnapshot, InstalledRetryPolicy)>>,
    pub(super) parallel: Vec<(usize, bool)>,
    pub(super) members: Vec<(usize, bool)>,
    pub(super) heads: Vec<(usize, Option<StoredWorkflowRecord>)>,
    pub(super) staging_used: usize,
}

impl LoadedRecordBatch {
    pub(super) fn load<T: GroupApplyTransaction>(
        tx: &T,
        scope: &RequestScope,
        decoded: &DecodedRequest<'_>,
        limits: RecordBatchTransactionLimits,
    ) -> Result<Self, RecordBatchTransactionError> {
        let items = decoded
            .request()
            .operation
            .mutation_items()
            .ok_or(RecordBatchApplyError::Encoding(CodecError::UnsupportedTag))?;
        let mut loaded = Self::new(items.len(), limits.maximum_staging_bytes)?;
        let mut seen_types = BTreeSet::new();
        loaded.copy_queries(items, &mut seen_types, limits.maximum_staging_bytes)?;
        loaded.load_points(tx, scope, limits.maximum_staging_bytes)?;
        for row in loaded.records.iter().flatten() {
            append_type(
                &mut loaded.policy_types,
                &mut seen_types,
                &mut loaded.staging_used,
                row.record.workflow_type(),
                limits.maximum_staging_bytes,
            )?;
        }
        drop(seen_types);
        loaded.load_policies(tx, scope, limits)?;
        super::observations::load_indexes(&mut loaded, tx, scope, limits.maximum_staging_bytes)?;
        Ok(loaded)
    }

    fn new(count: usize, cap: usize) -> Result<Self, RecordBatchTransactionError> {
        let mut loaded = Self {
            queries: Vec::new(),
            records: Vec::new(),
            placements: BTreeMap::new(),
            policy_types: Vec::new(),
            policies: Vec::new(),
            projections: Vec::new(),
            parallel: Vec::new(),
            members: Vec::new(),
            heads: Vec::new(),
            staging_used: 0,
        };
        debit(&mut loaded.staging_used, 1_024, cap)?;
        reserve_slots(&mut loaded.queries, count, &mut loaded.staging_used, cap)?;
        let type_slots = count.checked_mul(2).ok_or_else(overflow)?;
        reserve_slots(
            &mut loaded.policy_types,
            type_slots,
            &mut loaded.staging_used,
            cap,
        )?;
        Ok(loaded)
    }

    fn copy_queries<'input>(
        &mut self,
        items: &'input [PreparedBatchItem],
        seen_types: &mut BTreeSet<&'input [u8]>,
        cap: usize,
    ) -> Result<(), RecordBatchTransactionError> {
        let mut seen_queries = BTreeSet::new();
        for item in items {
            let query = (&item.partition, &item.command.id);
            if !seen_queries.contains(&query) {
                debit(&mut self.staging_used, 128, cap)?;
                seen_queries.insert(query);
                debit(
                    &mut self.staging_used,
                    sum(&[
                        item.partition.as_bytes().len(),
                        item.command.id.as_bytes().len(),
                    ])?,
                    cap,
                )?;
                self.queries.push((
                    PartitionId::new(copy(item.partition.as_bytes())?)
                        .map_err(|_| StateTransactionError::Corrupt)?,
                    RecordId::new(copy(item.command.id.as_bytes())?)
                        .map_err(|_| StateTransactionError::Corrupt)?,
                ));
            }
            if let Operation::Create(create) = &item.command.operation {
                append_type(
                    &mut self.policy_types,
                    seen_types,
                    &mut self.staging_used,
                    &create.workflow_type,
                    cap,
                )?;
            }
        }
        Ok(())
    }

    fn load_points<T: GroupApplyTransaction>(
        &mut self,
        tx: &T,
        scope: &RequestScope,
        cap: usize,
    ) -> Result<(), RecordBatchTransactionError> {
        reserve_slots(
            &mut self.records,
            self.queries.len(),
            &mut self.staging_used,
            cap,
        )?;
        for (partition, id) in &self.queries {
            if !self.placements.contains_key(partition)
                && let Some(group) = tx.placement(partition)?
            {
                debit(
                    &mut self.staging_used,
                    sum(&[256, partition.as_bytes().len(), group.as_bytes().len()])?,
                    cap,
                )?;
                let key = PartitionId::new(copy(partition.as_bytes())?)
                    .map_err(|_| StateTransactionError::Corrupt)?;
                self.placements.insert(key, group);
            }
            let row = tx.record(scope.tenant(), partition, id)?;
            if row.as_ref().is_some_and(|row| {
                row.tenant != *scope.tenant()
                    || row.partition != *partition
                    || row.record.id() != id
            }) {
                return Err(StateTransactionError::Corrupt.into());
            }
            self.records.push(row);
        }
        Ok(())
    }

    fn load_policies<T: GroupApplyTransaction>(
        &mut self,
        tx: &T,
        scope: &RequestScope,
        limits: RecordBatchTransactionLimits,
    ) -> Result<(), RecordBatchTransactionError> {
        let cap = limits.maximum_staging_bytes;
        reserve_slots(
            &mut self.policies,
            self.policy_types.len(),
            &mut self.staging_used,
            cap,
        )?;
        reserve_slots(
            &mut self.projections,
            self.policy_types.len(),
            &mut self.staging_used,
            cap,
        )?;
        let mut charge = PolicySchemaCharge::new(limits.schema);
        for kind in &self.policy_types {
            let policy =
                tx.stored_policy_value(scope.tenant(), kind, limits.schema, &mut charge)?;
            let projection = match &policy {
                Some(StoredPolicyValue::Complete(policy)) => {
                    if policy.data().workflow_type != *kind {
                        return Err(StateTransactionError::Corrupt.into());
                    }
                    let bytes = record_batch_policy_projection_bytes(policy)?;
                    debit(&mut self.staging_used, bytes, cap)?;
                    let (mode, retry) =
                        project_record_batch_policy(policy, limits.schema, &mut charge, bytes)?;
                    Some((mode.into_policy(), retry))
                }
                Some(StoredPolicyValue::Original(raw)) => {
                    let owner = raw
                        .len()
                        .checked_mul(256)
                        .and_then(|n| n.checked_add(65_536))
                        .ok_or_else(overflow)?;
                    debit(&mut self.staging_used, owner, cap)?;
                    let bytes = original_record_batch_policy_projection_bytes(
                        raw,
                        kind,
                        limits.schema,
                        &mut charge,
                    )?;
                    debit(
                        &mut self.staging_used,
                        owner.checked_add(bytes).ok_or_else(overflow)?,
                        cap,
                    )?;
                    Some(project_original_record_batch_policy(
                        raw,
                        kind,
                        limits.schema,
                        &mut charge,
                        bytes,
                    )?)
                }
                None => None,
            };
            self.policies.push(policy);
            self.projections.push(projection);
        }
        Ok(())
    }
}

pub(super) fn entry(owner: &StoredWorkflowRecord) -> ClaimRecord<'_> {
    ClaimRecord {
        record: &owner.record,
        partition_key: owner.partition.as_bytes(),
        priority: owner.ordering.priority,
        state_enter_seq: owner.ordering.state_enter_seq,
    }
}

fn append_type<'input>(
    types: &mut Vec<Vec<u8>>,
    seen: &mut BTreeSet<&'input [u8]>,
    used: &mut usize,
    kind: &'input [u8],
    cap: usize,
) -> Result<(), RecordBatchTransactionError> {
    if !seen.contains(kind) {
        debit(used, 128, cap)?;
        seen.insert(kind);
        debit(used, sum(&[128, kind.len()])?, cap)?;
        types.push(copy(kind)?);
    }
    Ok(())
}

pub(super) fn reserve_slots<T>(
    buffer: &mut Vec<T>,
    count: usize,
    used: &mut usize,
    cap: usize,
) -> Result<(), RecordBatchTransactionError> {
    debit(used, count.checked_mul(512).ok_or_else(overflow)?, cap)?;
    buffer
        .try_reserve_exact(count)
        .map_err(|_| StateTransactionError::Allocation)?;
    Ok(())
}

pub(super) fn debit(
    used: &mut usize,
    bytes: usize,
    cap: usize,
) -> Result<(), RecordBatchTransactionError> {
    let next = used.checked_add(bytes).ok_or_else(overflow)?;
    if next > cap {
        return Err(StateTransactionError::Limit.into());
    }
    *used = next;
    Ok(())
}

pub(super) fn sum(parts: &[usize]) -> Result<usize, RecordBatchTransactionError> {
    parts.iter().try_fold(0_usize, |sum, next| {
        sum.checked_add(*next).ok_or_else(overflow)
    })
}

pub(super) fn overflow() -> RecordBatchTransactionError {
    RecordBatchApplyError::Encoding(CodecError::LengthOverflow).into()
}

fn copy(bytes: &[u8]) -> Result<Vec<u8>, RecordBatchTransactionError> {
    let mut output = Vec::new();
    output
        .try_reserve_exact(bytes.len())
        .map_err(|_| StateTransactionError::Allocation)?;
    output.extend_from_slice(bytes);
    Ok(output)
}
