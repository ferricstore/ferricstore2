//! Full original ETF or exact original rejection on one supplied schema account.

use super::OriginalPolicySetRequestPlan;
use crate::log::CodecError;
use wf_domain::policy::complete::{
    CompletePolicyEncodingError as Error, OriginalPolicyEntry, PolicySchemaCharge,
    PolicySchemaError, PolicySchemaLimits, decode_original_complete_policy_entry_portable,
    original_policy_set_rejection_from_body,
};
use wf_domain::policy::installation::{MAX_POLICY_GENERATION, PolicyError};
use wf_domain::workflow::request::{DispatchDecision, RetainedStatus};

/// Restores allocated original policy success or an exact business diagnosis.
/// # Errors
/// Preserves smaller profiles and rejects malformed or unrelated responses.
pub fn original_policy_set_response_status(
    response: &[u8],
    schema: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
    maximum_policy_owner_bytes: usize,
) -> Result<RetainedStatus, CodecError> {
    validated_response(response, schema, charge, maximum_policy_owner_bytes)
        .map(|(status, _)| status)
}

fn validated_response<'raw>(
    response: &'raw [u8],
    schema: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
    maximum_policy_owner_bytes: usize,
) -> Result<(RetainedStatus, Option<OriginalPolicyEntry<'raw>>), CodecError> {
    if charge.profile() != schema {
        return Err(CodecError::Malformed);
    }
    if response.len() > 263_168 {
        return Err(CodecError::Malformed);
    }
    if response.first() == Some(&131) {
        let x = response
            .len()
            .checked_mul(256)
            .and_then(|n| n.checked_add(65_536))
            .ok_or(CodecError::LengthOverflow)?;
        if x > maximum_policy_owner_bytes {
            return Err(CodecError::ByteLimit);
        }
        let entry = decode_original_complete_policy_entry_portable(response, schema, charge)
            .map_err(encoding_error)?
            .ok_or(CodecError::Malformed)?;
        if entry.generation() == 0
            || entry.generation() > MAX_POLICY_GENERATION
            || entry.workflow_type().is_none_or(<[u8]>::is_empty)
        {
            return Err(CodecError::Malformed);
        }
        return Ok((RetainedStatus::Accepted, Some(entry)));
    }
    original_policy_set_rejection_from_body(response, schema, charge)
        .map_err(encoding_error)?
        .map(|status| (status, None))
        .ok_or(CodecError::Malformed)
}

/// Repeats complete response validation and compares the actual sealed Fresh plan.
/// # Errors
/// Preserves semantic/profile refusals; correspondence failure returns false.
pub fn original_policy_set_response_matches(
    plan: &OriginalPolicySetRequestPlan<'_>,
    schema: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
    maximum_policy_owner_bytes: usize,
) -> Result<bool, CodecError> {
    let DispatchDecision::Fresh((install, outcome)) = plan.decision() else {
        return Ok(false);
    };
    if outcome.intent().version() != 4
        || outcome.response_version() != 1
        || plan.portable_profile_id() != Some(1)
    {
        return Ok(false);
    }
    let (status, entry) = validated_response(
        outcome.response(),
        schema,
        charge,
        maximum_policy_owner_bytes,
    )?;
    if status != outcome.status() {
        return Ok(false);
    }
    Ok(match (install, entry) {
        (Some(install), Some(entry)) => {
            status == RetainedStatus::Accepted
                && install.generation() == entry.generation()
                && install.generation() > install.stored_generation()
                && install.encoded() == outcome.response()
                && install.policy_key().strip_prefix(b"f:{f}:policy:") == plan.workflow_type()
                && entry.workflow_type() == plan.workflow_type()
        }
        (None, None) => matches!(
            status,
            RetainedStatus::PolicyRejected(_) | RetainedStatus::OriginalPolicySetRejected(_)
        ),
        _ => false,
    })
}

fn encoding_error(error: Error) -> CodecError {
    match error {
        Error::UnsupportedPolicyProfile => CodecError::UnsupportedPolicyProfile,
        Error::UnsupportedVersion => CodecError::UnsupportedVersion,
        Error::AllocationFailure => CodecError::Allocation,
        Error::Schema(PolicySchemaError::WorkBytes { .. }) => CodecError::ByteLimit,
        Error::Schema(PolicySchemaError::Policy(PolicyError::StateBudget)) => CodecError::ItemLimit,
        _ => CodecError::Malformed,
    }
}
