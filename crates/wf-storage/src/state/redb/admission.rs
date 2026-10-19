//! Cumulative logical owners and precise operational engine error classes.

use super::{StateStoreLimits, store::RedbStateTransaction};
use crate::state::{key, value};
use std::cell::Cell;
use wf_domain::claim::{ClaimRecord, DueOrder, LaneKey, LaneOrder};
use wf_domain::policy::complete::{CompletePolicyEncodingError, PolicySchemaError};
use wf_domain::workflow::admission::AdmissionLimits;
use wf_domain::workflow::request::RequestScope;
use wf_ports::state::{StateTransactionError, StoredWorkflowRecord};
use wf_types::identity::{GroupId, PartitionId, RecordId, RecoveryId, RequestId, TenantId};

pub(super) const ENGINE_BYTES: usize = 3 * 1_024 * 1_024 * 1_024;

pub(super) fn total(
    base: usize,
    values: impl IntoIterator<Item = usize>,
) -> Result<usize, StateTransactionError> {
    values.into_iter().try_fold(base, |n, v| {
        n.checked_add(v).ok_or(StateTransactionError::Limit)
    })
}

pub(super) fn debit(
    counter: &Cell<usize>,
    bytes: usize,
    cap: usize,
) -> Result<(), StateTransactionError> {
    let next = total(counter.get(), [bytes])?;
    if next > cap {
        return Err(StateTransactionError::Limit);
    }
    counter.set(next);
    Ok(())
}

pub(super) fn copy_bytes(bytes: &[u8]) -> Result<Vec<u8>, StateTransactionError> {
    let mut output = Vec::new();
    output
        .try_reserve_exact(bytes.len())
        .map_err(|_| StateTransactionError::Allocation)?;
    output.extend_from_slice(bytes);
    Ok(output)
}

pub(super) fn value_error(error: value::StateValueError) -> StateTransactionError {
    match error {
        value::StateValueError::Allocation => StateTransactionError::Allocation,
        value::StateValueError::Limit { .. }
        | value::StateValueError::LengthOverflow
        | value::StateValueError::MembershipLimit => StateTransactionError::Limit,
        _ => StateTransactionError::Corrupt,
    }
}

pub(super) fn storage_error(error: &redb::StorageError) -> StateTransactionError {
    match error {
        redb::StorageError::Corrupted(_) => StateTransactionError::Corrupt,
        redb::StorageError::ValueTooLarge(_) => StateTransactionError::Limit,
        _ => StateTransactionError::Unavailable,
    }
}
pub(super) fn database_error(error: redb::DatabaseError) -> StateTransactionError {
    match error {
        redb::DatabaseError::Storage(e) => storage_error(&e),
        _ => StateTransactionError::Unavailable,
    }
}
pub(super) fn transaction_error(error: redb::TransactionError) -> StateTransactionError {
    match error {
        redb::TransactionError::Storage(e) => storage_error(&e),
        _ => StateTransactionError::Unavailable,
    }
}
pub(super) fn table_error(error: redb::TableError) -> StateTransactionError {
    match error {
        redb::TableError::Storage(e) => storage_error(&e),
        _ => StateTransactionError::Corrupt,
    }
}
pub(super) fn policy_error(error: CompletePolicyEncodingError) -> StateTransactionError {
    match error {
        CompletePolicyEncodingError::ItemLimit
        | CompletePolicyEncodingError::Schema(
            PolicySchemaError::WorkBytes { .. } | PolicySchemaError::LengthOverflow,
        ) => StateTransactionError::Limit,
        CompletePolicyEncodingError::Schema(PolicySchemaError::ProfileMismatch) => {
            StateTransactionError::InvalidChanges
        }
        CompletePolicyEncodingError::AllocationFailure => StateTransactionError::Allocation,
        _ => StateTransactionError::Corrupt,
    }
}
pub(super) fn workspace(
    group: &GroupId,
    recovery: &RecoveryId,
    limits: StateStoreLimits,
    extra: usize,
) -> Result<(), StateTransactionError> {
    let bytes = total(
        2_560,
        [
            group.as_bytes().len(),
            recovery.as_bytes().len(),
            limits.maximum_temporary_bytes(),
            extra,
        ],
    )?;
    if bytes > limits.workspace_bytes() {
        Err(StateTransactionError::Limit)
    } else {
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
pub(super) fn wire_record_limit(limits: AdmissionLimits) -> Result<usize, StateTransactionError> {
    total(limits.decision_bytes(), [131_072])
}
pub(super) fn engine_pair(key: usize, value: usize) -> Result<(), StateTransactionError> {
    if key > ENGINE_BYTES
        || value > ENGINE_BYTES
        || total(key, [value])? > ENGINE_BYTES + 768 * 1_024 * 1_024
    {
        Err(StateTransactionError::Limit)
    } else {
        Ok(())
    }
}

impl RedbStateTransaction<'_> {
    pub(super) fn step(&self) -> Result<(), StateTransactionError> {
        self.ready()?;
        debit(&self.steps, 1, self.maximum_steps)
    }
    pub(super) fn admit_read(&self, bytes: usize) -> Result<(), StateTransactionError> {
        self.ready()?;
        if self.recovering {
            self.temporary(bytes)
        } else {
            debit(&self.read, bytes, self.maximum_read)
        }
    }
    pub(super) fn temporary(&self, bytes: usize) -> Result<(), StateTransactionError> {
        self.ready()?;
        debit(&self.temporary, bytes, self.maximum_temporary)
    }
    pub(super) fn scope(&self, tenant: &TenantId) -> Result<RequestScope, StateTransactionError> {
        self.scope_bytes(tenant.as_bytes())
    }
    pub(super) fn scope_bytes(&self, tenant: &[u8]) -> Result<RequestScope, StateTransactionError> {
        self.temporary(total(
            512,
            [
                tenant.len(),
                self.group.as_bytes().len(),
                self.recovery.as_bytes().len(),
            ],
        )?)?;
        Ok(RequestScope::new(
            TenantId::new(copy_bytes(tenant)?).map_err(|_| StateTransactionError::Corrupt)?,
            GroupId::new(copy_bytes(self.group.as_bytes())?)
                .map_err(|_| StateTransactionError::Corrupt)?,
            RecoveryId::new(copy_bytes(self.recovery.as_bytes())?)
                .map_err(|_| StateTransactionError::Corrupt)?,
        ))
    }
    pub(super) fn key(
        &self,
        fields: &[&[u8]],
        fixed: usize,
        encode: impl FnOnce(usize) -> Result<Vec<u8>, value::StateValueError>,
    ) -> Result<Vec<u8>, StateTransactionError> {
        self.output_key(fields, fixed, 0, encode)
    }
    pub(super) fn output_key(
        &self,
        fields: &[&[u8]],
        fixed: usize,
        value_bytes: usize,
        encode: impl FnOnce(usize) -> Result<Vec<u8>, value::StateValueError>,
    ) -> Result<Vec<u8>, StateTransactionError> {
        let mut bytes = total(7, [fixed])?;
        for field in fields {
            bytes = total(bytes, [2])?;
            for byte in *field {
                bytes = total(bytes, [if *byte == 0 { 2 } else { 1 }])?;
            }
        }
        engine_pair(bytes, value_bytes)?;
        self.temporary(bytes)?;
        encode(bytes).map_err(value_error)
    }
    pub(super) fn outcome_wire_limit(
        &self,
        tenant: &TenantId,
        request: &RequestId,
    ) -> Result<usize, StateTransactionError> {
        total(
            102,
            [
                tenant.as_bytes().len(),
                self.group.as_bytes().len(),
                self.recovery.as_bytes().len(),
                request.as_bytes().len(),
                self.admission.decision_bytes(),
            ],
        )
    }
    pub(super) fn record_key(
        &self,
        scope: &RequestScope,
        partition: &PartitionId,
        id: &RecordId,
    ) -> Result<Vec<u8>, StateTransactionError> {
        self.key(
            &[
                self.group.as_bytes(),
                self.recovery.as_bytes(),
                scope.tenant().as_bytes(),
                partition.as_bytes(),
                id.as_bytes(),
            ],
            0,
            |cap| key::encode_record_key(scope, partition, id, cap),
        )
    }
    pub(super) fn due_key(
        &self,
        scope: &RequestScope,
        due: DueOrder<'_>,
    ) -> Result<Vec<u8>, StateTransactionError> {
        self.key(
            &[
                self.group.as_bytes(),
                self.recovery.as_bytes(),
                scope.tenant().as_bytes(),
                due.id,
                due.partition_key,
                due.state,
                due.workflow_type,
            ],
            9,
            |cap| key::encode_due_key(scope, due, cap),
        )
    }
    pub(super) fn fifo_key(
        &self,
        scope: &RequestScope,
        lane: LaneKey<'_>,
        member: LaneOrder<'_>,
    ) -> Result<Vec<u8>, StateTransactionError> {
        self.key(
            &[
                self.group.as_bytes(),
                self.recovery.as_bytes(),
                scope.tenant().as_bytes(),
                lane.workflow_type,
                lane.state,
                lane.partition_key,
                member.id(),
            ],
            17,
            |cap| key::encode_fifo_key(scope, lane, member, cap),
        )
    }
    pub(super) fn recovery_row(
        &self,
        key: &[u8],
        value: &[u8],
    ) -> Result<(), StateTransactionError> {
        if self.recovering {
            debit(&self.inspected, 1, self.limits.maximum_recovery_rows())?;
            debit(
                &self.wire,
                total(key.len(), [value.len()])?,
                self.limits.maximum_recovery_bytes(),
            )?;
        }
        Ok(())
    }
    pub(super) fn opened(&self) -> Result<(), StateTransactionError> {
        if self.recovering { self.step() } else { Ok(()) }
    }
}
