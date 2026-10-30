//! Explicit private credit receipts; legacy value selectors remain closed.

use super::{ScannedRetainedOutcome, StateValueError, add, claim_error, encode_fields, with_frame};
use wf_domain::workflow::admission::AdmissionLimits;
use wf_domain::workflow::request::{ClientIntent, RequestScope, RetainedOutcome};
use wf_ports::log::credit_retained_summary;
use wf_types::identity::RequestId;

pub(super) fn encode(
    outcome: &RetainedOutcome,
    maximum: usize,
    admission: AdmissionLimits,
) -> Result<Vec<u8>, StateValueError> {
    let (status, _) = credit_retained_summary(
        outcome.intent(),
        outcome.response_version(),
        outcome.response(),
        admission,
    )
    .map_err(claim_error)?;
    if maximum == 0 || status != outcome.status() {
        return Err(StateValueError::InvalidValue);
    }
    encode_fields(outcome, maximum)
}

pub(super) fn scan<'a>(
    bytes: &'a [u8],
    scope: &RequestScope,
    expected_request: &RequestId,
    admission: AdmissionLimits,
    maximum: usize,
) -> Result<ScannedRetainedOutcome<'a>, StateValueError> {
    with_frame(
        bytes,
        maximum,
        |fields,
         version,
         digest,
         original_position,
         original_time,
         expires_at,
         response_version| {
            if version != 12 || response_version != 10 {
                return Err(StateValueError::UnsupportedVersion);
            }
            let [tenant, group, recovery, request, response] = fields;
            if tenant != scope.tenant().as_bytes()
                || group != scope.group().as_bytes()
                || recovery != scope.recovery().as_bytes()
                || request != expected_request.as_bytes()
                || expires_at <= original_time
            {
                return Err(StateValueError::InvalidValue);
            }
            let intent = ClientIntent::governance_credit(digest);
            let (status, response_count) =
                credit_retained_summary(intent, response_version, response, admission)
                    .map_err(claim_error)?;
            let mut owned_bytes = 512;
            for field in fields {
                add(&mut owned_bytes, field.len())?;
            }
            Ok(ScannedRetainedOutcome {
                tenant,
                group,
                recovery,
                request,
                response,
                intent,
                original_position,
                original_time,
                expires_at,
                response_version,
                status,
                limits: admission,
                owned_bytes,
                response_count,
            })
        },
    )
}
