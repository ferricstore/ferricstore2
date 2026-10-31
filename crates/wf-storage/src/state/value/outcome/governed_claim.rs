//! Paid receipts are selected only by layout8; scalar codecs remain closed.
use super::{
    ScannedRetainedOutcome, StateValueError as E, add, claim_error, encode_fields, with_frame,
};
use wf_domain::workflow::{
    admission::AdmissionLimits,
    request::{ClientIntent, RequestScope, RetainedOutcome},
};
use wf_ports::log::{atomic_claim_retained_summary, governed_claim_retained_summary};
use wf_types::identity::RequestId;

pub(super) fn encode(
    outcome: &RetainedOutcome,
    maximum: usize,
    admission: AdmissionLimits,
) -> Result<Vec<u8>, E> {
    let summary = if outcome.intent().version() == 20 {
        atomic_claim_retained_summary
    } else {
        governed_claim_retained_summary
    };
    let (status, _) = summary(
        outcome.intent(),
        outcome.response_version(),
        outcome.response(),
        outcome.original_time(),
        admission,
    )
    .map_err(claim_error)?;
    if maximum == 0 || status != outcome.status() {
        return Err(E::InvalidValue);
    }
    encode_fields(outcome, maximum)
}

pub(super) fn scan<'a>(
    bytes: &'a [u8],
    scope: &RequestScope,
    request: &RequestId,
    admission: AdmissionLimits,
    maximum: usize,
) -> Result<ScannedRetainedOutcome<'a>, E> {
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
            if !matches!((version, response_version), (19, 16) | (20, 17)) {
                return Err(E::UnsupportedVersion);
            }
            let [tenant, group, recovery, original_request, response] = fields;
            if tenant != scope.tenant().as_bytes()
                || group != scope.group().as_bytes()
                || recovery != scope.recovery().as_bytes()
                || original_request != request.as_bytes()
                || expires_at <= original_time
            {
                return Err(E::InvalidValue);
            }
            let intent = if version == 20 {
                ClientIntent::atomic_claim(digest)
            } else {
                ClientIntent::governed_claim(digest)
            };
            let summary = if version == 20 {
                atomic_claim_retained_summary
            } else {
                governed_claim_retained_summary
            };
            let (status, response_count) =
                summary(intent, response_version, response, original_time, admission)
                    .map_err(claim_error)?;
            let mut owned_bytes = 512;
            for field in fields {
                add(&mut owned_bytes, field.len())?;
            }
            Ok(ScannedRetainedOutcome {
                tenant,
                group,
                recovery,
                request: original_request,
                response,
                intent,
                original_position,
                original_time,
                expires_at,
                status,
                response_version,
                limits: admission,
                owned_bytes,
                response_count,
            })
        },
    )
}
