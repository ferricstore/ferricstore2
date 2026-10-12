//! Original expiry, aggregate admission and immutable response ownership.

use wf_types::identity::RequestId;
use wf_types::version::{LogIdentity, TimestampMillis};

use crate::policy::installation::PolicyError;
use crate::workflow::admission::AdmissionLimits;
use crate::workflow::batch::ApplyError;
use crate::workflow::request::{
    ClientIntent, RequestScope, RequestValueError, RetainedMetadata, RetainedOutcome,
    RetainedStatus,
};

use super::super::admission::charge_schema;
use super::super::replace_content::complete_policy_replace_rejection_body_len;
use super::super::{
    CompletePolicyEncodingError, PolicySchemaCharge, PolicySchemaError, PolicySchemaLimits,
    encode_complete_policy_replace_rejection_body,
};
use super::{
    CompletePolicyReplaceApplyError, CompletePolicyReplaceFresh, ReplacePrepared, ReplaceRejected,
};

pub(super) fn retain_replace_outcome(
    scope: RequestScope,
    request: RequestId,
    committed: (ClientIntent, LogIdentity, TimestampMillis),
    prepared: ReplacePrepared,
    window: Result<u64, ApplyError>,
    profiles: (PolicySchemaLimits, AdmissionLimits),
    charge: &mut PolicySchemaCharge,
) -> Result<CompletePolicyReplaceFresh, CompletePolicyReplaceApplyError> {
    use CompletePolicyReplaceApplyError::{Outcome, Request};

    let (limits, admission) = profiles;
    let expires_at = replace_outcome_expiry(window, committed.2, charge)?;
    replace_parent_bytes(&scope, &request, &prepared, admission, charge)?;
    charge_schema(charge, 128, 0).map_err(|error| Request(error.into()))?;
    let status = match &prepared {
        Ok(_) => RetainedStatus::Accepted,
        Err(ReplaceRejected::Policy(reason)) => RetainedStatus::PolicyRejected(*reason),
        Err(ReplaceRejected::Content(reason)) => {
            RetainedStatus::CompletePolicyReplaceRejected(*reason)
        }
    };
    let metadata = RetainedMetadata {
        scope,
        request,
        intent: committed.0,
        original_position: committed.1,
        original_time: committed.2,
        expires_at,
        status,
    };
    let (installed, response) = match prepared {
        Ok((stored, value)) => {
            let response = copy_replace_response(value.encoded(), charge)?;
            (Some((stored, value)), response)
        }
        Err(ReplaceRejected::Policy(reason)) => (
            None,
            copy_replace_response(replace_generation_message(reason.error())?, charge)?,
        ),
        Err(ReplaceRejected::Content(reason)) => (
            None,
            encode_complete_policy_replace_rejection_body(reason, limits, charge)
                .map_err(|error| Request(error.into()))?,
        ),
    };
    charge_schema(charge, 128, 0).map_err(|error| Request(error.into()))?;
    let outcome = RetainedOutcome::new(metadata, 1, response, admission).map_err(Outcome)?;
    Ok((installed, outcome))
}

fn replace_generation_message(
    error: PolicyError,
) -> Result<&'static [u8], CompletePolicyReplaceApplyError> {
    Ok(match error {
        PolicyError::InvalidGeneration => b"ERR invalid flow policy generation",
        PolicyError::StaleGeneration => b"ERR stale flow policy generation",
        PolicyError::GenerationExhausted => b"ERR flow policy generation exhausted",
        _ => {
            return Err(CompletePolicyReplaceApplyError::Request(
                PolicySchemaError::InvalidAuthority.into(),
            ));
        }
    })
}

fn replace_outcome_expiry(
    window: Result<u64, ApplyError>,
    now: TimestampMillis,
    charge: &mut PolicySchemaCharge,
) -> Result<TimestampMillis, CompletePolicyReplaceApplyError> {
    use CompletePolicyReplaceApplyError::{Apply, Outcome, Request};

    charge_schema(charge, 128, 0).map_err(|error| Request(error.into()))?;
    let window = window.map_err(Apply)?;
    if window == 0 {
        return Err(Outcome(RequestValueError::InvalidExpiry));
    }
    now.checked_add(window)
        .ok_or(Outcome(RequestValueError::InvalidExpiry))
}

fn replace_parent_bytes(
    scope: &RequestScope,
    request: &RequestId,
    prepared: &ReplacePrepared,
    admission: AdmissionLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<(), CompletePolicyReplaceApplyError> {
    use CompletePolicyReplaceApplyError::Request;

    charge_schema(charge, 128, 0).map_err(|error| Request(error.into()))?;
    let (installed_len, response_len) = match prepared {
        Ok((_, value)) => (value.encoded().len(), value.encoded().len()),
        Err(ReplaceRejected::Policy(reason)) => {
            (0, replace_generation_message(reason.error())?.len())
        }
        Err(ReplaceRejected::Content(reason)) => (
            0,
            complete_policy_replace_rejection_body_len(*reason)
                .map_err(|error| Request(error.into()))?,
        ),
    };
    let bytes = [
        scope.tenant().as_bytes().len(),
        scope.group().as_bytes().len(),
        scope.recovery().as_bytes().len(),
        request.as_bytes().len(),
        installed_len,
        response_len,
    ]
    .into_iter()
    .try_fold(0_usize, usize::checked_add)
    .ok_or(Request(PolicySchemaError::LengthOverflow.into()))?;
    if bytes > admission.decision_bytes() {
        return Err(Request(CompletePolicyEncodingError::ByteLimit.into()));
    }
    Ok(())
}

fn copy_replace_response(
    bytes: &[u8],
    charge: &mut PolicySchemaCharge,
) -> Result<Vec<u8>, CompletePolicyReplaceApplyError> {
    use CompletePolicyReplaceApplyError::Request;

    let work = u64::try_from(bytes.len())
        .ok()
        .and_then(|length| length.checked_add(128))
        .ok_or(Request(PolicySchemaError::LengthOverflow.into()))?;
    charge_schema(charge, work, 0).map_err(|error| Request(error.into()))?;
    let mut response = Vec::new();
    response
        .try_reserve_exact(bytes.len())
        .map_err(|_| Request(CompletePolicyEncodingError::AllocationFailure.into()))?;
    response.extend_from_slice(bytes);
    Ok(response)
}
