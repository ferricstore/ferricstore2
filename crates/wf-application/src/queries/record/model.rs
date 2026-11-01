//! Owned result and explicit caller scope/consistency/resource inputs.
use std::fmt;
use wf_domain::{
    metadata::scope::OriginalScopeError,
    workflow::{control::ControlState, request::RequestScope},
};
use wf_ports::{
    projection::query_row::{
        NativeQueryPreparationError, NativeQueryPreparationLimits, NativeQueryRecord,
    },
    state::{StrongWorkflowRecordReadError, WorkflowReadPosition, WorkflowRecordPolicyReadLimits},
};
use wf_types::identity::RecordId;

/// Typed query input; server time and authorization are never client fields.
#[derive(Clone, Copy)]
pub struct StrongWorkflowRecordRequest<'a> {
    /// Exact tenant/group/recovery, reauthorized by the supplied capability.
    pub scope: &'a RequestScope,
    /// Exact record identity.
    pub id: &'a RecordId,
    /// Explicit original logical partition.
    pub partition: &'a [u8],
    /// Explicit isolation prefix; None means known unscoped input.
    pub scope_prefix: Option<&'a [u8]>,
    /// Optional stable scoped client write position.
    pub after: Option<WorkflowReadPosition<'a>>,
}

/// Server-owned independent read limits and whole-call logical resource ceilings.
#[derive(Clone, Copy)]
pub struct StrongWorkflowRecordLimits {
    /// Independent native read limits, clipped to remaining parent resources.
    pub read: WorkflowRecordPolicyReadLimits,
    /// The query ceilings cover this entire application call and native preparation.
    pub preparation: NativeQueryPreparationLimits,
    /// Bound all borrowed caller scope/point/position bytes before content traversal.
    pub maximum_input_bytes: usize,
}

/// One authorized read result; expired physical records are not exposed.
pub struct StrongWorkflowRecordResult {
    pub(super) scope: RequestScope,
    pub(super) control: ControlState,
    pub(super) record: Option<NativeQueryRecord>,
    pub(super) work: usize,
    pub(super) temporary: usize,
    pub(super) owned: usize,
}
impl StrongWorkflowRecordResult {
    /// Exact server-bound namespace of this observation.
    #[must_use]
    pub const fn scope(&self) -> &RequestScope {
        &self.scope
    }
    /// Visible current record; None is actual absence or expired terminal retention.
    #[must_use]
    pub const fn record(&self) -> Option<&NativeQueryRecord> {
        self.record.as_ref()
    }
    /// Actual applied position; empty genesis is distinct from applied entry zero.
    #[must_use]
    pub fn position(&self) -> Option<WorkflowReadPosition<'_>> {
        self.control.last_entry().map(|entry| WorkflowReadPosition {
            group: self.scope.group(),
            recovery: self.scope.recovery(),
            index: entry.log().index(),
        })
    }
    /// Complete control from the same physical snapshot as the result.
    #[must_use]
    pub const fn control(&self) -> ControlState {
        self.control
    }
    /// Continuous parent work, including routing and the complete native read.
    #[must_use]
    pub const fn work_units(&self) -> usize {
        self.work
    }
    /// Conservative whole-call peak logical ownership, not allocator/RSS.
    #[must_use]
    pub const fn temporary_bytes(&self) -> usize {
        self.temporary
    }
    /// Conservative returned logical owners, after hidden records drop.
    #[must_use]
    pub const fn owned_bytes(&self) -> usize {
        self.owned
    }
}
impl fmt::Debug for StrongWorkflowRecordResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("StrongWorkflowRecordResult(<redacted>)")
    }
}

/// No authority or operational failure is converted to a missing workflow.
#[derive(Debug)]
pub enum StrongWorkflowRecordError {
    /// A required resource ceiling is zero.
    InvalidLimits,
    /// Whole-call resource ceiling or checked arithmetic overflow.
    Limit,
    /// A fallible final owner reservation failed.
    Allocation,
    /// Client position belongs to another group or recovery.
    IncompatiblePosition,
    /// Returned state does not include the required applied position.
    Behind,
    /// Returned barrier, physical point or captured scope contradicts the request.
    InvalidObservation,
    /// The authorized read lifecycle refused; never absence.
    Read(StrongWorkflowRecordReadError),
    /// Original scope/routing preparation refused.
    Scope(OriginalScopeError),
    /// Native metadata/policy/query preparation refused.
    Preparation(NativeQueryPreparationError),
}
impl fmt::Display for StrongWorkflowRecordError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}
impl std::error::Error for StrongWorkflowRecordError {}
