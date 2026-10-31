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
    use crate::governance::release::{
        ReleaseOutboxError, append_release_outbox, validate_release_state,
    };
    validate_release_state(state).map_err(|_| ApplyError::InvalidAuthority)?;
    if count == 0 {
        return Ok(Ok(state));
    }
    match append_release_outbox(state, count) {
        Ok(next) => Ok(Ok(next)),
        Err(ReleaseOutboxError::BacklogFull) => Ok(Err(ActiveTimeoutRejection::ReleaseBacklog)),
        Err(_) => Err(ApplyError::InvalidAuthority),
    }
}

pub(super) fn release_occurrences<'v>(
    state: &[u8],
    reservation: Option<&'v GovernanceReservation>,
) -> Option<&'v GovernanceReservation> {
    (state != b"running").then_some(reservation).flatten()
}
