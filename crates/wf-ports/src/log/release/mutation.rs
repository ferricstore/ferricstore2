//! Release-capacity business rejection before immutable mutation companions.
use crate::log::{
    CodecError, RecordBatchApplyDecision as D, RecordBatchApplyError, RecordBatchApplyPlan,
};
use wf_domain::workflow::{
    admission::AdmissionLimits,
    indexed::{IndependentIndexedDecision, IndexRejection, IndexedDecision},
    request::{RetainedOutcome, RetainedStatus},
};

/// Replaces accepted final writes with one original whole-command rejection.
/// This mathematical transformation proves neither queue fullness nor commit.
/// # Errors
/// Refuses other profiles/decisions, output allocation or retention admission.
pub fn reject_workflow_release_backlog(
    mut plan: RecordBatchApplyPlan<'_>,
    limits: AdmissionLimits,
) -> Result<RecordBatchApplyPlan<'_>, RecordBatchApplyError> {
    let reason = IndexRejection::ReleaseBacklogFull;
    let (independent, outcome) = match plan.decision {
        D::Fresh {
            operation: IndexedDecision::Accepted(_),
            outcome,
        } => (false, outcome),
        D::IndependentFresh {
            operation: IndependentIndexedDecision::Accepted(_),
            outcome,
        } => (true, outcome),
        _ => return Err(CodecError::UnsupportedTag.into()),
    };
    if outcome.intent().version() != 9 || !matches!(outcome.response_version(), 5 | 6 | 8) {
        return Err(CodecError::UnsupportedVersion.into());
    }
    let (mut metadata, version, old) = outcome.into_parts();
    drop(old);
    metadata.status = RetainedStatus::IndexedRejected(reason);
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(3)
        .map_err(|_| CodecError::Allocation)?;
    super::super::record_batch::push_rejection(&mut bytes, reason)?;
    let outcome = Box::new(RetainedOutcome::new(metadata, version, bytes, limits)?);
    plan.decision = if independent {
        D::IndependentFresh {
            operation: IndependentIndexedDecision::Rejected(reason),
            outcome,
        }
    } else {
        D::Fresh {
            operation: IndexedDecision::Rejected(reason),
            outcome,
        }
    };
    plan.value_effects = None;
    Ok(plan)
}
