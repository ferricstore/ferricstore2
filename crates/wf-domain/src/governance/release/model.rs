//! Shared release values; no remote spend, routing or freshness authority.
use std::fmt;
use wf_types::identity::{PartitionId, RecordId};
use wf_types::version::TimestampMillis;

/// Captured distributed-limit enforcement mode.
#[derive(Clone, Copy, Eq, PartialEq)]
pub enum LimitEnforcement {
    /// Strict global reservation.
    StrictGlobal,
    /// Approximate global reservation.
    ApproximateGlobal,
}
/// Actual locally stored reservation, not remote ownership proof.
#[derive(Clone, Eq, PartialEq)]
pub struct GovernanceReservation {
    /// Actual opaque scope.
    pub scope: Vec<u8>,
    /// Actual numeric reservation shard.
    pub shard_id: u64,
    /// Source reservation identity, at most256 bytes.
    pub reservation_id: Vec<u8>,
    /// Captured enforcement mode.
    pub enforcement: LimitEnforcement,
}
/// Complete local outbox scalar authority; empty is head1/tail0.
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct ReleaseOutboxState {
    /// First pending sequence, possibly `MAX_EXACT+1` after acknowledgment.
    pub head: u64,
    /// Last allocated sequence.
    pub tail: u64,
}
/// Owned same-entry local release intent for later atomic persistence.
pub struct ReleaseIntent {
    /// Actual flow component.
    pub flow_id: RecordId,
    /// Actual partition component.
    pub partition: PartitionId,
    /// Captured scope bytes.
    pub scope: Vec<u8>,
    /// Captured shard.
    pub shard_id: u64,
    /// Actual reservation identity.
    pub reservation_id: Vec<u8>,
    /// Captured enforcement.
    pub enforcement: LimitEnforcement,
    /// Trusted source write time.
    pub created_at: TimestampMillis,
}
/// One appended local row, preserving each source write occurrence.
pub struct ReleaseAppend {
    /// Checked local sequence.
    pub sequence: u64,
    /// Actual owned intent.
    pub intent: ReleaseIntent,
}

/// Complete borrowed intent; validate before encoding or issuing effects.
#[derive(Clone, Copy)]
pub struct ReleaseIntentRef<'a> {
    /// Original opaque workflow ID.
    pub flow_id: &'a [u8],
    /// Original nil or opaque partition, including a present empty value.
    pub partition: Option<&'a [u8]>,
    /// Exact original credit scope.
    pub scope: &'a [u8],
    /// Exact original owner shard.
    pub shard_id: u64,
    /// Original reservation, never a regenerated ID.
    pub reservation_id: &'a [u8],
    /// Captured original enforcement.
    pub enforcement: LimitEnforcement,
    /// Actual originating workflow write time.
    pub created_at: TimestampMillis,
}
impl<'a> From<&'a ReleaseIntent> for ReleaseIntentRef<'a> {
    fn from(intent: &'a ReleaseIntent) -> Self {
        Self {
            flow_id: intent.flow_id.as_bytes(),
            partition: Some(intent.partition.as_bytes()),
            scope: &intent.scope,
            shard_id: intent.shard_id,
            reservation_id: &intent.reservation_id,
            enforcement: intent.enforcement,
            created_at: intent.created_at,
        }
    }
}
/// Pure source diagnoses; no operational authority or commit proof.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReleaseOutboxError {
    /// Invalid scalar meta or exhausted sequence arithmetic.
    Corrupt,
    /// The full pending cohort cannot fit65536 entries.
    BacklogFull,
    /// Expected head differs after the idempotent prefix check.
    StaleCursor,
    /// Acknowledgement exceeds tail or one page.
    InvalidAcknowledgment,
    /// Invalid, duplicate or out-of-tail completion IDs.
    InvalidCompletion,
    /// Complete intent field validation failed.
    InvalidIntent,
}
impl fmt::Display for ReleaseOutboxError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Corrupt => "ERR flow governance release outbox is corrupt",
            Self::BacklogFull => "ERR flow governance release outbox backlog is full",
            Self::StaleCursor => "ERR stale flow governance release outbox cursor",
            Self::InvalidAcknowledgment => {
                "ERR invalid flow governance release outbox acknowledgement"
            }
            Self::InvalidCompletion => "ERR invalid flow governance release outbox completion",
            Self::InvalidIntent => "ERR flow governance release intent is corrupt",
        })
    }
}
impl std::error::Error for ReleaseOutboxError {}
/// Mathematical prefix deletion, with no persisted acknowledgement authority.
pub struct ReleaseAckPlan {
    pub(super) next: ReleaseOutboxState,
    pub(super) range: Option<(u64, u64)>,
}
impl ReleaseAckPlan {
    /// Complete resulting scalar meta.
    #[must_use]
    pub const fn next(&self) -> ReleaseOutboxState {
        self.next
    }
    /// Original ordered prefix; no allocated sequence list.
    pub fn sequences(&self) -> impl Iterator<Item = u64> {
        self.range
            .map(|(first, last)| first..=last)
            .into_iter()
            .flatten()
    }
}
/// Validated bounded original completion cohort; current authority stays external.
pub struct ReleaseCompletionPlan<'a> {
    pub(super) state: ReleaseOutboxState,
    pub(super) sequences: &'a [u64],
}
impl ReleaseCompletionPlan<'_> {
    /// Pending original sequences, in caller order; already acknowledged entries drop.
    pub fn sequences(&self) -> impl Iterator<Item = u64> + '_ {
        self.sequences
            .iter()
            .copied()
            .filter(|n| *n >= self.state.head)
    }
}
/// Supplied source observation, without validating origin or accepted leader reads.
#[derive(Clone, Copy)]
pub enum ReleaseRecordObservation<'a> {
    /// Actual missing status, distinct from a read error.
    Missing,
    /// Decoded map facts; absent state/reservation remain representable.
    Record {
        /// Original physical state, if present as bytes.
        state: Option<&'a [u8]>,
        /// Original governance reservation ID, if present as bytes.
        reservation_id: Option<&'a [u8]>,
    },
    /// Current read could not resolve the record.
    Unavailable,
    /// Invalid decoding or malformed status.
    Corrupt,
}
/// Pure classification only; callers still need actual current authority.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReleaseDisposition {
    /// Original ID is held by a physical running record.
    Retain,
    /// Missing or decoded map no longer holds this running ID.
    Release,
    /// Source observation cannot justify a release.
    ReadError,
}
