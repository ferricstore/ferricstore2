//! Explicit Step retained data; generic store profiles remain closed.

use wf_domain::policy::complete::{PolicySchemaCharge, PolicySchemaLimits};
use wf_domain::workflow::admission::AdmissionLimits;
use wf_domain::workflow::request::{ClientIntent, RequestScope, RetainedOutcome, RetainedStatus};
use wf_ports::log::step_continue_retained_summary;
use wf_types::identity::RequestId;

use super::{ScannedRetainedOutcome, StateValueError, add, claim_error, encode_fields, with_frame};

pub(crate) fn scan_outcome_in_layout<'a>(
    bytes: &'a [u8],
    scope: &RequestScope,
    request: &RequestId,
    admission: AdmissionLimits,
    maximum: usize,
    profile: (PolicySchemaLimits, &mut PolicySchemaCharge, usize),
    layout: u16,
) -> Result<ScannedRetainedOutcome<'a>, StateValueError> {
    if layout == 8 && with_frame(bytes, maximum, |_, intent, _, _, _, _, _| Ok(intent == 18))? {
        return super::release_maintenance::scan(bytes, scope, request, admission, maximum);
    }
    if layout == 8
        && with_frame(bytes, maximum, |_, intent, _, _, _, _, _| {
            Ok(matches!(intent, 19 | 20))
        })?
    {
        return super::governed_claim::scan(bytes, scope, request, admission, maximum);
    }
    if layout == 8
        && with_frame(bytes, maximum, |_, intent, _, _, _, _, _| {
            Ok(matches!(intent, 12..=17))
        })?
    {
        return super::credit::scan(bytes, scope, request, admission, maximum);
    }
    if layout == 8
        && with_frame(bytes, maximum, |_, intent, _, _, _, _, _| {
            Ok(matches!(intent, 10 | 11))
        })?
    {
        return scan_step_continue_outcome(bytes, scope, request, admission, maximum);
    }
    let (schema, charge, validation) = profile;
    super::scan_outcome_profiled(
        bytes,
        scope,
        request,
        admission,
        maximum,
        (schema, charge),
        validation,
    )
}

pub(crate) fn encode_outcome_in_layout(
    outcome: &RetainedOutcome,
    maximum: usize,
    admission: AdmissionLimits,
    layout: u16,
) -> Result<Vec<u8>, StateValueError> {
    if layout == 8 && outcome.intent().version() == 18 {
        return super::release_maintenance::encode(outcome, maximum, admission);
    }
    if layout == 8 && matches!(outcome.intent().version(), 19 | 20) {
        return super::governed_claim::encode(outcome, maximum, admission);
    }
    if layout == 8 && matches!(outcome.intent().version(), 12..=17) {
        return super::credit::encode(outcome, maximum, admission);
    }
    if layout == 8 && matches!(outcome.intent().version(), 10 | 11) {
        return encode_step_continue_outcome(outcome, maximum, admission);
    }
    super::encode_outcome(outcome, maximum)
}

/// Encodes one explicitly selected Step receipt under actual captured field caps.
/// # Errors
/// Rejects other profiles, inconsistent responses, limits or allocation failure.
pub fn encode_step_continue_outcome(
    outcome: &RetainedOutcome,
    maximum_wire_bytes: usize,
    admission: AdmissionLimits,
) -> Result<Vec<u8>, StateValueError> {
    if maximum_wire_bytes == 0 {
        return Err(StateValueError::InvalidValue);
    }
    let (status, count) = step_continue_retained_summary(
        outcome.intent(),
        outcome.response_version(),
        outcome.response(),
        outcome.original_position(),
        outcome.original_time(),
        admission,
    )
    .map_err(claim_error)?;
    validate_count(status, count)?;
    if status != outcome.status() {
        return Err(StateValueError::InvalidValue);
    }
    encode_fields(outcome, maximum_wire_bytes)
}

/// Scans the entire selected receipt before admitting any materialized owner.
/// # Errors
/// Rejects malformed/foreign data, unsupported profiles and captured field limits.
pub fn scan_step_continue_outcome<'input>(
    bytes: &'input [u8],
    expected_scope: &RequestScope,
    expected_request: &RequestId,
    admission: AdmissionLimits,
    maximum_wire_bytes: usize,
) -> Result<ScannedRetainedOutcome<'input>, StateValueError> {
    with_frame(
        bytes,
        maximum_wire_bytes,
        |fields,
         intent_version,
         digest,
         original_position,
         original_time,
         expires_at,
         response_version| {
            if !matches!(intent_version, 10 | 11) || response_version != 9 {
                return Err(StateValueError::UnsupportedVersion);
            }
            let [tenant, group, recovery, request, response] = fields;
            if tenant != expected_scope.tenant().as_bytes()
                || group != expected_scope.group().as_bytes()
                || recovery != expected_scope.recovery().as_bytes()
                || request != expected_request.as_bytes()
                || expires_at <= original_time
            {
                return Err(StateValueError::InvalidValue);
            }
            let intent = if intent_version == 11 {
                ClientIntent::stored_step_continue(digest)
            } else {
                ClientIntent::step_continue(digest)
            };
            let (status, response_count) = step_continue_retained_summary(
                intent,
                response_version,
                response,
                original_position,
                original_time,
                admission,
            )
            .map_err(claim_error)?;
            validate_count(status, response_count)?;
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

fn validate_count(status: RetainedStatus, count: Option<usize>) -> Result<(), StateValueError> {
    match (status, count) {
        (RetainedStatus::Accepted, Some(1)) | (RetainedStatus::IndexedRejected(_), None) => Ok(()),
        _ => Err(StateValueError::InvalidValue),
    }
}
