//! Expiry, aggregate bytes and exactly one final retained response owner.

use super::super::super::flow_input::rejection;
use super::super::super::retention_input;
use super::{
    ApplyErrorType, CompletePolicyReplaceApplyError, Error, FlowReplacePrepared,
    FlowReplaceRejected, OriginalFlowPolicyReplaceFresh,
    OriginalFlowPolicyReplaceRejection as Reason, PolicySchemaCharge, PolicySchemaError, copy,
    encoding, operational, pay, plus, schema,
};
use crate::policy::installation::PolicyError;
use crate::workflow::admission::AdmissionLimits;
use crate::workflow::batch::ApplyError;
use crate::workflow::request::{
    ClientIntent, RequestScope, RequestValueError, RetainedMetadata, RetainedOutcome,
    RetainedStatus,
};
use wf_types::identity::RequestId;
use wf_types::version::{LogIdentity, TimestampMillis};

pub(super) fn retain(
    scope: RequestScope,
    request: RequestId,
    committed: (ClientIntent, LogIdentity, TimestampMillis),
    prepared: FlowReplacePrepared,
    window: Result<u64, ApplyError>,
    admission: AdmissionLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<OriginalFlowPolicyReplaceFresh, ApplyErrorType> {
    pay(charge, 128)?;
    let window = window.map_err(operational)?;
    let expires_at = committed
        .2
        .checked_add(window)
        .filter(|_| window != 0)
        .ok_or_else(|| outcome(RequestValueError::InvalidExpiry))?;
    aggregate(&scope, &request, &prepared, admission, charge)?;
    pay(charge, 128)?;
    let status = match &prepared {
        Ok(_) => RetainedStatus::Accepted,
        Err(FlowReplaceRejected::Policy(reason)) => RetainedStatus::PolicyRejected(*reason),
        Err(FlowReplaceRejected::Flow(reason)) => {
            RetainedStatus::OriginalFlowPolicyReplaceRejected(*reason)
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
        Ok(value) => {
            let response = copy(value.encoded(), charge)?;
            (Some(value), response)
        }
        Err(FlowReplaceRejected::Policy(reason)) => {
            (None, copy(generation_message(reason.error())?, charge)?)
        }
        Err(FlowReplaceRejected::Flow(reason)) => (None, body(reason, charge)?),
    };
    pay(charge, 128)?;
    let retained = RetainedOutcome::new(metadata, 1, response, admission).map_err(outcome)?;
    Ok((installed, retained))
}

fn aggregate(
    scope: &RequestScope,
    request: &RequestId,
    prepared: &FlowReplacePrepared,
    admission: AdmissionLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<(), ApplyErrorType> {
    pay(charge, 128)?;
    let body = match prepared {
        Ok(value) => plus(
            value.policy_key().len(),
            plus(value.encoded().len(), value.encoded().len())?,
        )?,
        Err(FlowReplaceRejected::Policy(reason)) => generation_message(reason.error())?.len(),
        Err(FlowReplaceRejected::Flow(reason)) => body_len(*reason)?,
    };
    let mut total = body;
    for bytes in [
        scope.tenant().as_bytes(),
        scope.group().as_bytes(),
        scope.recovery().as_bytes(),
        request.as_bytes(),
    ] {
        total = plus(total, bytes.len())?;
    }
    if total > admission.decision_bytes() {
        return Err(encoding(Error::ByteLimit));
    }
    Ok(())
}

fn body_len(reason: Reason) -> Result<usize, ApplyErrorType> {
    match reason {
        Reason::Input(reason) => rejection::body_len(reason).map_err(encoding),
        _ => Ok(parts(reason)?.0.iter().map(|part| part.len()).sum()),
    }
}

fn parts(reason: Reason) -> Result<([&'static [u8]; 4], Option<u32>), ApplyErrorType> {
    Ok(match reason {
        Reason::PatchShape => ([b"ERR invalid flow policy patch", b"", b"", b""], None),
        Reason::PolicyKey => ([b"ERR invalid flow policy key", b"", b"", b""], None),
        Reason::Input(reason) => rejection::parts(reason).map_err(encoding)?,
    })
}

fn body(reason: Reason, charge: &mut PolicySchemaCharge) -> Result<Vec<u8>, ApplyErrorType> {
    pay(charge, 128)?;
    let (parts, maximum) = parts(reason)?;
    let digits = maximum.map_or(0, retention_input::decimal_length);
    let length = parts
        .iter()
        .try_fold(digits, |sum, part| sum.checked_add(part.len()))
        .ok_or_else(|| schema(PolicySchemaError::LengthOverflow))?;
    pay(charge, digits)?;
    pay(charge, plus(128, length)?)?;
    let mut output = Vec::new();
    output
        .try_reserve_exact(length)
        .map_err(|_| encoding(Error::AllocationFailure))?;
    for part in parts {
        output.extend_from_slice(part);
    }
    if let Some(maximum) = maximum {
        let mut scratch = [0; 7];
        output.extend_from_slice(
            retention_input::write_decimal(maximum, &mut scratch).map_err(encoding)?,
        );
    }
    Ok(output)
}

fn generation_message(error: PolicyError) -> Result<&'static [u8], ApplyErrorType> {
    match error {
        PolicyError::InvalidGeneration => Ok(b"ERR invalid flow policy generation"),
        PolicyError::StaleGeneration => Ok(b"ERR stale flow policy generation"),
        PolicyError::GenerationExhausted => Ok(b"ERR flow policy generation exhausted"),
        _ => Err(schema(PolicySchemaError::InvalidAuthority)),
    }
}
fn outcome(error: RequestValueError) -> ApplyErrorType {
    ApplyErrorType::Replace(CompletePolicyReplaceApplyError::Outcome(error))
}
