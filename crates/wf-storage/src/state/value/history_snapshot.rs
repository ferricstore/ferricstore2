//! Explicit represented-core snapshots; source provenance and publication are external.

use wf_domain::history::HistoryEntry;
use wf_domain::workflow::request::RequestScope;
use wf_ports::state::{StoredWorkflowHistorySnapshot, StoredWorkflowRecord};
use wf_types::identity::{PartitionId, RecordId};

use super::StateValueError;

mod model;
pub use model::HistorySnapshotLimits;

/// Encodes the exact represented postimage and matching event into one owner.
/// # Errors
/// Currently refuses every input until the selected tests-first body exists.
pub fn encode_history_snapshot(
    _scope: &RequestScope,
    _postimage: &StoredWorkflowRecord,
    _event: &HistoryEntry,
    _limits: HistorySnapshotLimits,
) -> Result<Vec<u8>, StateValueError> {
    Err(StateValueError::UnsupportedVersion)
}

/// Validates the complete explicit envelope before materializing either owner.
/// # Errors
/// Currently refuses every input until the selected tests-first body exists.
pub fn decode_history_snapshot(
    _bytes: &[u8],
    _scope: &RequestScope,
    _partition: &PartitionId,
    _id: &RecordId,
    _limits: HistorySnapshotLimits,
) -> Result<StoredWorkflowHistorySnapshot, StateValueError> {
    Err(StateValueError::UnsupportedVersion)
}
