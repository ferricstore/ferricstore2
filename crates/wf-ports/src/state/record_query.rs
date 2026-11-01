//! Authorized strong reads; a physical store cannot implement this capability alone.
use super::{
    StateTransactionError, WorkflowRecordPolicyReadLimits, WorkflowRecordPolicyReadSnapshot,
};
use wf_domain::workflow::request::RequestScope;
use wf_types::{
    identity::{GroupId, PartitionId, RecordId, RecoveryId},
    version::{AppliedPosition, LogIdentity, TimestampMillis},
};

/// Stable client write position, meaningful only in its exact group/recovery.
#[derive(Clone, Copy, Debug)]
pub struct WorkflowReadPosition<'a> {
    /// Original group; client hints confer no placement authority.
    pub group: &'a GroupId,
    /// Original recovery; disaster restore cannot silently reinterpret its index.
    pub recovery: &'a RecoveryId,
    /// An applied entry, including actual entry zero rather than empty genesis.
    pub index: AppliedPosition,
}

/// Ordinary observations returned by the authorized strong-read lifecycle.
/// These fields do not independently prove quorum authority.
pub struct StrongWorkflowRecordRead {
    /// Coherent core/metadata/lifecycle/current-policy/control observation.
    pub snapshot: WorkflowRecordPolicyReadSnapshot,
    /// Full current quorum barrier identity; None means no committed entry yet.
    pub barrier: Option<LogIdentity>,
    /// Trusted server visibility time captured by this read, never client time.
    pub now: TimestampMillis,
}

/// Read authority/admission failure, always distinct from a missing workflow.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StrongWorkflowRecordReadError {
    /// Current tenant/query authorization refused.
    Unauthorized,
    /// Current placement or request namespace is invalid.
    InvalidScope,
    /// Client position belongs to another group or recovery.
    IncompatiblePosition,
    /// The current node cannot establish leader authority for this read.
    Redirect,
    /// Quorum, applied wait, clock or read lifecycle is currently unavailable.
    Unavailable,
    /// Shared admission or a read ceiling was exhausted.
    Limit,
    /// Actual physical storage diagnosis, without conversion into absence.
    State(StateTransactionError),
}

/// One authorized, bounded leader/quorum read lifecycle.
/// Implementations must reauthorize the current principal/tenant, validate actual
/// placement/recovery, establish a current leader/quorum barrier, wait for its
/// application and the requested minimum, then read the coherent physical snapshot.
/// The sampled server time and snapshot must belong to that same read attempt.
/// A local committed/applied comparison or an implementation of this trait alone
/// establishes no production quorum guarantee. Never downgrade to a stale read.
pub trait StrongWorkflowRecordReadPort {
    /// Acquire one exact current workflow after current authorization and barrier.
    /// # Errors
    /// Preserve authority, admission, applied-wait, clock and storage failures.
    fn read_strong_workflow_record(
        &mut self,
        scope: &RequestScope,
        partition: &PartitionId,
        id: &RecordId,
        after: Option<WorkflowReadPosition<'_>>,
        limits: WorkflowRecordPolicyReadLimits,
    ) -> Result<StrongWorkflowRecordRead, StrongWorkflowRecordReadError>;
}
