//! One new history observation per accepted record mutation.

use wf_types::version::{Fence, TimestampMillis};

use crate::policy::RetryPolicy;
use crate::retry::RetryDecision;

pub mod metadata;
pub mod original;
pub mod values;

/// The nine source retry metadata observations, retained as typed values.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RetryHistory {
    /// Scheduled or exhausted.
    pub decision: RetryDecision,
    /// Logical run state before retry, independent of the post-retry target.
    pub run_state: Vec<u8>,
    /// Resolved post-retry due time; absent for terminal exhaustion.
    pub next_run_at: Option<TimestampMillis>,
    /// Effective limit, four backoff fields and exhaustion target.
    pub policy: RetryPolicy,
}

/// Supported source event names; duplicate commands emit no second event.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EventKind {
    /// New record creation.
    Created,
    /// First claim or expired running-record reclaim.
    Claimed,
    /// Dedicated nonterminal state transition.
    Transitioned,
    /// A running lease continued into a logical step.
    StepContinued,
    /// Successful terminal effect.
    Completed,
    /// Failed terminal effect.
    Failed,
    /// Scheduled or exhausted retry.
    Retry,
    /// Existing lease deadline replaced.
    LeaseExtended,
    /// Explicit cancellation.
    Cancelled,
    /// A completed child updated its parent accounting.
    ChildCompleted,
    /// A failed child updated its parent accounting.
    ChildFailed,
    /// A cancelled child updated its parent accounting.
    ChildCancelled,
    /// The parent recorded closure of its remaining children.
    ChildrenCancelled,
    /// Explicit restored witness of successful child creation.
    ChildrenSpawned,
}

impl EventKind {
    /// Returns the exact source event label.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Created => "created",
            Self::Claimed => "claimed",
            Self::Transitioned => "transitioned",
            Self::StepContinued => "step_continued",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Retry => "retry",
            Self::LeaseExtended => "lease_extended",
            Self::Cancelled => "cancelled",
            Self::ChildCompleted => "child_completed",
            Self::ChildFailed => "child_failed",
            Self::ChildCancelled => "child_cancelled",
            Self::ChildrenCancelled => "children_cancelled",
            Self::ChildrenSpawned => "children_spawned",
        }
    }
}

/// Reduced history snapshot; physical keys, encoding and retention are omitted.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HistoryEntry {
    /// Source-compatible `milliseconds-version` identity.
    pub event_id: String,
    /// Mutation event label.
    pub kind: EventKind,
    /// Explicit committed event time (distinct from event-ID ordering time).
    pub occurred_at: TimestampMillis,
    /// New record version.
    pub version: u64,
    /// Preserved or newly minted fencing generation.
    pub fence: Fence,
    /// Retry-attempt count after this event.
    pub attempts: u64,
    /// Lifecycle state after this event.
    pub state: Vec<u8>,
    /// Stored due or lease deadline; dedicated terminal effects clear it.
    pub next_run_at: Option<TimestampMillis>,
    /// Source-compatible zero when no lease exists.
    pub lease_deadline: TimestampMillis,
    /// Worker identity when the post-event record owns a lease.
    pub worker: Option<Vec<u8>>,
    /// Source retry audit values when installed/default policy was resolved.
    pub retry: Option<RetryHistory>,
}

/// The bounded authoritative event witness used by legacy retry guards.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct LatestEvent {
    pub(crate) kind: EventKind,
    pub(crate) version: u64,
    pub(crate) fence: Fence,
}
