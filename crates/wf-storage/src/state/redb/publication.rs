//! Shared prospective primitives for atomic and independent publication.

use super::{
    ROWS,
    admission::{
        engine_pair, entry, storage_error, table_error, total, value_error, wire_record_limit,
    },
    store::RedbStateTransaction,
};
use crate::state::{key, value};
use redb::ReadableTable;
use wf_domain::claim::{DueOrder, LaneKey};
use wf_domain::history::HistoryEntry;
use wf_domain::workflow::WorkflowRecord;
use wf_domain::workflow::admission::record_bytes;
use wf_domain::workflow::indexed::{IndexDelta, IndexRows, OrderingMetadata};
use wf_domain::workflow::request::RequestScope;
use wf_ports::state::{GroupApplyTransaction, StateTransactionError};
use wf_types::identity::{PartitionId, RecordId};

pub(super) fn validate_version(
    tx: &RedbStateTransaction<'_>,
    scope: &RequestScope,
    partition: &PartitionId,
    id: &RecordId,
    expected: Option<u64>,
) -> Result<(), StateTransactionError> {
    tx.step()?;
    let key = tx.record_key(scope, partition, id)?;
    let table = tx.write.open_table(ROWS).map_err(table_error)?;
    let guard = table.get(key.as_slice()).map_err(|e| storage_error(&e))?;
    let current = guard
        .as_ref()
        .map(|guard| {
            value::scan_record_point_profiled(
                guard.value(),
                scope,
                partition,
                id,
                tx.admission,
                wire_record_limit(tx.admission)?,
            )
            .map(|record| record.version())
            .map_err(value_error)
        })
        .transpose()?;
    if current != expected {
        return Err(StateTransactionError::KnownAborted);
    }
    Ok(())
}

pub(super) fn affected_lanes<'r>(
    tx: &RedbStateTransaction<'_>,
    count: usize,
    deltas: impl Iterator<Item = Result<IndexDelta<'r>, StateTransactionError>>,
) -> Result<Vec<LaneKey<'r>>, StateTransactionError> {
    let slots = count.checked_mul(2).ok_or(StateTransactionError::Limit)?;
    tx.temporary(slots.checked_mul(128).ok_or(StateTransactionError::Limit)?)?;
    let mut lanes = Vec::new();
    lanes
        .try_reserve_exact(slots)
        .map_err(|_| StateTransactionError::Allocation)?;
    for delta in deltas {
        let delta = delta?;
        for row in [delta.before, delta.after].into_iter().flatten() {
            if let IndexRows::Fifo { lane, .. } = row {
                tx.temporary(total(
                    0,
                    [
                        lane.workflow_type.len(),
                        lane.state.len(),
                        lane.partition_key.len(),
                    ],
                )?)?;
                lanes.push(lane);
            }
        }
    }
    lanes.sort_unstable();
    lanes.dedup();
    Ok(lanes)
}

pub(super) fn index_delta(
    tx: &RedbStateTransaction<'_>,
    scope: &RequestScope,
    delta: &IndexDelta<'_>,
) -> Result<(), StateTransactionError> {
    if let Some(before) = delta.before {
        index_row(tx, scope, before, false)?;
    }
    if let Some(after) = delta.after {
        index_row(tx, scope, after, true)?;
    }
    Ok(())
}

pub(super) fn write_record(
    tx: &RedbStateTransaction<'_>,
    scope: &RequestScope,
    partition: &PartitionId,
    record: &WorkflowRecord,
    ordering: OrderingMetadata,
) -> Result<(), StateTransactionError> {
    record_bytes(record, tx.admission).map_err(|_| StateTransactionError::Limit)?;
    let size = record_size(scope, partition, record)?;
    let key = tx.output_key(
        &[
            scope.group().as_bytes(),
            scope.recovery().as_bytes(),
            scope.tenant().as_bytes(),
            partition.as_bytes(),
            record.id().as_bytes(),
        ],
        0,
        size,
        |cap| key::encode_record_key(scope, partition, record.id(), cap),
    )?;
    engine_pair(key.len(), size)?;
    tx.temporary(size)?;
    let encoded =
        value::encode_record_fields(scope, partition, record, ordering, tx.admission, size)
            .map_err(value_error)?;
    put(tx, &key, &encoded)
}

pub(super) fn write_history(
    tx: &RedbStateTransaction<'_>,
    scope: &RequestScope,
    partition: &PartitionId,
    id: &RecordId,
    history: &HistoryEntry,
) -> Result<(), StateTransactionError> {
    let size = history_size(scope, partition, id, history)?;
    let key = tx.output_key(
        &[
            scope.group().as_bytes(),
            scope.recovery().as_bytes(),
            scope.tenant().as_bytes(),
            partition.as_bytes(),
            id.as_bytes(),
            history.event_id.as_bytes(),
        ],
        8,
        size,
        |cap| key::encode_history_key(scope, partition, id, history, cap),
    )?;
    engine_pair(key.len(), size)?;
    tx.temporary(size)?;
    let encoded =
        value::encode_history(scope, partition, id, history, size).map_err(value_error)?;
    put(tx, &key, &encoded)
}

pub(super) fn head_due(
    tx: &RedbStateTransaction<'_>,
    scope: &RequestScope,
    lane: LaneKey<'_>,
    insert: bool,
) -> Result<(), StateTransactionError> {
    if let Some(owner) = tx.fifo_head(scope.tenant(), lane)?
        && let Some(due) = DueOrder::for_record(&entry(&owner))
    {
        let key = tx.due_key(scope, due)?;
        if insert {
            put(tx, &key, &[1])?;
        } else {
            remove(tx, &key, &[1])?;
        }
    }
    Ok(())
}

fn index_row(
    tx: &RedbStateTransaction<'_>,
    scope: &RequestScope,
    row: IndexRows<'_>,
    insert: bool,
) -> Result<(), StateTransactionError> {
    let (key, value) = match row {
        IndexRows::Parallel { due } => (tx.due_key(scope, due)?, [0].as_slice()),
        IndexRows::Fifo { lane, member, .. } => (tx.fifo_key(scope, lane, member)?, [].as_slice()),
    };
    if insert {
        put(tx, &key, value)
    } else {
        remove(tx, &key, value)
    }
}

pub(super) fn put(
    tx: &RedbStateTransaction<'_>,
    key: &[u8],
    value: &[u8],
) -> Result<(), StateTransactionError> {
    engine_pair(key.len(), value.len())?;
    tx.step()?;
    let mut table = tx.write.open_table(ROWS).map_err(table_error)?;
    table.insert(key, value).map_err(|e| storage_error(&e))?;
    Ok(())
}

fn remove(
    tx: &RedbStateTransaction<'_>,
    key: &[u8],
    expected: &[u8],
) -> Result<(), StateTransactionError> {
    tx.step()?;
    let mut table = tx.write.open_table(ROWS).map_err(table_error)?;
    tx.step()?;
    let previous = table
        .remove(key)
        .map_err(|e| storage_error(&e))?
        .ok_or(StateTransactionError::Corrupt)?;
    if previous.value() != expected {
        return Err(StateTransactionError::Corrupt);
    }
    Ok(())
}

fn record_size(
    scope: &RequestScope,
    partition: &PartitionId,
    record: &WorkflowRecord,
) -> Result<usize, StateTransactionError> {
    let mut bytes = total(
        114,
        [
            scope.group().as_bytes().len(),
            scope.recovery().as_bytes().len(),
            scope.tenant().as_bytes().len(),
            partition.as_bytes().len(),
            record.id().as_bytes().len(),
            record.workflow_type().len(),
            record.state().len(),
        ],
    )?;
    if record.next_run_at().is_some() {
        bytes = total(bytes, [8])?;
    }
    for value in [
        record.run_state(),
        record.payload(),
        record.result(),
        record.error(),
    ]
    .into_iter()
    .flatten()
    {
        bytes = total(bytes, [4, value.len()])?;
    }
    if let Some(lease) = record.lease() {
        bytes = total(
            bytes,
            [16, lease.worker.len(), lease.token.as_bytes().len()],
        )?;
    }
    Ok(bytes)
}

fn history_size(
    scope: &RequestScope,
    partition: &PartitionId,
    id: &RecordId,
    history: &HistoryEntry,
) -> Result<usize, StateTransactionError> {
    let mut bytes = total(
        78,
        [
            scope.group().as_bytes().len(),
            scope.recovery().as_bytes().len(),
            scope.tenant().as_bytes().len(),
            partition.as_bytes().len(),
            id.as_bytes().len(),
            history.event_id.len(),
            history.state.len(),
        ],
    )?;
    if history.next_run_at.is_some() {
        bytes = total(bytes, [8])?;
    }
    if let Some(worker) = &history.worker {
        bytes = total(bytes, [4, worker.len()])?;
    }
    if let Some(retry) = &history.retry {
        bytes = total(
            bytes,
            [30, retry.run_state.len(), retry.policy.exhausted_to.len()],
        )?;
        if retry.next_run_at.is_some() {
            bytes = total(bytes, [8])?;
        }
    }
    Ok(bytes)
}
