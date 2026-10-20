//! Stable canonical owners and paid physical frontiers on one writer revision.

use super::super::load::{debit, entry, reserve_slots, sum};
use super::{ClaimTransactionError, ClaimTransactionLimits};
use std::collections::BTreeMap;
use wf_domain::claim::apply::{ClaimQuery, ClaimStates, ClaimWindowEnd};
use wf_domain::claim::{ClaimRecord, DueOrder, LaneKey};
use wf_domain::policy::InstalledRetryPolicy;
use wf_domain::policy::complete::{
    CompletePolicyValue, PolicySchemaCharge, project_record_batch_policy,
    record_batch_policy_projection_bytes,
};
use wf_domain::policy::definition::StateMode;
use wf_domain::policy::installation::PolicyCandidate;
use wf_domain::workflow::request::RequestScope;
use wf_ports::log::{DecodedRequest, FreshClaimPhase, RecordBatchApplyError};
use wf_ports::state::{
    ClaimDueKind, ClaimDueStep, GroupClaimDueCursor, GroupClaimTransaction, StateTransactionError,
    StoredWorkflowRecord,
};
use wf_types::identity::{GroupId, PartitionId, RecordId};
use wf_types::version::TimestampMillis;

mod lanes;

pub(super) struct LoadedClaim {
    pub(super) queries: Vec<(PartitionId, RecordId)>,
    pub(super) records: Vec<StoredWorkflowRecord>,
    pub(super) placements: BTreeMap<PartitionId, GroupId>,
    pub(super) policy: Option<CompletePolicyValue>,
    pub(super) projection: Option<(PolicyCandidate, InstalledRetryPolicy)>,
    pub(super) parallel: Vec<usize>,
    pub(super) heads: Vec<(Vec<u8>, PartitionId, Option<usize>)>,
    pub(super) state_keys: Vec<Vec<u8>>,
    pub(super) parallel_end: ClaimWindowEnd,
    pub(super) heads_complete: bool,
    pub(super) staging_used: usize,
}

impl LoadedClaim {
    pub(super) fn load<T: GroupClaimTransaction>(
        tx: &T,
        scope: &RequestScope,
        fresh: &FreshClaimPhase<'_>,
        limits: ClaimTransactionLimits,
    ) -> Result<Self, ClaimTransactionError> {
        let log = fresh
            .control()
            .next()
            .last_entry()
            .ok_or(StateTransactionError::Corrupt)?
            .log();
        let envelope = fresh
            .decoded()
            .claim_envelope(log)
            .ok_or(StateTransactionError::Corrupt)?;
        let cap = limits.base.maximum_staging_bytes;
        let mut loaded = Self {
            queries: Vec::new(),
            records: Vec::new(),
            placements: BTreeMap::new(),
            policy: None,
            projection: None,
            parallel: Vec::new(),
            heads: Vec::new(),
            state_keys: Vec::new(),
            parallel_end: ClaimWindowEnd::RangeEnd,
            heads_complete: true,
            staging_used: 0,
        };
        debit(&mut loaded.staging_used, 1024, cap)?;
        loaded.load_policy(tx, scope, envelope.query.workflow_type, limits)?;
        loaded.load_lanes(tx, scope, envelope.query, envelope.head_budget, cap)?;
        if loaded.heads_complete {
            loaded.load_due(
                tx,
                scope,
                envelope.query,
                fresh.control().execution_time(),
                envelope.scan_budget,
                cap,
            )?;
        }
        loaded.load_state_keys(fresh.decoded(), cap)?;
        Ok(loaded)
    }

    fn load_policy<T: GroupClaimTransaction>(
        &mut self,
        tx: &T,
        scope: &RequestScope,
        kind: &[u8],
        limits: ClaimTransactionLimits,
    ) -> Result<(), ClaimTransactionError> {
        let mut charge = PolicySchemaCharge::new(limits.base.schema);
        self.policy = tx.installed_policy(scope.tenant(), kind, limits.base.schema, &mut charge)?;
        if let Some(policy) = &self.policy {
            if policy.data().workflow_type != kind {
                return Err(StateTransactionError::Corrupt.into());
            }
            if policy.data().governance.is_some()
                || policy
                    .data()
                    .states
                    .values()
                    .any(|state| state.governance.is_some())
            {
                return Err(ClaimTransactionError::UnsupportedGovernance);
            }
            let bytes = record_batch_policy_projection_bytes(policy)?;
            debit(
                &mut self.staging_used,
                bytes,
                limits.base.maximum_staging_bytes,
            )?;
            self.projection = Some(project_record_batch_policy(
                policy,
                limits.base.schema,
                &mut charge,
                bytes,
            )?);
        }
        Ok(())
    }

    fn load_due<T: GroupClaimTransaction>(
        &mut self,
        tx: &T,
        scope: &RequestScope,
        query: ClaimQuery<'_>,
        now: TimestampMillis,
        budget: usize,
        cap: usize,
    ) -> Result<(), ClaimTransactionError> {
        let mut cursor = tx.claim_due_cursor(scope.tenant(), query, now)?;
        loop {
            match cursor.next()? {
                ClaimDueStep::RangeEnd => {
                    self.parallel_end = ClaimWindowEnd::RangeEnd;
                    return Ok(());
                }
                ClaimDueStep::BudgetEnd => return Err(StateTransactionError::Limit.into()),
                ClaimDueStep::Row { due, kind } => {
                    debit(
                        &mut self.staging_used,
                        sum(&[512, due.partition_key.len(), due.id.len()])?,
                        cap,
                    )?;
                    let partition = PartitionId::new(copy(due.partition_key)?)
                        .map_err(|_| StateTransactionError::Corrupt)?;
                    let id =
                        RecordId::new(copy(due.id)?).map_err(|_| StateTransactionError::Corrupt)?;
                    self.load_placement(tx, scope, &partition, cap)?;
                    let owner = tx
                        .record(scope.tenant(), &partition, &id)?
                        .ok_or(StateTransactionError::Corrupt)?;
                    let row = entry(&owner);
                    if owner.tenant != *scope.tenant()
                        || owner.partition != partition
                        || owner.record.id() != &id
                        || DueOrder::for_record(&row) != Some(due)
                    {
                        return Err(StateTransactionError::Corrupt.into());
                    }
                    match kind {
                        ClaimDueKind::FifoHead => {
                            self.verify_fifo(tx, scope, &owner)?;
                        }
                        ClaimDueKind::Parallel => {
                            let lane =
                                LaneKey::for_record(&row).ok_or(StateTransactionError::Corrupt)?;
                            if self.mode(row.logical_state()) != StateMode::Parallel
                                || tx.fifo_head(scope.tenant(), lane)?.is_some()
                            {
                                return Err(StateTransactionError::Corrupt.into());
                            }
                        }
                    }
                    let eligible = kind == ClaimDueKind::Parallel && physical_matches(query, row);
                    let index = self.store_record(owner, cap)?;
                    if eligible {
                        reserve_slots(&mut self.parallel, 1, &mut self.staging_used, cap)?;
                        self.parallel.push(index);
                        if self.parallel.len() == budget + 1 {
                            self.parallel_end = ClaimWindowEnd::PrefixEnd;
                            return Ok(());
                        }
                    }
                }
            }
        }
    }

    pub(super) fn mode(&self, state: &[u8]) -> StateMode {
        self.projection
            .as_ref()
            .map_or(StateMode::Parallel, |(policy, _)| {
                policy.policy().state_mode(state)
            })
    }

    fn store_record(
        &mut self,
        owner: StoredWorkflowRecord,
        cap: usize,
    ) -> Result<usize, ClaimTransactionError> {
        if let Some(index) = self.records.iter().position(|row| {
            row.partition == owner.partition && row.record.id() == owner.record.id()
        }) {
            if self.records[index] != owner {
                return Err(StateTransactionError::Corrupt.into());
            }
            return Ok(index);
        }
        reserve_slots(&mut self.queries, 1, &mut self.staging_used, cap)?;
        reserve_slots(&mut self.records, 1, &mut self.staging_used, cap)?;
        debit(
            &mut self.staging_used,
            sum(&[
                owner.partition.as_bytes().len(),
                owner.record.id().as_bytes().len(),
            ])?,
            cap,
        )?;
        let partition = PartitionId::new(copy(owner.partition.as_bytes())?)
            .map_err(|_| StateTransactionError::Corrupt)?;
        let id = RecordId::new(copy(owner.record.id().as_bytes())?)
            .map_err(|_| StateTransactionError::Corrupt)?;
        self.queries.push((partition, id));
        self.records.push(owner);
        Ok(self.records.len() - 1)
    }

    fn load_placement<T: GroupClaimTransaction>(
        &mut self,
        tx: &T,
        scope: &RequestScope,
        partition: &PartitionId,
        cap: usize,
    ) -> Result<(), ClaimTransactionError> {
        if self.placements.contains_key(partition) {
            return Ok(());
        }
        let group = tx
            .placement(partition)?
            .ok_or(StateTransactionError::Corrupt)?;
        if &group != scope.group() {
            return Err(StateTransactionError::Corrupt.into());
        }
        debit(
            &mut self.staging_used,
            sum(&[256, partition.as_bytes().len()])?,
            cap,
        )?;
        let key = PartitionId::new(copy(partition.as_bytes())?)
            .map_err(|_| StateTransactionError::Corrupt)?;
        self.placements.insert(key, group);
        Ok(())
    }

    fn load_state_keys(
        &mut self,
        decoded: &DecodedRequest<'_>,
        cap: usize,
    ) -> Result<(), ClaimTransactionError> {
        reserve_slots(
            &mut self.state_keys,
            self.queries.len(),
            &mut self.staging_used,
            cap,
        )?;
        for (partition, id) in &self.queries {
            let length = sum(&[
                18,
                decoded.isolation_scope().len(),
                partition.as_bytes().len(),
                id.as_bytes().len(),
            ])?;
            debit(&mut self.staging_used, length, cap)?;
            self.state_keys.push(
                decoded
                    .claim_state_key(partition, id)
                    .map_err(RecordBatchApplyError::Encoding)?,
            );
        }
        Ok(())
    }
}

pub(super) fn copy(bytes: &[u8]) -> Result<Vec<u8>, ClaimTransactionError> {
    let mut copy = Vec::new();
    copy.try_reserve_exact(bytes.len())
        .map_err(|_| StateTransactionError::Allocation)?;
    copy.extend_from_slice(bytes);
    Ok(copy)
}

pub(super) fn physical_matches(query: ClaimQuery<'_>, row: ClaimRecord<'_>) -> bool {
    (match query.states {
        ClaimStates::Any => true,
        ClaimStates::Only(states) => states.contains(&row.record.state()),
    }) && !query.exclude_states.contains(&row.record.state())
}
