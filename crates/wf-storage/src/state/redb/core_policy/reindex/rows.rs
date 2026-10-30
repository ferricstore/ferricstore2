//! Full reached canonical ownership and exact untouched predecessor index facts.

use super::super::super::{
    admission::{
        copy_bytes, entry, storage_error, table_error, total, value_error, wire_record_limit,
    },
    store::RedbStateTransaction,
};
use crate::state::{key, value};
use redb::ReadableTable;
use std::ops::Bound;
use wf_domain::claim::{DueOrder, LaneKey, LaneOrder};
use wf_domain::policy::definition::StateMode;
use wf_domain::workflow::request::RequestScope;
use wf_ports::state::{GroupApplyTransaction, StateTransactionError, StoredWorkflowRecord};
use wf_types::identity::{PartitionId, RecordId};

pub(super) fn next(
    tx: &RedbStateTransaction<'_>,
    scope: &RequestScope,
    prefix: &[u8],
    previous: Option<&StoredWorkflowRecord>,
) -> Result<Option<StoredWorkflowRecord>, StateTransactionError> {
    let lower = previous
        .map(|p| tx.record_key(scope, &p.partition, p.record.id()))
        .transpose()?;
    let bound = lower.as_deref().unwrap_or(prefix);
    tx.temporary(bound.len())?;
    tx.step()?;
    let table = tx.write.open_table(tx.rows_table).map_err(table_error)?;
    tx.step()?;
    let start = if previous.is_some() {
        Bound::Excluded(bound)
    } else {
        Bound::Included(bound)
    };
    let mut rows = table
        .range::<&[u8]>((start, Bound::Unbounded))
        .map_err(|e| storage_error(&e))?;
    tx.step()?;
    let Some((physical, bytes)) = rows.next().transpose().map_err(|e| storage_error(&e))? else {
        return Ok(None);
    };
    if !physical.value().starts_with(prefix) {
        return Ok(None);
    }
    tx.recovery_row(physical.value(), bytes.value())?;
    tx.temporary(total(physical.value().len(), [bytes.value().len()])?)?;
    let maximum = total(256, [physical.value().len()])?;
    tx.admit_read(maximum)?;
    tx.temporary(maximum)?;
    let decoded = key::decode_state_key(
        physical.value(),
        tx.group,
        tx.recovery,
        physical.value().len(),
        maximum,
    )
    .map_err(value_error)?;
    if decoded.family() != 1 || decoded.part(2) != scope.tenant().as_bytes() {
        return Err(StateTransactionError::Corrupt);
    }
    tx.temporary(total(decoded.part(3).len(), [decoded.part(4).len()])?)?;
    let partition = PartitionId::new(copy_bytes(decoded.part(3))?)
        .map_err(|_| StateTransactionError::Corrupt)?;
    let id =
        RecordId::new(copy_bytes(decoded.part(4))?).map_err(|_| StateTransactionError::Corrupt)?;
    let scanned = value::scan_record_in_layout(
        bytes.value(),
        scope,
        &partition,
        &id,
        tx.admission,
        wire_record_limit(tx.admission)?,
        tx.layout,
    )
    .map_err(value_error)?;
    let n = scanned.owned_bytes();
    tx.admit_read(n)?;
    scanned.into_owned(n).map(Some).map_err(value_error)
}

pub(super) fn tag(
    tx: &RedbStateTransaction<'_>,
    scope: &RequestScope,
    due: DueOrder<'_>,
    expected: Option<u8>,
) -> Result<(), StateTransactionError> {
    let key = tx.due_key(scope, due)?;
    tx.step()?;
    let table = tx.write.open_table(tx.rows_table).map_err(table_error)?;
    tx.step()?;
    let bytes = table.get(key.as_slice()).map_err(|e| storage_error(&e))?;
    match (bytes.as_ref().map(redb::AccessGuard::value), expected) {
        (None, None) => Ok(()),
        (Some([value]), Some(wanted)) if *value == wanted => Ok(()),
        _ => Err(StateTransactionError::Corrupt),
    }
}

pub(super) fn verify(
    tx: &RedbStateTransaction<'_>,
    scope: &RequestScope,
    owner: &StoredWorkflowRecord,
    old: StateMode,
    captured: bool,
) -> Result<(), StateTransactionError> {
    if owner.ordering.priority > 2 {
        return Err(StateTransactionError::Corrupt);
    }
    let record = entry(owner);
    if let Some(lane) = LaneKey::for_record(&record) {
        let member = tx.fifo_member(scope.tenant(), lane, LaneOrder::for_record(&record))?;
        match old {
            StateMode::Fifo if member.as_ref() != Some(owner) || !captured => {
                return Err(StateTransactionError::Corrupt);
            }
            StateMode::Parallel if member.is_some() => return Err(StateTransactionError::Corrupt),
            _ => {}
        }
    }
    if let Some(due) = DueOrder::for_record(&record) {
        tag(
            tx,
            scope,
            due,
            if old == StateMode::Parallel {
                Some(0)
            } else {
                None
            },
        )?;
    }
    Ok(())
}
