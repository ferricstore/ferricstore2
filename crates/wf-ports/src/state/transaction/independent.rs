//! Pure validation of counted independent changes, without storage authority.

use wf_domain::workflow::indexed::{IndependentIndexedDecision, IndependentIndexedPlan};
use wf_domain::workflow::request::{RequestScope, RetainedOutcome, RetainedStatus};

use super::StateTransactionError;
use crate::log::{
    RecordBatchApplyDecision, independent_record_batch_response_matches,
    public_mutation_response_matches, public_mutation_retained_status,
    record_batch_retained_summary,
};

pub(super) fn validate(
    scope: &RequestScope,
    operation: &IndependentIndexedDecision<'_>,
    outcome: &RetainedOutcome,
) -> Result<(), StateTransactionError> {
    if outcome.intent().version() == 1 {
        if outcome.scope() != scope
            || public_mutation_retained_status(
                outcome.intent(),
                outcome.response_version(),
                outcome.response(),
            )
            .map_err(|_| StateTransactionError::InvalidChanges)?
                != outcome.status()
            || !public_mutation_response_matches(operation, outcome.response())
                .map_err(|_| StateTransactionError::InvalidChanges)?
        {
            return Err(StateTransactionError::InvalidChanges);
        }
        return Ok(());
    }
    let status = match operation {
        IndependentIndexedDecision::Accepted(_) => RetainedStatus::Accepted,
        IndependentIndexedDecision::Rejected(reason) => RetainedStatus::IndexedRejected(*reason),
    };
    if outcome.intent().version() != 6
        || outcome.scope() != scope
        || outcome.status() != status
        || record_batch_retained_summary(
            outcome.intent(),
            outcome.response_version(),
            outcome.response(),
        )
        .map_err(|_| StateTransactionError::InvalidChanges)?
        .0 != status
        || !independent_record_batch_response_matches(operation, outcome.response())
            .map_err(|_| StateTransactionError::InvalidChanges)?
    {
        return Err(StateTransactionError::InvalidChanges);
    }
    Ok(())
}

pub(super) fn business<'plan, 'current>(
    decision: &'plan RecordBatchApplyDecision<'current>,
) -> Option<&'plan IndependentIndexedPlan<'current>> {
    match decision {
        RecordBatchApplyDecision::IndependentFresh {
            operation: IndependentIndexedDecision::Accepted(plan),
            ..
        } => Some(plan),
        _ => None,
    }
}
