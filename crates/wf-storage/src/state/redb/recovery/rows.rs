//! One paid row owner, bound to the independently decoded complete physical key.

use super::super::{
    admission::{copy_bytes, policy_error, storage_error, total, value_error, wire_record_limit},
    store::RedbStateTransaction,
};
use super::index;
use crate::state::{key::DecodedStateKey, value};
use redb::ReadableTable;
use wf_domain::policy::complete::{
    CompletePolicyValue, PolicySchemaCharge, decode_complete_policy_value_v2,
};
use wf_ports::state::{StateTransactionError, StoredWorkflowRecord};
use wf_types::identity::{PartitionId, RecordId, RequestId};

pub(super) fn validate(
    tx: &RedbStateTransaction<'_>,
    table: &impl ReadableTable<&'static [u8], &'static [u8]>,
    key: &DecodedStateKey,
    bytes: &[u8],
    charge: &mut PolicySchemaCharge,
) -> Result<(), StateTransactionError> {
    match key.family() {
        0 => {
            value::decode_control(bytes, tx.group, tx.recovery, 1_024).map_err(value_error)?;
        }
        1 => {
            let owner = record(tx, key, bytes)?;
            index::canonical(tx, table, &owner, charge)?;
        }
        2 => outcome(tx, key, bytes)?,
        3 | 4 => index::reverse(tx, table, key, bytes, charge)?,
        5 => history(tx, key, bytes)?,
        6 => {
            let _scope = tx.scope_bytes(key.part(2))?;
            decode_policy(tx, bytes, key.part(3), charge)?;
        }
        7 => {
            if key.part(1).is_empty()
                || key.part(1).len() > 65_535
                || bytes.is_empty()
                || bytes.len() > 256
            {
                return Err(StateTransactionError::Corrupt);
            }
        }
        _ => return Err(StateTransactionError::Corrupt),
    }
    Ok(())
}

pub(super) fn record(
    tx: &RedbStateTransaction<'_>,
    key: &DecodedStateKey,
    bytes: &[u8],
) -> Result<StoredWorkflowRecord, StateTransactionError> {
    let scope = tx.scope_bytes(key.part(2))?;
    tx.temporary(total(key.part(3).len(), [key.part(4).len()])?)?;
    let partition =
        PartitionId::new(copy_bytes(key.part(3))?).map_err(|_| StateTransactionError::Corrupt)?;
    let id = RecordId::new(copy_bytes(key.part(4))?).map_err(|_| StateTransactionError::Corrupt)?;
    let scanned = value::scan_record_point_profiled(
        bytes,
        &scope,
        &partition,
        &id,
        tx.admission,
        wire_record_limit(tx.admission)?,
    )
    .map_err(value_error)?;
    let owner = scanned.owned_bytes();
    tx.temporary(owner)?;
    scanned.into_owned(owner).map_err(value_error)
}

fn outcome(
    tx: &RedbStateTransaction<'_>,
    key: &DecodedStateKey,
    bytes: &[u8],
) -> Result<(), StateTransactionError> {
    let scope = tx.scope_bytes(key.part(2))?;
    tx.temporary(key.part(3).len())?;
    let request =
        RequestId::new(copy_bytes(key.part(3))?).map_err(|_| StateTransactionError::Corrupt)?;
    let scanned = value::scan_outcome(
        bytes,
        &scope,
        &request,
        tx.admission,
        tx.outcome_wire_limit(scope.tenant(), &request)?,
    )
    .map_err(value_error)?;
    if scanned
        .response_count()
        .is_some_and(|n| n > tx.admission.batch_items())
    {
        return Err(StateTransactionError::Limit);
    }
    let owner = scanned.owned_bytes();
    tx.temporary(owner)?;
    let _outcome = scanned.into_owned(owner).map_err(value_error)?;
    Ok(())
}

fn history(
    tx: &RedbStateTransaction<'_>,
    key: &DecodedStateKey,
    bytes: &[u8],
) -> Result<(), StateTransactionError> {
    let scope = tx.scope_bytes(key.part(2))?;
    tx.temporary(total(
        key.part(3).len(),
        [key.part(4).len(), 512, bytes.len()],
    )?)?;
    let partition =
        PartitionId::new(copy_bytes(key.part(3))?).map_err(|_| StateTransactionError::Corrupt)?;
    let id = RecordId::new(copy_bytes(key.part(4))?).map_err(|_| StateTransactionError::Corrupt)?;
    let history =
        value::decode_history(bytes, &scope, &partition, &id, bytes.len()).map_err(value_error)?;
    if history.event_id.as_bytes() != key.part(5) {
        return Err(StateTransactionError::Corrupt);
    }
    Ok(())
}

pub(super) fn decode_policy(
    tx: &RedbStateTransaction<'_>,
    bytes: &[u8],
    workflow_type: &[u8],
    charge: &mut PolicySchemaCharge,
) -> Result<CompletePolicyValue, StateTransactionError> {
    let owner = bytes
        .len()
        .checked_mul(216)
        .and_then(|n| n.checked_add(2_048))
        .ok_or(StateTransactionError::Limit)?;
    tx.temporary(owner)?;
    let value =
        decode_complete_policy_value_v2(bytes, tx.limits.schema(), charge).map_err(policy_error)?;
    if value.data().workflow_type != workflow_type {
        return Err(StateTransactionError::Corrupt);
    }
    Ok(value)
}

pub(super) fn lookup_record(
    tx: &RedbStateTransaction<'_>,
    table: &impl ReadableTable<&'static [u8], &'static [u8]>,
    tenant: &[u8],
    partition: &[u8],
    id: &[u8],
) -> Result<Option<StoredWorkflowRecord>, StateTransactionError> {
    let scope = tx.scope_bytes(tenant)?;
    tx.temporary(total(partition.len(), [id.len()])?)?;
    let partition =
        PartitionId::new(copy_bytes(partition)?).map_err(|_| StateTransactionError::Corrupt)?;
    let id = RecordId::new(copy_bytes(id)?).map_err(|_| StateTransactionError::Corrupt)?;
    let key = tx.record_key(&scope, &partition, &id)?;
    tx.step()?;
    let guard = table.get(key.as_slice()).map_err(|e| storage_error(&e))?;
    let Some(guard) = guard else {
        return Ok(None);
    };
    tx.recovery_row(&key, guard.value())?;
    let scanned = value::scan_record_point_profiled(
        guard.value(),
        &scope,
        &partition,
        &id,
        tx.admission,
        wire_record_limit(tx.admission)?,
    )
    .map_err(value_error)?;
    let owner = scanned.owned_bytes();
    tx.temporary(owner)?;
    scanned.into_owned(owner).map(Some).map_err(value_error)
}
