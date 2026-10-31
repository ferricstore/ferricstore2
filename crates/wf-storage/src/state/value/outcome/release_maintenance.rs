//! Explicit layout8 maintenance receipts; scalar and old profiles stay closed.
use super::{
    ScannedRetainedOutcome, StateValueError as E, add, claim_error, encode_fields, with_frame,
};
use wf_domain::workflow::{
    admission::AdmissionLimits,
    request::{ClientIntent, RequestScope, RetainedOutcome},
};
use wf_ports::log::release_maintenance_retained_summary;
use wf_types::identity::RequestId;

pub(super) fn encode(
    outcome: &RetainedOutcome,
    maximum: usize,
    admission: AdmissionLimits,
) -> Result<Vec<u8>, E> {
    let (status, _) = release_maintenance_retained_summary(
        outcome.intent(),
        outcome.response_version(),
        outcome.response(),
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
            if version != 18 || response_version != 15 {
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
            let intent = ClientIntent::workflow_release_maintenance(digest);
            let (status, response_count) =
                release_maintenance_retained_summary(intent, response_version, response, admission)
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
                response_version,
                status,
                limits: admission,
                owned_bytes,
                response_count,
            })
        },
    )
}
