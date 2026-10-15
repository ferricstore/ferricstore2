//! Pure public GET ingress, with no SET conversion or reader capability.

use super::RawFlowPolicyValue as Raw;
use super::admission::charge_schema;
use super::{
    CompletePolicyEncodingError as Error, PolicySchemaCharge, PolicySchemaError, PolicySchemaLimits,
};

mod model;

pub use model::{OriginalPolicyGetRejection, OriginalPolicyGetRequest};

/// Prepares exact public GET controls and its single owned allocator key.
///
/// Full keyword shape precedes type, first state and key diagnoses. Unrelated
/// values are opaque: this is no whole-ingress or hidden-payload validity proof.
/// Only reached atom keys are checked against their represented source limit.
/// One advancing list cursor uses constant scratch and freshly charged work.
///
/// The final debit prospectively covers one immediately following ports read;
/// this function performs no I/O and the result grants no executed-read credit.
/// This invocation admits its key owner only. Other caller-held requests and
/// original payloads remain external to that stage-specific logical bound.
///
/// # Errors
/// Outer errors preserve profile/work/represented-class/reservation failures.
/// Inner errors are only the reached original caller input diagnoses.
pub fn prepare_original_policy_get_request<'input>(
    workflow_type: &Raw<'input>,
    opts: &Raw<'input>,
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<Result<OriginalPolicyGetRequest<'input>, OriginalPolicyGetRejection>, Error> {
    use OriginalPolicyGetRejection as Rejection;

    if charge.profile() != limits {
        return Err(Error::Schema(PolicySchemaError::ProfileMismatch));
    }
    pay(charge, 128)?;
    pay(charge, 128)?;
    let Raw::Binary(workflow_type) = *workflow_type else {
        return Ok(Err(Rejection::Options));
    };
    if !matches!(opts, Raw::List(_) | Raw::ImproperList { .. }) {
        return Ok(Err(Rejection::Options));
    }
    let first = match first_state(*opts, charge)? {
        Ok(first) => first,
        Err(reason) => return Ok(Err(reason)),
    };
    pay(charge, 128)?;
    if workflow_type.is_empty() {
        return Ok(Err(Rejection::Type));
    }
    pay(charge, 128)?;
    let response_state = match first {
        None | Some(Raw::Nil | Raw::Atom("nil")) => None,
        Some(Raw::Binary(state)) => Some(state),
        _ => return Ok(Err(Rejection::State)),
    };
    pay(charge, 128)?;
    let length = workflow_type.len().checked_add(13).ok_or_else(overflow)?;
    if length > 65_535 {
        return Ok(Err(Rejection::KeyBytes));
    }
    let bound = length.checked_add(128).ok_or_else(overflow)?;
    charge_schema(charge, 0, u64::try_from(bound).map_err(|_| overflow())?)
        .map_err(Error::Schema)?;
    pay(charge, bound)?;
    let mut policy_key = Vec::new();
    policy_key
        .try_reserve_exact(length)
        .map_err(|_| Error::AllocationFailure)?;
    policy_key.extend_from_slice(b"f:{f}:policy:");
    policy_key.extend_from_slice(workflow_type);
    pay(charge, 128)?;
    let request = OriginalPolicyGetRequest {
        policy_key,
        workflow_type,
        response_state,
        profile: limits,
    };
    pay(charge, bound)?;
    Ok(Ok(request))
}

fn first_state<'input>(
    mut current: Raw<'input>,
    charge: &mut PolicySchemaCharge,
) -> Result<Result<Option<Raw<'input>>, OriginalPolicyGetRejection>, Error> {
    use OriginalPolicyGetRejection as Rejection;

    pay(charge, 128)?;
    let mut first = None;
    loop {
        let (rows, tail) = match current {
            Raw::List(rows) => (rows, None),
            Raw::ImproperList { prefix, tail } => {
                if prefix.is_empty() {
                    return Err(Error::Malformed);
                }
                (prefix, Some(tail))
            }
            _ => return Ok(Err(Rejection::Options)),
        };
        for row in rows {
            pay(charge, 128)?;
            let Raw::Tuple(pair) = *row else {
                return Ok(Err(Rejection::Options));
            };
            if pair.len() != 2 {
                return Ok(Err(Rejection::Options));
            }
            let name = match pair[0] {
                Raw::Nil => "nil",
                Raw::Atom(name) => name,
                _ => return Ok(Err(Rejection::Options)),
            };
            pay(charge, name.len().checked_add(128).ok_or_else(overflow)?)?;
            if name.chars().take(256).count() > 255 {
                return Err(Error::ByteLimit);
            }
            if first.is_none() {
                pay(charge, name.len().checked_add(133).ok_or_else(overflow)?)?;
                if name == "state" {
                    first = Some(pair[1]);
                }
            }
        }
        let Some(tail) = tail else {
            return Ok(Ok(first));
        };
        pay(charge, 128)?;
        current = *tail;
    }
}

fn pay(charge: &mut PolicySchemaCharge, fee: usize) -> Result<(), Error> {
    charge_schema(charge, u64::try_from(fee).map_err(|_| overflow())?, 0).map_err(Error::Schema)
}

fn overflow() -> Error {
    Error::Schema(PolicySchemaError::LengthOverflow)
}
