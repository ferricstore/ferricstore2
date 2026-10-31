//! One bounded original response owner for the selected Claim profile.
use super::{
    apply::{ClaimApplyDecision, ClaimApplyPlan},
    response,
};
use crate::log::{
    DecodedRequest, RecordBatchApplyError,
    governed_claim::{CapturedGovernedClaimPayment, response as paid_response},
    record_batch::{admit, sum},
};
use wf_domain::{
    claim::apply::ClaimDecision,
    workflow::{
        control::ControlPlan,
        request::{RequestValueError, RetainedMetadata, RetainedOutcome},
    },
};
use wf_types::version::LogIdentity;
pub(super) fn retain<'current>(
    (decoded, capture): (DecodedRequest<'_>, Option<CapturedGovernedClaimPayment<'_>>),
    log: LogIdentity,
    control: ControlPlan,
    operation: ClaimDecision<'current>,
    additional_live_bytes: usize,
    workspace_limit: usize,
    outcome_ttl_ms: u64,
) -> Result<ClaimApplyPlan<'current>, RecordBatchApplyError> {
    let encoded_len = decoded.encoded.len();
    let DecodedRequest {
        request,
        isolation,
        state_keys,
        claim_filters,
        value_metadata,
        limits,
        ..
    } = decoded;
    drop(request.operation);
    drop(isolation);
    drop(state_keys);
    drop(claim_filters);
    drop(value_metadata);
    let header_bytes = sum(&[
        request.scope.tenant().as_bytes().len(),
        request.scope.group().as_bytes().len(),
        request.scope.recovery().as_bytes().len(),
        request.request.as_bytes().len(),
    ])?;
    let output_len = match capture {
        Some(capture) => paid_response::measure(&operation, capture.payment(), limits.admission())?,
        None => response::measure(&operation, limits.admission())?,
    };
    admit(
        sum(&[
            encoded_len,
            limits.admission().decision_bytes(),
            output_len,
            header_bytes,
            1_024,
            additional_live_bytes,
        ])?,
        workspace_limit,
    )?;
    let (response, status) = match capture {
        Some(capture) => paid_response::encode(
            &operation,
            capture.payment(),
            limits.admission(),
            output_len,
        )?,
        None => response::encode(&operation, limits.admission(), output_len)?,
    };
    let expires_at = control
        .execution_time()
        .checked_add(outcome_ttl_ms)
        .ok_or(RequestValueError::InvalidExpiry)?;
    let outcome = RetainedOutcome::new(
        RetainedMetadata {
            scope: request.scope,
            request: request.request,
            intent: request.intent,
            original_position: log,
            original_time: control.execution_time(),
            expires_at,
            status,
        },
        if capture.is_some() { 16 } else { 3 },
        response,
        limits.admission(),
    )?;
    Ok(ClaimApplyPlan {
        control,
        decision: ClaimApplyDecision::Fresh {
            operation,
            outcome: Box::new(outcome),
        },
    })
}
