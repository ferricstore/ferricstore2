//! Public Policy.set controls and raw patch preparation, without dispatch.

use super::RawFlowPolicyValue as Raw;
use super::admission::charge_schema;
use super::{
    CompletePolicyEncodingError as Error, PolicySchemaCharge, PolicySchemaError, PolicySchemaLimits,
};
use model::{PolicySetPatchArena, PolicySetPatchCell, PolicySetPatchSlot};

pub(in crate::policy::complete) mod merge;
mod metric;
mod model;
pub(in crate::policy::complete) mod native_order;
pub(in crate::policy::complete) mod order;
mod patch;

pub use model::{
    OriginalPolicySetInputRejection, OriginalPolicySetPatchView, OriginalPolicySetRequest,
};
pub use order::OriginalPolicySetMapOrders;

/// Prepares original public controls and converted raw patch without admission.
///
/// Both replace values are retained. The returned borrowed data provides no
/// committed identity, captured-order, merge, current or installation proof.
/// Each supplied `RawFlowPolicyValue::Map` must represent an ordinary source
/// map with mathematically unique keys and captured reduce order, including
/// nil, integer and proper-list aliases; this API does not certify that input
/// condition. Opaque `Other` and reached struct `Enumerable` behavior remain
/// temporary unsupported scope.
///
/// # Errors
/// Outer errors retain bounded operational/work/profile and allocation errors;
/// inner errors are the reached original public caller diagnoses only.
pub fn prepare_original_policy_set_request<'input>(
    workflow_type: &Raw<'input>,
    opts: &Raw<'input>,
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<Result<OriginalPolicySetRequest<'input>, OriginalPolicySetInputRejection>, Error> {
    profile(limits, charge)?;
    debit(charge, 128)?;
    let mut bounds = (0, 0, 0);
    preflight(*workflow_type, 0, &mut bounds, charge)?;
    preflight(*opts, 0, &mut bounds, charge)?;
    debit(charge, 128)?;
    let Raw::Binary(workflow_type) = *workflow_type else {
        return Ok(Err(OriginalPolicySetInputRejection::Options));
    };
    if !is_list(*opts) {
        return Ok(Err(OriginalPolicySetInputRejection::Options));
    }
    let Some(controls) = controls(*opts, charge)? else {
        return Ok(Err(OriginalPolicySetInputRejection::Options));
    };
    debit(charge, 128)?;
    if workflow_type.is_empty() {
        return Ok(Err(OriginalPolicySetInputRejection::Type));
    }
    debit(charge, 128)?;
    let key_len = workflow_type.len().checked_add(13).ok_or_else(overflow)?;
    if key_len > 65_535 {
        return Ok(Err(OriginalPolicySetInputRejection::KeyBytes));
    }
    debit(charge, 128)?;
    let response_state = match controls[0] {
        None | Some(Raw::Nil | Raw::Atom("nil")) => None,
        Some(Raw::Binary(value)) => Some(value),
        _ => return Ok(Err(OriginalPolicySetInputRejection::State)),
    };
    debit(charge, 128)?;
    let expected_generation = match controls[1] {
        None | Some(Raw::Nil | Raw::Atom("nil")) => None,
        Some(value) => match unsigned(value, charge)? {
            Some(value) if value <= 9_007_199_254_740_991 => Some(value),
            _ => return Ok(Err(OriginalPolicySetInputRejection::ExpectedGeneration)),
        },
    };
    debit(charge, 128)?;
    let replace = match controls[2] {
        None | Some(Raw::Atom("false")) => false,
        Some(Raw::Atom("true")) => true,
        _ => return Ok(Err(OriginalPolicySetInputRejection::Replace)),
    };
    let (mut arena, mut work) = PolicySetPatchArena::new(bounds.0, bounds.2, charge)?;
    let root = patch::normalize(*opts, &mut arena, &mut work, charge)?;
    drop(work);
    if metric::measure(&arena, root, charge)? > 262_144 {
        return Ok(Err(OriginalPolicySetInputRejection::SnapshotBytes));
    }
    debit(charge, 128 + key_len)?;
    let mut policy_key = Vec::new();
    policy_key
        .try_reserve_exact(key_len)
        .map_err(|_| Error::AllocationFailure)?;
    policy_key.extend_from_slice(b"f:{f}:policy:");
    policy_key.extend_from_slice(workflow_type);
    debit(charge, 128)?;
    arena.root = root;
    Ok(Ok(OriginalPolicySetRequest {
        policy_key,
        expected_generation,
        replace,
        response_state,
        patch: arena,
        reservation: (bounds.0, bounds.2),
    }))
}

/// Encodes one exact public preparation diagnosis into one fallible owner.
///
/// # Errors
/// Retains profile/work/checked-size/allocation errors on the supplied account.
pub fn encode_original_policy_set_input_rejection_body(
    reason: OriginalPolicySetInputRejection,
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<Vec<u8>, Error> {
    profile(limits, charge)?;
    debit(charge, 128)?;
    let bytes = reason.as_bytes();
    debit(charge, 128 + bytes.len())?;
    let mut result = Vec::new();
    result
        .try_reserve_exact(bytes.len())
        .map_err(|_| Error::AllocationFailure)?;
    result.extend_from_slice(bytes);
    Ok(result)
}

fn profile(limits: PolicySchemaLimits, charge: &PolicySchemaCharge) -> Result<(), Error> {
    if charge.profile() != limits {
        return Err(Error::Schema(PolicySchemaError::ProfileMismatch));
    }
    Ok(())
}

fn preflight(
    value: Raw<'_>,
    depth: usize,
    bounds: &mut (usize, usize, usize),
    charge: &mut PolicySchemaCharge,
) -> Result<(), Error> {
    debit(charge, 128)?;
    bounds.0 = bounds.0.checked_add(1).ok_or_else(overflow)?;
    if bounds.0 > 131_072 || depth > 64 {
        return Err(Error::ItemLimit);
    }
    bounds.2 = bounds.2.max(depth);
    match value {
        Raw::Atom(text) => {
            text_bound(text.len(), bounds, charge)?;
            if text.chars().count() > 255 {
                return Err(authority());
            }
        }
        Raw::Binary(bytes)
        | Raw::BigInteger {
            little_endian: bytes,
            ..
        } => text_bound(bytes.len(), bounds, charge)?,
        Raw::Map(rows) => {
            for &(key, value) in rows {
                preflight(key, depth + 1, bounds, charge)?;
                preflight(value, depth + 1, bounds, charge)?;
            }
        }
        Raw::List(items) | Raw::Tuple(items) => {
            for &item in items {
                preflight(item, depth + 1, bounds, charge)?;
            }
        }
        Raw::ImproperList { prefix, tail } => {
            if prefix.is_empty() {
                return Err(Error::Malformed);
            }
            for &item in prefix {
                preflight(item, depth + 1, bounds, charge)?;
            }
            preflight(*tail, depth + 1, bounds, charge)?;
        }
        Raw::Nil | Raw::Integer(_) | Raw::Other => {}
    }
    Ok(())
}

fn text_bound(
    length: usize,
    bounds: &mut (usize, usize, usize),
    charge: &mut PolicySchemaCharge,
) -> Result<(), Error> {
    bounds.1 = bounds.1.checked_add(length).ok_or_else(overflow)?;
    if bounds.1 > 67_108_864 {
        return Err(Error::ByteLimit);
    }
    debit(charge, length)
}

fn controls<'input>(
    opts: Raw<'input>,
    charge: &mut PolicySchemaCharge,
) -> Result<Option<[Option<Raw<'input>>; 3]>, Error> {
    debit(charge, 128)?;
    let mut found = [None; 3];
    let mut index = 0;
    loop {
        let (row, proper) = item(opts, index, charge)?;
        let Some(row) = row else {
            return Ok(proper.then_some(found));
        };
        let Raw::Tuple(pair) = row else {
            return Ok(None);
        };
        if pair.len() != 2 {
            return Ok(None);
        }
        let Some(key) = atom(pair[0]) else {
            return Ok(None);
        };
        if let Some(field) = control_name(key, charge)?
            && found[field].is_none()
        {
            found[field] = Some(pair[1]);
        }
        index += 1;
    }
}

pub(super) fn control_name(
    key: &[u8],
    charge: &mut PolicySchemaCharge,
) -> Result<Option<usize>, Error> {
    for (index, name) in [b"state".as_slice(), b"expected_generation", b"replace"]
        .into_iter()
        .enumerate()
    {
        debit(charge, 128 + key.len() + name.len())?;
        if key == name {
            return Ok(Some(index));
        }
    }
    Ok(None)
}

pub(super) fn atom(value: Raw<'_>) -> Option<&[u8]> {
    match value {
        Raw::Nil => Some(b"nil"),
        Raw::Atom(text) => Some(text.as_bytes()),
        _ => None,
    }
}

pub(super) const fn is_list(value: Raw<'_>) -> bool {
    matches!(value, Raw::List(_) | Raw::ImproperList { .. })
}

pub(super) fn item<'input>(
    value: Raw<'input>,
    index: usize,
    charge: &mut PolicySchemaCharge,
) -> Result<(Option<Raw<'input>>, bool), Error> {
    match value {
        Raw::List(items) => {
            if index < items.len() {
                debit(charge, 128)?;
            }
            Ok((items.get(index).copied(), true))
        }
        Raw::ImproperList { prefix, tail } => {
            if index < prefix.len() {
                debit(charge, 128)?;
                Ok((prefix.get(index).copied(), false))
            } else {
                debit(charge, 128)?;
                item(*tail, index - prefix.len(), charge)
            }
        }
        _ => Ok((None, false)),
    }
}

pub(super) fn row<'input>(
    value: Raw<'input>,
    index: usize,
    charge: &mut PolicySchemaCharge,
) -> Result<(Raw<'input>, Raw<'input>), Error> {
    if let Raw::Map(rows) = value {
        debit(charge, 128)?;
        return rows.get(index).copied().ok_or(Error::Malformed);
    }
    let (Some(Raw::Tuple(pair)), _) = item(value, index, charge)? else {
        return Err(Error::Malformed);
    };
    if pair.len() != 2 {
        return Err(Error::Malformed);
    }
    Ok((pair[0], pair[1]))
}

pub(super) fn significant<'input>(
    bytes: &'input [u8],
    charge: &mut PolicySchemaCharge,
) -> Result<&'input [u8], Error> {
    debit(charge, 128 + bytes.len())?;
    let length = bytes
        .iter()
        .rposition(|&byte| byte != 0)
        .map_or(0, |index| index + 1);
    Ok(&bytes[..length])
}

fn unsigned(value: Raw<'_>, charge: &mut PolicySchemaCharge) -> Result<Option<u64>, Error> {
    match value {
        Raw::Integer(number) => Ok(u64::try_from(number).ok()),
        Raw::BigInteger {
            negative,
            little_endian,
        } => {
            let bytes = significant(little_endian, charge)?;
            if bytes.is_empty() {
                return Ok(Some(0));
            }
            if negative || bytes.len() > 8 {
                return Ok(None);
            }
            let mut number = 0_u64;
            for (index, &byte) in bytes.iter().enumerate() {
                number |= u64::from(byte) << (8 * index);
            }
            Ok(Some(number))
        }
        _ => Ok(None),
    }
}

pub(super) fn debit(charge: &mut PolicySchemaCharge, bytes: usize) -> Result<(), Error> {
    charge_schema(charge, u64::try_from(bytes).map_err(|_| overflow())?, 0).map_err(Error::Schema)
}

pub(super) fn overflow() -> Error {
    Error::Schema(PolicySchemaError::LengthOverflow)
}
pub(super) fn authority() -> Error {
    Error::Schema(PolicySchemaError::InvalidAuthority)
}

pub use super::flow_input::rejection::original_policy_set_rejection_from_body;
