//! One paid row owner, bound to the independently decoded complete physical key.

use super::super::{
    admission::{copy_bytes, storage_error, total, value_error, wire_record_limit},
    store::RedbStateTransaction,
};
use super::index;
use crate::state::{key::DecodedStateKey, value};
use redb::ReadableTable;
use wf_domain::policy::complete::PolicySchemaCharge;
use wf_ports::state::{StateTransactionError, StoredWorkflowRecord};
use wf_types::identity::{PartitionId, RecordId, RequestId};

pub(super) fn validate(
    tx: &RedbStateTransaction<'_>,
    table: &impl ReadableTable<&'static [u8], &'static [u8]>,
    key: &DecodedStateKey,
    bytes: &[u8],
    charge: &mut PolicySchemaCharge,
    (layout, manifest): (u16, &[u8]),
) -> Result<(), StateTransactionError> {
    match key.family() {
        0 => {
            value::decode_control(bytes, tx.group, tx.recovery, 1_024).map_err(value_error)?;
        }
        1 => {
            let owner = record(tx, key, bytes, layout)?;
            index::canonical(tx, table, &owner, charge, layout)?;
        }
        2 => outcome(tx, key, bytes, charge, (layout, manifest))?,
        3 | 4 => index::reverse(tx, table, key, bytes, charge, layout)?,
        5 => history(tx, key, bytes, layout)?,
        6 => {
            let _scope = tx.scope_bytes(key.part(2))?;
            decode_policy(tx, bytes, key.part(3), charge, layout)?;
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
        8..=11 if matches!(layout, 7 | 8) => {}
        _ => return Err(StateTransactionError::Corrupt),
    }
    Ok(())
}

pub(super) fn record(
    tx: &RedbStateTransaction<'_>,
    key: &DecodedStateKey,
    bytes: &[u8],
    layout: u16,
) -> Result<StoredWorkflowRecord, StateTransactionError> {
    let scope = tx.scope_bytes(key.part(2))?;
    tx.temporary(total(key.part(3).len(), [key.part(4).len()])?)?;
    let partition =
        PartitionId::new(copy_bytes(key.part(3))?).map_err(|_| StateTransactionError::Corrupt)?;
    let id = RecordId::new(copy_bytes(key.part(4))?).map_err(|_| StateTransactionError::Corrupt)?;
    let scanned = value::scan_record_in_layout(
        bytes,
        &scope,
        &partition,
        &id,
        tx.admission,
        wire_record_limit(tx.admission)?,
        layout,
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
    charge: &mut PolicySchemaCharge,
    (layout, manifest): (u16, &[u8]),
) -> Result<(), StateTransactionError> {
    let scope = tx.scope_bytes(key.part(2))?;
    tx.temporary(key.part(3).len())?;
    let request =
        RequestId::new(copy_bytes(key.part(3))?).map_err(|_| StateTransactionError::Corrupt)?;
    let maximum = tx.outcome_wire_limit(scope.tenant(), &request)?;
    let validation = value::outcome_policy_validation_bytes_in_layout(bytes, maximum, layout)
        .map_err(value_error)?;
    tx.temporary(validation)?;
    let scanned = value::scan_outcome_in_layout(
        bytes,
        &scope,
        &request,
        tx.admission,
        maximum,
        (tx.limits.schema(), charge, validation),
        layout,
    )
    .map_err(value_error)?;
    if matches!(scanned.intent_version(), 9 | 11 | 12 | 13) {
        tx.temporary(total(
            scanned.owned_bytes(),
            [
                manifest.len(),
                std::mem::size_of::<super::super::values::manifest::ValueManifest>(),
            ],
        )?)?;
        let facts = super::super::values::manifest::parse(manifest, tx.group, tx.recovery)?;
        if facts.alias_format != 2
            || (matches!(scanned.intent_version(), 12 | 13) && facts.credit_shard_count.is_none())
            || (scanned.credit_catalog_required() && !facts.credit_catalog)
        {
            return Err(StateTransactionError::Corrupt);
        }
    }
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
    layout: u16,
) -> Result<(), StateTransactionError> {
    let scope = tx.scope_bytes(key.part(2))?;
    let full = layout == 8 && bytes.starts_with(b"FWHS");
    tx.temporary(total(
        key.part(3).len(),
        [
            key.part(4).len(),
            if full { 2048 } else { 512 },
            bytes.len(),
        ],
    )?)?;
    let partition =
        PartitionId::new(copy_bytes(key.part(3))?).map_err(|_| StateTransactionError::Corrupt)?;
    let id = RecordId::new(copy_bytes(key.part(4))?).map_err(|_| StateTransactionError::Corrupt)?;
    if full {
        let scanned = value::inspect_history_snapshot(
            bytes,
            &scope,
            &partition,
            &id,
            tx.admission,
            bytes.len(),
        )
        .map_err(value_error)?;
        if scanned.event_id() != key.part(5) {
            return Err(StateTransactionError::Corrupt);
        }
        return Ok(());
    }
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
    layout: u16,
) -> Result<wf_ports::state::StoredPolicyValue, StateTransactionError> {
    super::super::original_policy::decode_stored(
        tx,
        bytes,
        workflow_type,
        tx.limits.schema(),
        charge,
        false,
        layout,
    )
}

pub(super) fn lookup_record(
    tx: &RedbStateTransaction<'_>,
    table: &impl ReadableTable<&'static [u8], &'static [u8]>,
    tenant: &[u8],
    partition: &[u8],
    id: &[u8],
    layout: u16,
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
    let scanned = value::scan_record_in_layout(
        guard.value(),
        &scope,
        &partition,
        &id,
        tx.admission,
        wire_record_limit(tx.admission)?,
        layout,
    )
    .map_err(value_error)?;
    let owner = scanned.owned_bytes();
    tx.temporary(owner)?;
    scanned.into_owned(owner).map(Some).map_err(value_error)
}
