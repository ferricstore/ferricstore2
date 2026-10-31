//! Complete scalar/cohort preflight before a mathematical outbox decision.
use super::{
    ReleaseAckPlan, ReleaseCompletionPlan, ReleaseDisposition, ReleaseIntentRef,
    ReleaseOutboxError as E, ReleaseOutboxState, ReleaseRecordObservation,
};
use crate::workflow::MAX_EXACT_MILLIS as MAX;
use wf_types::bounds::MAX_RECORD_ID_BYTES as MAX_KEY;
/// Validates source finite scalar meta, including the exhausted empty state.
/// # Errors
/// Refuses malformed head/tail or unsupported numeric values.
pub fn validate_release_state(state: ReleaseOutboxState) -> Result<(), E> {
    if state.head == 0 || state.tail > MAX || state.head > state.tail + 1 {
        Err(E::Corrupt)
    } else {
        Ok(())
    }
}
/// Plans one complete nonempty append, without allocating IDs or changing state.
/// # Errors
/// Refuses corrupt/exhausted arithmetic or the pending-entry ceiling.
pub fn append_release_outbox(
    state: ReleaseOutboxState,
    count: u64,
) -> Result<ReleaseOutboxState, E> {
    validate_release_state(state)?;
    let tail = state
        .tail
        .checked_add(count)
        .filter(|n| *n <= MAX)
        .ok_or(E::Corrupt)?;
    if count == 0 {
        return Err(E::Corrupt);
    }
    let pending = state.tail + 1 - state.head;
    if pending.checked_add(count).is_none_or(|n| n > 65_536) {
        return Err(E::BacklogFull);
    }
    Ok(ReleaseOutboxState {
        head: state.head,
        tail,
    })
}
/// Plans at most256 prefix deletions, preserving idempotent retries before stale checks.
/// # Errors
/// Refuses corrupt meta/arguments, stale head, or a tail/page overrun.
pub fn acknowledge_release_outbox(
    state: ReleaseOutboxState,
    expected_head: u64,
    up_to: u64,
) -> Result<ReleaseAckPlan, E> {
    validate_release_state(state)?;
    if expected_head == 0 || up_to == 0 {
        return Err(E::Corrupt);
    }
    if up_to < state.head {
        return Ok(ReleaseAckPlan {
            next: state,
            range: None,
        });
    }
    if expected_head != state.head {
        return Err(E::StaleCursor);
    }
    if up_to > state.tail || up_to - state.head >= 256 {
        return Err(E::InvalidAcknowledgment);
    }
    Ok(ReleaseAckPlan {
        next: ReleaseOutboxState {
            head: up_to + 1,
            tail: state.tail,
        },
        range: Some((state.head, up_to)),
    })
}
/// Validates the whole original completion cohort before filtering old IDs.
/// # Errors
/// Refuses malformed meta or invalid/duplicate/out-of-tail/oversized sequences.
pub fn prepare_release_completion(
    state: ReleaseOutboxState,
    sequences: &[u64],
) -> Result<ReleaseCompletionPlan<'_>, E> {
    if sequences.is_empty() || sequences.len() > 256 || sequences.contains(&0) {
        return Err(E::InvalidCompletion);
    }
    let mut ordered = [0_u64; 256];
    let original = &mut ordered[..sequences.len()];
    original.copy_from_slice(sequences);
    original.sort_unstable();
    if original.windows(2).any(|pair| pair[0] == pair[1]) {
        return Err(E::InvalidCompletion);
    }
    validate_release_state(state)?;
    if sequences.iter().any(|n| *n > state.tail) {
        return Err(E::InvalidCompletion);
    }
    Ok(ReleaseCompletionPlan { state, sequences })
}
/// Checks complete borrowed source intent fields, without owning payload bytes.
/// # Errors
/// Refuses missing/oversized IDs, scope/partition or nonfinite scalars.
pub fn validate_release_intent(intent: ReleaseIntentRef<'_>) -> Result<(), E> {
    if [intent.flow_id, intent.scope]
        .iter()
        .any(|v| v.is_empty() || v.len() > MAX_KEY)
        || intent.partition.is_some_and(|p| p.len() > MAX_KEY)
        || intent.reservation_id.is_empty()
        || intent.reservation_id.len() > 256
        || intent.shard_id > MAX
        || intent.created_at.get() > MAX
    {
        Err(E::InvalidIntent)
    } else {
        Ok(())
    }
}
/// Classifies supplied decoded-map facts, without granting read or release authority.
#[must_use]
pub fn classify_release(
    reservation_id: &[u8],
    observation: ReleaseRecordObservation<'_>,
) -> ReleaseDisposition {
    match observation {
        ReleaseRecordObservation::Record {
            state: Some(b"running"),
            reservation_id: Some(held),
        } if held == reservation_id => ReleaseDisposition::Retain,
        ReleaseRecordObservation::Missing | ReleaseRecordObservation::Record { .. } => {
            ReleaseDisposition::Release
        }
        ReleaseRecordObservation::Unavailable | ReleaseRecordObservation::Corrupt => {
            ReleaseDisposition::ReadError
        }
    }
}
