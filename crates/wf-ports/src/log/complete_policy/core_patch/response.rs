//! Complete retained response validation on the received schema account.

use wf_domain::policy::complete::{
    CompleteCorePolicyPatchFresh, CompletePolicyEncodingError, CompletePolicyValue,
    OriginalPolicyEntry, PolicySchemaCharge, PolicySchemaError, PolicySchemaLimits,
    StoredPolicyValue, core_policy_patch_rejection_for_response,
    decode_complete_policy_value_v2_profiled,
};
use wf_domain::policy::installation::{
    MAX_ENCODED_POLICY_BYTES, MAX_POLICY_GENERATION, PolicyError,
};
use wf_domain::workflow::admission::AdmissionLimits;
use wf_domain::workflow::request::{ClientIntent, RetainedStatus};

use super::super::super::CodecError;

/// Validates the complete intent-two response without retaining policy owners.
///
/// Accepted responses use the original full semantic decoder. Business responses
/// use the charged exact registry, including every comparison before a match.
/// # Errors
/// Separates narrower operational profiles from invalid persisted grammar.
pub fn complete_core_policy_patch_retained_status(
    intent: ClientIntent,
    response_version: u16,
    response: &[u8],
    profiles: (PolicySchemaLimits, AdmissionLimits),
    charge: &mut PolicySchemaCharge,
    maximum_policy_owner_bytes: usize,
) -> Result<RetainedStatus, CodecError> {
    if original_response(intent, response_version, response) {
        return validated_original(response, profiles, charge, maximum_policy_owner_bytes)
            .map(|_| RetainedStatus::Accepted);
    }
    validated_response(
        intent,
        response_version,
        response,
        profiles,
        charge,
        maximum_policy_owner_bytes,
    )
    .map(|(status, _)| status)
}

fn validated_response(
    intent: ClientIntent,
    response_version: u16,
    response: &[u8],
    profiles: (PolicySchemaLimits, AdmissionLimits),
    charge: &mut PolicySchemaCharge,
    maximum_policy_owner_bytes: usize,
) -> Result<(RetainedStatus, Option<CompletePolicyValue>), CodecError> {
    if intent.version() != 2 || response_version != 1 {
        return Err(CodecError::UnsupportedVersion);
    }
    let (schema, admission) = profiles;
    if charge.profile() != schema {
        return Err(CodecError::Malformed);
    }
    if response.len() > admission.decision_bytes() {
        return Err(CodecError::ByteLimit);
    }
    if response.starts_with(b"FWPV") {
        let owner = response
            .len()
            .checked_mul(216)
            .and_then(|bytes| bytes.checked_add(2048))
            .ok_or(CodecError::LengthOverflow)?;
        if owner > maximum_policy_owner_bytes {
            return Err(CodecError::ByteLimit);
        }
        let value = decode_complete_policy_value_v2_profiled(response, schema, charge)
            .map_err(encoding_error)?;
        Ok((RetainedStatus::Accepted, Some(value)))
    } else {
        core_policy_patch_rejection_for_response(response, schema, charge)
            .map_err(|error| encoding_error(CompletePolicyEncodingError::Schema(error)))?
            .map(|reason| (RetainedStatus::PolicyRejected(reason), None))
            .ok_or(CodecError::Malformed)
    }
}

/// Repeats actual response validation before comparing the proposed Fresh result.
/// # Errors
/// Preserves invalid grammar and operational profile diagnoses from validation.
pub fn complete_core_policy_patch_response_matches(
    fresh: &CompleteCorePolicyPatchFresh,
    profiles: (PolicySchemaLimits, AdmissionLimits),
    charge: &mut PolicySchemaCharge,
    maximum_policy_owner_bytes: usize,
) -> Result<bool, CodecError> {
    response_matches_for_query(fresh, profiles, charge, maximum_policy_owner_bytes, None)
}

pub(super) fn response_matches_for_query(
    fresh: &CompleteCorePolicyPatchFresh,
    profiles: (PolicySchemaLimits, AdmissionLimits),
    charge: &mut PolicySchemaCharge,
    maximum_policy_owner_bytes: usize,
    query: Option<&[u8]>,
) -> Result<bool, CodecError> {
    let (candidate, outcome) = fresh;
    if original_response(
        outcome.intent(),
        outcome.response_version(),
        outcome.response(),
    ) {
        let entry = validated_original(
            outcome.response(),
            profiles,
            charge,
            maximum_policy_owner_bytes,
        )?;
        return Ok(match candidate {
            Some((expected, StoredPolicyValue::Original(raw))) => {
                outcome.status() == RetainedStatus::Accepted
                    && *expected < MAX_POLICY_GENERATION
                    && entry.generation() > *expected
                    && raw.as_slice() == outcome.response()
                    && query.is_none_or(|kind| entry.workflow_type() == Some(kind))
            }
            _ => false,
        });
    }
    let (status, decoded) = validated_response(
        outcome.intent(),
        outcome.response_version(),
        outcome.response(),
        profiles,
        charge,
        maximum_policy_owner_bytes,
    )?;
    if status != outcome.status() {
        return Ok(false);
    }
    Ok(match candidate {
        Some((expected, StoredPolicyValue::Complete(value))) => {
            status == RetainedStatus::Accepted
                && *expected < MAX_POLICY_GENERATION
                && value.generation() == expected + 1
                && decoded.as_ref() == Some(value.as_ref())
        }
        Some((_, StoredPolicyValue::Original(_))) => false,
        None => matches!(status, RetainedStatus::PolicyRejected(_)),
    })
}

fn original_response(intent: ClientIntent, version: u16, response: &[u8]) -> bool {
    intent.version() == 2 && version == 2 && response.first() == Some(&131)
}

fn validated_original<'raw>(
    response: &'raw [u8],
    profiles: (PolicySchemaLimits, AdmissionLimits),
    charge: &mut PolicySchemaCharge,
    maximum_policy_owner_bytes: usize,
) -> Result<OriginalPolicyEntry<'raw>, CodecError> {
    if charge.profile() != profiles.0 {
        return Err(CodecError::Malformed);
    }
    if response.len() > MAX_ENCODED_POLICY_BYTES {
        return Err(CodecError::Malformed);
    }
    if response.len() > profiles.1.decision_bytes() {
        return Err(CodecError::ByteLimit);
    }
    // The exact existing ETF validator performs one decoder and no accepted
    // registry pass. Its returned entry borrows the original response bytes.
    super::super::public_set::response::validated_response(
        response,
        profiles.0,
        charge,
        maximum_policy_owner_bytes,
    )?
    .1
    .ok_or(CodecError::Malformed)
}

fn encoding_error(error: CompletePolicyEncodingError) -> CodecError {
    match error {
        CompletePolicyEncodingError::UnsupportedVersion => CodecError::UnsupportedVersion,
        CompletePolicyEncodingError::UnsupportedTag => CodecError::UnsupportedTag,
        CompletePolicyEncodingError::InvalidOptionality => CodecError::InvalidOptionality,
        CompletePolicyEncodingError::TrailingBytes => CodecError::TrailingBytes,
        CompletePolicyEncodingError::AllocationFailure => CodecError::Allocation,
        CompletePolicyEncodingError::Schema(PolicySchemaError::WorkBytes { .. }) => {
            CodecError::ByteLimit
        }
        CompletePolicyEncodingError::Schema(PolicySchemaError::Policy(
            PolicyError::StateBudget,
        )) => CodecError::ItemLimit,
        _ => CodecError::Malformed,
    }
}
