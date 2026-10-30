//! Pure Step coherence against the independently supplied full control plan.

use wf_domain::workflow::admission::{AdmissionLimits, MAX_BATCH_ITEMS, MAX_INLINE_BYTES};
use wf_domain::workflow::control::ControlPlan;
use wf_domain::workflow::indexed::IndexedDecision;
use wf_domain::workflow::request::{RequestScope, RetainedOutcome, RetainedStatus};

use super::StateTransactionError;
use crate::log::{step_continue_response_matches, step_continue_retained_summary};

pub(super) fn validate(
    scope: &RequestScope,
    control: ControlPlan,
    operation: &IndexedDecision<'_>,
    outcome: &RetainedOutcome,
) -> Result<(), StateTransactionError> {
    let (expected_status, expected_count) = match operation {
        IndexedDecision::Accepted(_) => (RetainedStatus::Accepted, Some(1)),
        IndexedDecision::Rejected(reason) => (RetainedStatus::IndexedRejected(*reason), None),
    };
    let position = control
        .next()
        .last_entry()
        .ok_or(StateTransactionError::InvalidChanges)?
        .log();
    let time = control.execution_time();
    if !matches!(outcome.intent().version(), 10 | 11)
        || outcome.response_version() != 9
        || outcome.scope() != scope
        || outcome.status() != expected_status
        || outcome.original_position() != position
        || outcome.original_time() != time
    {
        return Err(StateTransactionError::InvalidChanges);
    }
    // Structural ceilings do not pay the actual captured transaction profile.
    let limits = AdmissionLimits::new(MAX_INLINE_BYTES, outcome.response().len(), MAX_BATCH_ITEMS)
        .map_err(|_| StateTransactionError::InvalidChanges)?;
    let (status, count) = step_continue_retained_summary(
        outcome.intent(),
        outcome.response_version(),
        outcome.response(),
        position,
        time,
        limits,
    )
    .map_err(|_| StateTransactionError::InvalidChanges)?;
    if status != expected_status
        || count != expected_count
        || !step_continue_response_matches(operation, outcome.response(), position, time, limits)
            .map_err(|_| StateTransactionError::InvalidChanges)?
    {
        return Err(StateTransactionError::InvalidChanges);
    }
    Ok(())
}
