//! Explicit represented-core snapshots; source provenance and publication are external.

use wf_domain::history::HistoryEntry;
use wf_domain::workflow::WorkflowRecord;
use wf_domain::workflow::indexed::OrderingMetadata;
use wf_domain::workflow::request::RequestScope;
use wf_ports::state::{StoredWorkflowHistorySnapshot, StoredWorkflowRecord};
use wf_types::identity::{PartitionId, RecordId};

use super::{StateValueError, add};

mod model;
mod scan;
mod stored;
mod write;
pub use model::HistorySnapshotLimits;
pub(crate) use scan::ScannedHistorySnapshot;
pub(crate) use stored::{
    binding_source_fields_in_layout, encode_record_in_layout, scan_record_in_layout,
};

/// Encodes borrowed represented fields without cloning a temporary record owner.
/// # Errors
/// Rejects inconsistent fields, explicit limits, checked sizes or allocation failure.
pub fn encode_history_snapshot_fields(
    scope: &RequestScope,
    partition: &PartitionId,
    record: &WorkflowRecord,
    ordering: OrderingMetadata,
    event: &HistoryEntry,
    limits: HistorySnapshotLimits,
) -> Result<Vec<u8>, StateValueError> {
    write::encode_fields(scope, partition, record, ordering, event, limits)
}

pub(crate) fn history_snapshot_encoded_size(
    scope: &RequestScope,
    partition: &PartitionId,
    record: &WorkflowRecord,
    ordering: OrderingMetadata,
    event: &HistoryEntry,
    limits: HistorySnapshotLimits,
) -> Result<usize, StateValueError> {
    write::measure(scope, partition, record, ordering, event, limits).map(|sizes| sizes.2)
}

pub(crate) fn inspect_history_snapshot<'a>(
    bytes: &'a [u8],
    scope: &RequestScope,
    partition: &PartitionId,
    id: &RecordId,
    admission: wf_domain::workflow::admission::AdmissionLimits,
    maximum_wire_bytes: usize,
) -> Result<ScannedHistorySnapshot<'a>, StateValueError> {
    scan::scan(bytes, scope, partition, id, admission, maximum_wire_bytes)
}

/// Encodes the exact represented postimage and matching event into one owner.
/// # Errors
/// Rejects mismatched fields, explicit limits, size overflow or allocation failure.
pub fn encode_history_snapshot(
    scope: &RequestScope,
    postimage: &StoredWorkflowRecord,
    event: &HistoryEntry,
    limits: HistorySnapshotLimits,
) -> Result<Vec<u8>, StateValueError> {
    write::encode(scope, postimage, event, limits)
}

/// Validates the complete explicit envelope before materializing either owner.
/// # Errors
/// Rejects malformed/foreign fields, explicit limits, overflow or allocation failure.
pub fn decode_history_snapshot(
    bytes: &[u8],
    scope: &RequestScope,
    partition: &PartitionId,
    id: &RecordId,
    limits: HistorySnapshotLimits,
) -> Result<StoredWorkflowHistorySnapshot, StateValueError> {
    let scanned = scan::scan(
        bytes,
        scope,
        partition,
        id,
        limits.admission,
        limits.encoded,
    )?;
    let mut owned = scanned.record.owned_bytes();
    add(&mut owned, scanned.history.owned_bytes()?)?;
    admit(owned, limits.owned)?;
    let mut workspace = bytes.len();
    add(&mut workspace, owned)?;
    add(&mut workspace, 2048)?;
    admit(workspace, limits.workspace)?;
    Ok(StoredWorkflowHistorySnapshot {
        postimage: scanned.record.into_owned(owned)?,
        event: scanned.history.into_owned()?,
    })
}

fn admit(bytes: usize, maximum: usize) -> Result<(), StateValueError> {
    if bytes > maximum {
        return Err(StateValueError::Limit { bytes, maximum });
    }
    Ok(())
}
