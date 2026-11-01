//! Coherent physical record reads; barriers, authorization and visibility are external.

use std::fmt;
use wf_domain::workflow::{control::ControlState, request::RequestScope};
use wf_types::identity::{PartitionId, RecordId};

use super::{StateTransactionError, StoredWorkflowRecord};
use crate::blob::WorkflowValueBinding;

/// Selects one exact physical current record or historical postimage.
#[derive(Clone, Copy, Debug, Default)]
pub struct WorkflowRecordReadQuery<'a> {
    /// Exact canonical event ID; absence selects the current record.
    pub history: Option<&'a str>,
    /// Required same-snapshot group control; disagreement is `KnownAborted`.
    pub expected_control: Option<ControlState>,
}

/// Independent prospective logical limits; engine pages and allocator RSS are excluded.
#[derive(Clone, Copy, Debug)]
pub struct WorkflowRecordReadLimits {
    /// Physical point probes, including absent, manifest and control probes.
    pub maximum_row_reads: usize,
    /// Cumulative inspected physical key and value bytes.
    pub maximum_inspected_bytes: usize,
    /// Complete returned logical owner, including metadata and reference backing.
    pub maximum_returned_bytes: usize,
    /// Continuous conservative work for the whole read.
    pub maximum_work: usize,
    /// Peak simultaneous logical guards, keys, scratch and returned owners.
    pub maximum_temporary_bytes: usize,
}

/// Same-point represented core, actual lifecycle/value references and opaque capture.
/// Metadata has no original typed validity until decoded with the actual context.
pub struct StoredWorkflowRecordCapture {
    /// Complete native represented record and ordering at the selected point.
    pub record: StoredWorkflowRecord,
    /// Actual persisted lifecycle and value provenance for that exact point.
    pub values: Box<WorkflowValueBinding>,
    /// Unchanged complete original metadata operand, independently owned.
    pub metadata: Vec<u8>,
}

/// Independently owned data acquired on one physical committed revision.
/// An absent record is known missing under a supported capture capability.
pub struct WorkflowRecordReadSnapshot {
    /// Actual control observed on the same snapshot as the record and companion.
    pub control: ControlState,
    /// Complete physical observation, without lifecycle or read-barrier filtering.
    pub capture: Option<StoredWorkflowRecordCapture>,
    /// Physical probes consumed, including absent probes.
    pub row_reads: usize,
    /// Physical key and value bytes admitted across all probes.
    pub inspected_bytes: usize,
    /// Complete conservative logical returned ownership.
    pub returned_bytes: usize,
    /// Conservative work consumed across the complete read.
    pub work_units: usize,
    /// Peak conservative simultaneous requested ownership.
    pub temporary_bytes: usize,
}

impl fmt::Debug for StoredWorkflowRecordCapture {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("StoredWorkflowRecordCapture(<redacted>)")
    }
}
impl fmt::Debug for WorkflowRecordReadSnapshot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("WorkflowRecordReadSnapshot(<redacted>)")
    }
}

/// Narrow physical record acquisition, independent of concrete storage formats.
pub trait WorkflowRecordReadPort {
    /// Captures control, core, lifecycle and metadata from one committed snapshot.
    /// Callers separately enforce scope admission, read barriers and visibility.
    /// # Errors
    /// Preserves unsupported profile, control, resource, corruption and I/O failures.
    fn read_workflow_record(
        &self,
        scope: &RequestScope,
        partition: &PartitionId,
        id: &RecordId,
        query: WorkflowRecordReadQuery<'_>,
        limits: WorkflowRecordReadLimits,
    ) -> Result<WorkflowRecordReadSnapshot, StateTransactionError>;
}
