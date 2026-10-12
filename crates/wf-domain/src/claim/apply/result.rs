//! Owned touched-record plans and opaque snapshot-specific continuation metadata.

use std::fmt;

use wf_types::version::TimestampMillis;

use crate::claim::{SelectionContinuation, SelectionStop};
use crate::workflow::admission::AdmissionLimits;
use crate::workflow::indexed::IndexedBatchPlan;
use crate::workflow::request::RequestScope;

use super::{ClaimApplyView, ClaimEnvelope, ClaimQuery, ClaimWorkLimits};

/// Complete fresh semantic decision; operational failures have no value here.
#[derive(Debug)]
pub enum ClaimDecision<'a> {
    /// Complete atomic touched-record/index/history plan, possibly empty.
    Accepted(ClaimPlan<'a>),
    /// Deterministic business rejection with no partial write plan.
    Rejected(ClaimRejection),
}

/// Touched-only write owners and explicit selection stop accounting.
#[derive(Debug)]
pub struct ClaimPlan<'a> {
    batch: Option<IndexedBatchPlan<'a>>,
    scanned: usize,
    head_reads: usize,
    stop: SelectionStop,
    resume: ClaimResume<'a>,
}
impl<'a> ClaimPlan<'a> {
    /// Borrows the complete record/history/index plan; empty acceptance has none.
    #[must_use]
    pub const fn batch(&self) -> Option<&IndexedBatchPlan<'a>> {
        self.batch.as_ref()
    }
    /// Reads merged candidates examined under the explicit scan budget.
    #[must_use]
    pub const fn scanned(&self) -> usize {
        self.scanned
    }
    /// Reads independent lane-head lookups, including empty lanes.
    #[must_use]
    pub const fn head_reads(&self) -> usize {
        self.head_reads
    }
    /// Distinguishes completed empty selection from bounded partial selection.
    #[must_use]
    pub const fn stop(&self) -> SelectionStop {
        self.stop
    }
    /// Borrows opaque continuation metadata; no public constructor grants authority.
    #[must_use]
    pub const fn resume(&self) -> &ClaimResume<'a> {
        &self.resume
    }
    pub(super) const fn new(
        batch: Option<IndexedBatchPlan<'a>>,
        scanned: usize,
        head_reads: usize,
        stop: SelectionStop,
        resume: ClaimResume<'a>,
    ) -> Self {
        Self {
            batch,
            scanned,
            head_reads,
            stop,
            resume,
        }
    }
}

#[derive(Debug)]
pub(super) enum ClaimResumeKind<'a> {
    Complete,
    SameSnapshot(Box<ClaimResumeBinding<'a>>),
    RestartFromCurrent,
}

#[derive(Debug)]
pub(super) struct ClaimResumeBinding<'a> {
    pub(super) scope: &'a RequestScope,
    pub(super) query: ClaimQuery<'a>,
    pub(super) token_version: u16,
    pub(super) now: TimestampMillis,
    pub(super) revision: u64,
    pub(super) scan_budget: usize,
    pub(super) head_budget: usize,
    pub(super) limits: AdmissionLimits,
    pub(super) work: ClaimWorkLimits,
    pub(super) cursor: SelectionContinuation<'a>,
}

/// Borrowed private cursor, valid only for an unchanged zero-write snapshot.
///
/// A mutating partial result requires a fresh request and current-index restart.
/// This value is not a serialized durable client cursor or apply certificate.
#[derive(Debug)]
pub struct ClaimResume<'a> {
    pub(super) kind: ClaimResumeKind<'a>,
}
impl<'a> ClaimResume<'a> {
    pub(super) const fn complete() -> Self {
        Self {
            kind: ClaimResumeKind::Complete,
        }
    }
    pub(super) const fn restart_from_current() -> Self {
        Self {
            kind: ClaimResumeKind::RestartFromCurrent,
        }
    }
    pub(super) fn same_snapshot(
        view: ClaimApplyView<'a>,
        envelope: &ClaimEnvelope<'a>,
        now: TimestampMillis,
        limits: AdmissionLimits,
        work: ClaimWorkLimits,
        cursor: &SelectionContinuation<'a>,
    ) -> Self {
        Self {
            kind: ClaimResumeKind::SameSnapshot(Box::new(ClaimResumeBinding {
                scope: view.gate.scope,
                query: envelope.query,
                token_version: envelope.token_version,
                now,
                revision: view.revision,
                scan_budget: envelope.scan_budget,
                head_budget: envelope.head_budget,
                limits,
                work,
                cursor: *cursor,
            })),
        }
    }
}

/// Business-only retained claim status; index/canonical/host failures are excluded.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClaimRejection {
    /// Invalid or duplicated query scope/filter.
    InvalidScope,
    /// Caller priority is outside zero through two.
    InvalidPriority,
    /// Unsupported initial selection/scan/head count profile.
    InvalidBudget,
    /// The complete relevant FIFO view exceeds the head budget.
    HeadBudget,
    /// Opaque cursor does not match every effective snapshot/query input.
    StaleContinuation,
    /// Workflow type is empty.
    EmptyType,
    /// Binary worker is empty.
    EmptyWorker,
    /// Lease duration is zero.
    ZeroLease,
    /// Lease duration exceeds the source scalar ceiling.
    LeaseTooLarge,
    /// Effective time plus duration exceeds the source deadline ceiling.
    LeaseDeadlineTooLarge,
    /// Canonical record version cannot advance.
    VersionOverflow,
    /// Canonical fence cannot advance.
    FenceOverflow,
    /// Actual query/generated value exceeds an immutable profile.
    ValueTooLarge {
        /// Actual byte length.
        bytes: usize,
        /// Selected immutable maximum.
        maximum: usize,
    },
    /// Aggregate prepared/plan bytes exceed the selected profile.
    DecisionBytes,
    /// Checked length arithmetic cannot be represented.
    LengthOverflow,
    /// Represented read/work plus plan charge exceeds the trusted work profile.
    WorkBytes {
        /// Actual prospective represented charge.
        bytes: u64,
        /// Separately supplied immutable maximum.
        maximum: u64,
    },
}
impl fmt::Display for ClaimRejection {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ValueTooLarge { bytes, maximum } => formatter.write_str(&format!(
                "ERR value too large ({bytes} bytes, max {maximum} bytes)")),
            Self::WorkBytes { bytes, maximum } => formatter.write_str(&format!(
                "claim represented work exceeds immutable budget ({bytes} bytes, max {maximum} bytes)")),
            _ => formatter.write_str(match self {
                Self::InvalidScope => "invalid claim selection scope",
                Self::InvalidPriority => "claim priority must be between 0 and 2",
                Self::InvalidBudget => "claim selection budget exceeds supported bounds",
                Self::HeadBudget => "claim selection requires more authoritative lane heads",
                Self::StaleContinuation => "claim selection continuation does not match this snapshot/request",
                Self::EmptyType => "ERR flow type must be a non-empty string",
                Self::EmptyWorker => "ERR flow worker must be a non-empty string",
                Self::ZeroLease => "ERR flow lease_ms must be a positive integer",
                Self::LeaseTooLarge => "ERR flow lease_ms exceeds maximum 9007199254740991",
                Self::LeaseDeadlineTooLarge => "ERR flow lease_ms deadline exceeds maximum 9007199254740991",
                Self::VersionOverflow => "ERR flow version overflow",
                Self::FenceOverflow => "ERR flow fencing token overflow",
                Self::DecisionBytes => "workflow decision byte budget exceeded",
                Self::LengthOverflow => "workflow byte length overflow",
                Self::ValueTooLarge { .. } | Self::WorkBytes { .. } => "claim byte budget exceeded",
            }),
        }
    }
}
impl std::error::Error for ClaimRejection {}
