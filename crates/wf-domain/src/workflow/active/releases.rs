//! Ordered local release intents; remote credit/acknowledgment stays outside.

use super::{ActiveTimeoutRejection, ActiveValueError, GovernanceReservation, ReleaseOutboxState};
use crate::workflow::MAX_EXACT_MILLIS;
use crate::workflow::batch::ApplyError;
use wf_types::bounds::MAX_RECORD_ID_BYTES;

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
