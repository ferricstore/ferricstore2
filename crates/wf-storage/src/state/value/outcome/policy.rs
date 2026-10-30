//! Explicit-profile receipts through the shared FWSO framing and owners.

use super::{ScannedRetainedOutcome, encode_fields, encode_outcome, scan_with_policy, with_frame};
use crate::state::value::StateValueError;
use wf_domain::policy::complete::{PolicySchemaCharge, PolicySchemaLimits};
use wf_domain::workflow::admission::AdmissionLimits;
use wf_domain::workflow::request::{ClientIntent, RequestScope, RetainedOutcome, RetainedStatus};
use wf_ports::log::{
    CodecError, claim_retained_summary, complete_core_policy_patch_retained_status,
    original_policy_set_response_status, public_mutation_retained_status,
    record_batch_retained_summary, workflow_value_retained_summary,
};
use wf_types::identity::RequestId;
use wf_types::version::{LogIdentity, TimestampMillis};

/// Encodes policy and value receipts with the actual captured profile.
///
/// The caller prospectively pays the temporary policy owner and output wire.
/// Other intent formats retain the exact legacy encoder and its admission rules.
/// # Errors
/// Refuses profile limits, malformed responses, status mismatch or allocation.
pub fn encode_outcome_profiled(
    outcome: &RetainedOutcome,
    maximum_wire_bytes: usize,
    admission: AdmissionLimits,
    schema: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
    maximum_policy_owner_bytes: usize,
) -> Result<Vec<u8>, StateValueError> {
    if !matches!(outcome.intent().version(), 2 | 4 | 8 | 9) {
        return encode_outcome(outcome, maximum_wire_bytes);
    }
    if maximum_wire_bytes == 0 {
        return Err(StateValueError::InvalidValue);
    }
    let status = if matches!(outcome.intent().version(), 8 | 9) {
        workflow_value_status(outcome, admission)?
    } else {
        retained_status(
            outcome.intent(),
            outcome.response_version(),
            outcome.response(),
            admission,
            (schema, charge),
            maximum_policy_owner_bytes,
        )?
    };
    if status != outcome.status() {
        return Err(StateValueError::InvalidValue);
    }
    encode_fields(outcome, maximum_wire_bytes)
}

/// Scans retained framing and policy semantics on the same passed schema account.
///
/// The result borrows only `bytes`; no temporary policy value escapes the scan.
/// The actual retained owner is separately admitted before `into_owned`.
/// # Errors
/// Refuses malformed/scope-invalid data or legitimate narrower policy profiles.
pub fn scan_outcome_profiled<'input>(
    bytes: &'input [u8],
    expected_scope: &RequestScope,
    expected_request: &RequestId,
    admission: AdmissionLimits,
    maximum_wire_bytes: usize,
    profile: (PolicySchemaLimits, &mut PolicySchemaCharge),
    maximum_policy_owner_bytes: usize,
) -> Result<ScannedRetainedOutcome<'input>, StateValueError> {
    let (schema, charge) = profile;
    scan_with_policy(
        bytes,
        expected_scope,
        expected_request,
        admission,
        maximum_wire_bytes,
        Some((schema, charge, maximum_policy_owner_bytes)),
    )
}

/// Sizes only prospective policy validation; success is not semantic validation.
///
/// Every intent2 FWPV prefix receives `2048 + 216 * actual_response_bytes`, even
/// if its version, content or trailing bytes will subsequently be rejected.
/// Intent4 ETF receives `65536 + 256 * actual_response_bytes` independently.
/// Zero for other bodies does not recognize a valid business error or old value.
/// # Errors
/// Refuses malformed/oversized FWSO framing or checked-size overflow.
pub fn outcome_policy_validation_bytes(
    bytes: &[u8],
    maximum_wire_bytes: usize,
) -> Result<usize, StateValueError> {
    validation_bytes(bytes, maximum_wire_bytes, None)
}

pub(crate) fn outcome_policy_validation_bytes_in_layout(
    bytes: &[u8],
    maximum_wire_bytes: usize,
    layout: u16,
) -> Result<usize, StateValueError> {
    validation_bytes(bytes, maximum_wire_bytes, Some(layout))
}

fn validation_bytes(
    bytes: &[u8],
    maximum_wire_bytes: usize,
    layout: Option<u16>,
) -> Result<usize, StateValueError> {
    with_frame(
        bytes,
        maximum_wire_bytes,
        |fields, intent, _, _, _, _, response_version| {
            let response = fields[4];
            if matches!(intent, 8 | 9) && layout.is_some_and(|layout| !matches!(layout, 7 | 8)) {
                return Err(StateValueError::Malformed);
            }
            let core_original = layout.is_some() && intent == 2 && response_version == 2;
            if core_original && !matches!(layout, Some(6..=8)) {
                return Err(StateValueError::Malformed);
            }
            if (intent == 4 || core_original) && response.first() == Some(&131) {
                if response.len() > 263_168 {
                    return Err(StateValueError::Malformed);
                }
                return response
                    .len()
                    .checked_mul(256)
                    .and_then(|n| n.checked_add(65_536))
                    .ok_or(StateValueError::LengthOverflow);
            }
            if intent != 2 || !response.starts_with(b"FWPV") {
                return Ok(0);
            }
            response
                .len()
                .checked_mul(216)
                .and_then(|bytes| bytes.checked_add(2048))
                .ok_or(StateValueError::LengthOverflow)
        },
    )
}

pub(super) fn restore_intent(
    version: u16,
    digest: [u8; 32],
    profiled: bool,
) -> Result<ClientIntent, StateValueError> {
    Ok(match version {
        1 => ClientIntent::new(1, digest).map_err(|_| StateValueError::UnsupportedVersion)?,
        2 if profiled => ClientIntent::complete_core_policy_patch(digest),
        4 if profiled => ClientIntent::original_policy_set(digest),
        5 => ClientIntent::record_batch(digest),
        6 => ClientIntent::independent_record_batch(digest),
        7 => ClientIntent::atomic_start(digest),
        8 => ClientIntent::workflow_value_batch(digest),
        9 => ClientIntent::workflow_stored_value_batch(digest),
        _ => return Err(StateValueError::UnsupportedVersion),
    })
}

pub(super) fn response_summary(
    intent: ClientIntent,
    response_version: u16,
    response: &[u8],
    original: (LogIdentity, TimestampMillis),
    admission: AdmissionLimits,
    profile: Option<(PolicySchemaLimits, &mut PolicySchemaCharge, usize)>,
) -> Result<(RetainedStatus, Option<usize>), StateValueError> {
    let (position, time) = original;
    if matches!(intent.version(), 2 | 4) {
        let (schema, charge, maximum_owner) = profile.ok_or(StateValueError::UnsupportedVersion)?;
        return Ok((
            retained_status(
                intent,
                response_version,
                response,
                admission,
                (schema, charge),
                maximum_owner,
            )?,
            None,
        ));
    }
    if intent.version() == 1 && response_version == 3 {
        return claim_retained_summary(intent, response_version, response, time, admission)
            .map_err(super::claim_error);
    }
    if intent.version() == 7 {
        return super::atomic_start::scan(
            intent,
            response_version,
            response,
            position,
            time,
            admission,
        );
    }
    if matches!(intent.version(), 8 | 9) {
        return workflow_value_retained_summary(
            intent,
            response_version,
            response,
            position,
            time,
            admission,
        )
        .map_err(super::claim_error);
    }
    if intent.version() == 1 {
        return Ok((
            public_mutation_retained_status(intent, response_version, response)?,
            None,
        ));
    }
    Ok(record_batch_retained_summary(
        intent,
        response_version,
        response,
    )?)
}

pub(super) fn workflow_value_status(
    outcome: &RetainedOutcome,
    admission: AdmissionLimits,
) -> Result<RetainedStatus, StateValueError> {
    workflow_value_retained_summary(
        outcome.intent(),
        outcome.response_version(),
        outcome.response(),
        outcome.original_position(),
        outcome.original_time(),
        admission,
    )
    .map(|(status, _)| status)
    .map_err(super::claim_error)
}

pub(super) fn retained_status(
    intent: ClientIntent,
    response_version: u16,
    response: &[u8],
    admission: AdmissionLimits,
    profile: (PolicySchemaLimits, &mut PolicySchemaCharge),
    maximum_policy_owner_bytes: usize,
) -> Result<RetainedStatus, StateValueError> {
    let (schema, charge) = profile;
    if intent.version() == 4 {
        if response_version != 1 {
            return Err(StateValueError::UnsupportedVersion);
        }
        if charge.profile() != schema {
            return Err(StateValueError::InvalidValue);
        }
        if response.len() > 263_168 {
            return Err(StateValueError::Malformed);
        }
        if response.len() > admission.decision_bytes() {
            return Err(StateValueError::ProfileLimit);
        }
        return original_policy_set_response_status(
            response,
            schema,
            charge,
            maximum_policy_owner_bytes,
        )
        .map_err(|error| match error {
            CodecError::ByteLimit | CodecError::ItemLimit | CodecError::ValueLimit => {
                StateValueError::ProfileLimit
            }
            CodecError::UnsupportedPolicyProfile => StateValueError::UnsupportedPolicyProfile,
            other => other.into(),
        });
    }
    complete_core_policy_patch_retained_status(
        intent,
        response_version,
        response,
        (schema, admission),
        charge,
        maximum_policy_owner_bytes,
    )
    .map_err(|error| match error {
        CodecError::ByteLimit | CodecError::ItemLimit | CodecError::ValueLimit => {
            StateValueError::ProfileLimit
        }
        CodecError::UnsupportedPolicyProfile => StateValueError::UnsupportedPolicyProfile,
        other => other.into(),
    })
}
