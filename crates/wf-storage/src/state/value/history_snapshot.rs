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
mod write;
pub use model::HistorySnapshotLimits;

/// Encodes borrowed represented fields without cloning a temporary record owner.
/// # Errors
/// Currently rejects every input until the selected tests-first body exists.
pub fn encode_history_snapshot_fields(
    _scope: &RequestScope,
    _partition: &PartitionId,
    _record: &WorkflowRecord,
    _ordering: OrderingMetadata,
    _event: &HistoryEntry,
    _limits: HistorySnapshotLimits,
) -> Result<Vec<u8>, StateValueError> {
    Err(StateValueError::UnsupportedVersion)
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
    let scanned = scan::scan(bytes, scope, partition, id, limits)?;
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
