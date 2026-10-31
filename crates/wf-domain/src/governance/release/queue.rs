//! Complete scalar/cohort preflight before a mathematical outbox decision.
use super::{
    ReleaseAckPlan, ReleaseCompletionPlan, ReleaseDisposition, ReleaseIntentRef,
    ReleaseOutboxError as E, ReleaseOutboxState, ReleaseRecordObservation,
};
/// Validates source finite scalar meta, including the exhausted empty state.
/// # Errors
/// Refuses malformed head/tail or unsupported numeric values.
pub fn validate_release_state(state: ReleaseOutboxState) -> Result<(), E> {
    let _ = state;
    Err(E::Corrupt)
}
/// Plans one complete nonempty append, without allocating IDs or changing state.
/// # Errors
/// Refuses corrupt/exhausted arithmetic or the pending-entry ceiling.
pub fn append_release_outbox(
    state: ReleaseOutboxState,
    count: u64,
) -> Result<ReleaseOutboxState, E> {
    let _ = (state, count);
    Err(E::Corrupt)
}
/// Plans at most256 prefix deletions, preserving idempotent retries before stale checks.
/// # Errors
/// Refuses corrupt meta/arguments, stale head, or a tail/page overrun.
pub fn acknowledge_release_outbox(
    state: ReleaseOutboxState,
    expected_head: u64,
    up_to: u64,
) -> Result<ReleaseAckPlan, E> {
    let _ = (state, expected_head, up_to);
    Err(E::Corrupt)
}
/// Validates the whole original completion cohort before filtering old IDs.
/// # Errors
/// Refuses malformed meta or invalid/duplicate/out-of-tail/oversized sequences.
pub fn prepare_release_completion(
    state: ReleaseOutboxState,
    sequences: &[u64],
) -> Result<ReleaseCompletionPlan<'_>, E> {
    let _ = (state, sequences);
    Err(E::Corrupt)
}
/// Checks complete borrowed source intent fields, without owning payload bytes.
/// # Errors
/// Refuses missing/oversized IDs, scope/partition or nonfinite scalars.
pub fn validate_release_intent(intent: ReleaseIntentRef<'_>) -> Result<(), E> {
    let _ = intent;
    Err(E::InvalidIntent)
}
/// Classifies supplied decoded-map facts, without granting read or release authority.
#[must_use]
pub fn classify_release(
    reservation_id: &[u8],
    observation: ReleaseRecordObservation<'_>,
) -> ReleaseDisposition {
    let _ = (reservation_id, observation);
    ReleaseDisposition::ReadError
}
