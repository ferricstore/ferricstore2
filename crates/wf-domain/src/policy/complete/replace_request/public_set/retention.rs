//! Separate response ownership and original late retention/aggregate ordering.

use super::{
    Error, ExistingError, OriginalFlowPolicyInstall, OriginalPolicySetFresh, PolicySchemaCharge,
    PolicySchemaError, PolicySetRejected, RawError, copy, encoding, operational, pay, plus, schema,
};
use crate::policy::complete::flow_input::rejection;
use crate::policy::complete::retention_input;
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
    prepared: Result<OriginalFlowPolicyInstall, PolicySetRejected>,
    window: Result<u64, ApplyError>,
    admission: AdmissionLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<OriginalPolicySetFresh, RawError> {
    pay(charge, 128)?;
    let window = window.map_err(operational)?;
    let expires_at = committed
        .2
        .checked_add(window)
        .filter(|_| window != 0)
        .ok_or_else(|| outcome(RequestValueError::InvalidExpiry))?;
    pay(charge, 128)?;
    let length = match &prepared {
        Ok(value) => plus(
            value.policy_key().len(),
            plus(value.encoded().len(), value.encoded().len())?,
        )?,
        Err(PolicySetRejected::Policy(reason)) => policy_body(reason.error())?.len(),
        Err(PolicySetRejected::Input(reason)) => rejection::body_len(*reason).map_err(encoding)?,
    };
    let mut total = length;
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
    super::peak_at(plus(512, total)?, charge).map_err(encoding)?;
    pay(charge, 128)?;
    let status = match &prepared {
        Ok(_) => RetainedStatus::Accepted,
        Err(PolicySetRejected::Policy(reason)) => RetainedStatus::PolicyRejected(*reason),
        Err(PolicySetRejected::Input(reason)) => RetainedStatus::OriginalPolicySetRejected(*reason),
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
        Err(PolicySetRejected::Policy(reason)) => {
            (None, copy(policy_body(reason.error())?, charge)?)
        }
        Err(PolicySetRejected::Input(reason)) => {
            pay(charge, 128)?;
            let (parts, maximum) = rejection::parts(reason).map_err(encoding)?;
            let digits = maximum.map_or(0, retention_input::decimal_length);
            let size = parts
                .iter()
                .try_fold(digits, |n, part| n.checked_add(part.len()))
                .ok_or_else(|| schema(PolicySchemaError::LengthOverflow))?;
            pay(charge, digits)?;
            pay(charge, plus(128, size)?)?;
            let mut out = Vec::new();
            out.try_reserve_exact(size)
                .map_err(|_| encoding(Error::AllocationFailure))?;
            for part in parts {
                out.extend_from_slice(part);
            }
            if let Some(maximum) = maximum {
                let mut buffer = [0; 7];
                out.extend_from_slice(
                    retention_input::write_decimal(maximum, &mut buffer).map_err(encoding)?,
                );
            }
            (None, out)
        }
    };
    pay(charge, 128)?;
    Ok((
        installed,
        RetainedOutcome::new(metadata, 1, response, admission).map_err(outcome)?,
    ))
}

fn policy_body(error: PolicyError) -> Result<&'static [u8], RawError> {
    match error {
        PolicyError::InvalidGeneration => Ok(b"ERR invalid flow policy generation"),
        PolicyError::StaleGeneration => Ok(b"ERR stale flow policy generation"),
        PolicyError::GenerationExhausted => Ok(b"ERR flow policy generation exhausted"),
        _ => Err(schema(PolicySchemaError::InvalidAuthority)),
    }
}
fn outcome(error: RequestValueError) -> RawError {
    RawError::Replace(ExistingError::Outcome(error))
}
