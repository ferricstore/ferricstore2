//! Ordered local release intents; remote credit/acknowledgment stays outside.

use super::{ActiveTimeoutRejection, ActiveValueError};
use crate::workflow::MAX_EXACT_MILLIS;
use crate::workflow::batch::ApplyError;
use wf_types::bounds::MAX_RECORD_ID_BYTES;
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

pub(super) fn validate_reservation(
    reservation: &GovernanceReservation,
) -> Result<usize, ActiveValueError> {
    if reservation.scope.is_empty()
        || reservation.scope.len() > MAX_RECORD_ID_BYTES
        || reservation.reservation_id.is_empty()
        || reservation.reservation_id.len() > 256
        || reservation.shard_id > MAX_EXACT_MILLIS
    {
        return Err(ActiveValueError::InvalidReservation);
    }
    Ok(reservation.scope.len() + reservation.reservation_id.len())
}

pub(super) fn plan_release_append(
    state: ReleaseOutboxState,
    count: u64,
) -> Result<Result<ReleaseOutboxState, ActiveTimeoutRejection>, ApplyError> {
    if state.head == 0
        || state.tail > MAX_EXACT_MILLIS
        || state.head
            > state
                .tail
                .checked_add(1)
                .ok_or(ApplyError::InvalidAuthority)?
    {
        return Err(ApplyError::InvalidAuthority);
    }
    if count == 0 {
        return Ok(Ok(state));
    }
    let next = state
        .tail
        .checked_add(count)
        .filter(|n| *n <= MAX_EXACT_MILLIS)
        .ok_or(ApplyError::InvalidAuthority)?;
    let pending = state.tail.saturating_add(1).saturating_sub(state.head);
    if pending.checked_add(count).is_none_or(|n| n > 65_536) {
        return Ok(Err(ActiveTimeoutRejection::ReleaseBacklog));
    }
    Ok(Ok(ReleaseOutboxState {
        head: state.head,
        tail: next,
    }))
}

pub(super) fn release_occurrences<'v>(
    state: &[u8],
    reservation: Option<&'v GovernanceReservation>,
) -> Option<&'v GovernanceReservation> {
    (state != b"running").then_some(reservation).flatten()
}
