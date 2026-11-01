//! Regular-value request, configured stage ceilings and independently owned writes.
use super::super::{
    StrongWorkflowRecordError, StrongWorkflowRecordLimits, StrongWorkflowRecordRequest,
    StrongWorkflowRecordResult,
};
use std::fmt;
use wf_domain::{
    history::values::{HistoryValueError, HistoryValueLimits, HistoryValueWrite},
    options::OriginalPayloadReturn,
};
use wf_ports::{
    blob::{
        HistoryValueReadConversionError, HistoryValueReadConversionLimits, OriginalValueBuildError,
        WorkflowValueReadLimits,
    },
    state::StrongWorkflowRecordReadError,
};
/// Exact record selection and already normalized regular-value return policy.
#[derive(Clone, Copy)]
pub struct StrongWorkflowRecordValueRequest<'a> {
    /// Tenant-bound current-record query and scoped client minimum.
    pub record: StrongWorkflowRecordRequest<'a>,
    /// Actual selected public/internal maximum, with no authority inferred.
    pub value_return: OriginalPayloadReturn,
}
/// Independent stage ceilings and whole-call logical ownership/work limits.
#[derive(Clone, Copy)]
pub struct StrongWorkflowRecordValueLimits {
    /// Record acquisition/preparation; clipped to remaining whole-call work/peak.
    pub record: StrongWorkflowRecordLimits,
    /// Actual sized immutable value reads.
    pub values: WorkflowValueReadLimits,
    /// Consuming slot/owner conversion.
    pub conversion: HistoryValueReadConversionLimits,
    /// Shared regular-value collector/projector and reached decoder profile.
    pub projection: HistoryValueLimits,
    /// Final logical record plus exact write/document owners.
    pub maximum_output_bytes: usize,
    /// Whole-call work, including nonrefundable reached-stage ceiling cohorts.
    pub maximum_work_units: usize,
    /// Whole-call conservative logical peak, distinct from allocator/RSS.
    pub maximum_temporary_bytes: usize,
}
/// Typed original base plus owned payload/result/error field writes.
/// Public DTO projection must still apply these writes to the selected base.
pub struct StrongWorkflowRecordValueResult {
    pub(super) record: StrongWorkflowRecordResult,
    pub(super) writes: Vec<HistoryValueWrite>,
    pub(super) work: usize,
    pub(super) temporary: usize,
    pub(super) owned: usize,
}
impl StrongWorkflowRecordValueResult {
    /// Unchanged typed base and authoritative scoped position.
    #[must_use]
    pub const fn record(&self) -> &StrongWorkflowRecordResult {
        &self.record
    }
    /// Ordered writes over entry zero; absent writes leave the base untouched.
    #[must_use]
    pub fn value_writes(&self) -> &[HistoryValueWrite] {
        &self.writes
    }
    /// Parent logical work and reached-stage ceiling cohorts.
    #[must_use]
    pub const fn work_units(&self) -> usize {
        self.work
    }
    /// Conservative peak logical liabilities for the complete call.
    #[must_use]
    pub const fn temporary_bytes(&self) -> usize {
        self.temporary
    }
    /// Final logical owner count after original read buffers drop.
    #[must_use]
    pub const fn owned_bytes(&self) -> usize {
        self.owned
    }
}
impl fmt::Debug for StrongWorkflowRecordValueResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("StrongWorkflowRecordValueResult(<redacted>)")
    }
}
/// No query, authorization or operational error becomes an absent workflow.
#[derive(Debug)]
pub enum StrongWorkflowRecordValueError {
    /// A reached operational ceiling is zero.
    InvalidLimits,
    /// A positive whole-call or checked-arithmetic ceiling was exhausted.
    Limit,
    /// Exact record acquisition/preparation refused.
    Query(StrongWorkflowRecordError),
    /// Current value authorization/clock/scope/storage refused.
    Read(StrongWorkflowRecordReadError),
    /// Native reference cannot be represented by the existing source constructor.
    Reference(OriginalValueBuildError),
    /// Acquired slot/owner observations cannot honestly be projected.
    Conversion(HistoryValueReadConversionError),
    /// Continuous collector/decoder/projector refused.
    Projection(HistoryValueError),
}
impl fmt::Display for StrongWorkflowRecordValueError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}
impl std::error::Error for StrongWorkflowRecordValueError {}
