//! Canonical mathematical allocator intent and captured-order transport.

use super::super::admission::charge_schema;
use super::super::{
    CompletePolicyEncodingError as Error, PolicySchemaCharge, PolicySchemaError,
    PolicySchemaLimits, RawFlowPolicyValue as Raw,
};
use super::CompletePolicyRequestError;
use crate::workflow::admission::AdmissionLimits;

mod canonical;
mod codec;
mod model;
mod raw;
mod scan;

pub use model::{
    OriginalFlowPolicyReplaceFrameHeader, OriginalFlowPolicyReplaceInput,
    ScannedOriginalFlowPolicyReplaceFrame,
};
pub(in crate::policy::complete) use raw::FlowPolicyFrameCursor;

const PREFIX: &[u8] = b"WF-INTENT\0ORIGINAL-FLOW-REPLACE\0V1\0";
const MAX_BYTES: usize = 64 * 1024 * 1024;
const MAX_FRAME: usize = 96 * 1024 * 1024;

/// Prepares a canonical scaffold, digest position and exact intent range.
///
/// No hash or reusable payment/origin credential is returned. Ports performs both
/// actual hashes after the same-account prospective admissions succeed.
/// # Errors
/// Preserves profile, scope, represented-class, bounds, allocation and work errors.
pub fn prepare_original_flow_policy_replace_frame_bytes(
    input: OriginalFlowPolicyReplaceInput<'_>,
    limits: PolicySchemaLimits,
    admission: AdmissionLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<(Vec<u8>, usize, std::ops::Range<usize>), CompletePolicyRequestError> {
    profile(limits, charge)?;
    visit(charge, 128)?;
    visit(charge, 128)?;
    check_scope(
        input.scope.tenant().as_bytes(),
        input.scope.group().as_bytes(),
        input.scope.recovery().as_bytes(),
    )?;
    let mut metrics = (0, 0, 0);
    visible(input.policy_key.len(), &mut metrics.2, charge)?;
    for term in [input.expected, input.patch] {
        preflight(term, 0, &mut metrics, charge)?;
    }
    let scratch = metrics
        .0
        .checked_mul(8)
        .and_then(|n| metrics.1.checked_mul(128).and_then(|d| n.checked_add(d)))
        .and_then(|n| n.checked_add(256))
        .ok_or_else(overflow)?;
    charge_schema(charge, 0, u64::try_from(scratch).map_err(|_| overflow())?)?;
    visit(
        charge,
        add(128, metrics.0.checked_mul(8).ok_or_else(overflow)?)?,
    )?;
    let mut slots = Vec::new();
    slots
        .try_reserve_exact(metrics.0)
        .map_err(|_| Error::AllocationFailure)?;
    let result = codec::prepare(&input, admission, &mut slots, charge);
    drop(input);
    result
}

/// Scans exact contiguous structure without owning an adapter tree or index.
/// # Errors
/// Preserves profile, frame, canonical grammar, scope and prospective work errors.
pub fn scan_original_flow_policy_replace_frame<'frame>(
    encoded: &'frame [u8],
    limits: PolicySchemaLimits,
    admission: AdmissionLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<ScannedOriginalFlowPolicyReplaceFrame<'frame>, CompletePolicyRequestError> {
    profile(limits, charge)?;
    cap(encoded.len(), admission)?;
    let (scanned, tenant) = scan::frame(encoded, limits, charge)?;
    visit(charge, add(128, add(tenant.len(), scanned.tenant.len())?)?)?;
    if tenant != scanned.tenant {
        return Err(Error::Malformed.into());
    }
    visit(charge, add(128, scanned.original_intent.len())?)?;
    Ok(scanned)
}

/// Restores independent header owners after comparing the freshly supplied hash.
/// # Errors
/// Preserves profile, digest, owner reservation and every reached tail refusal.
pub fn restore_original_flow_policy_replace_frame_header<'frame>(
    scanned: ScannedOriginalFlowPolicyReplaceFrame<'frame>,
    recomputed: [u8; 32],
    limits: PolicySchemaLimits,
    admission: AdmissionLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<OriginalFlowPolicyReplaceFrameHeader<'frame>, CompletePolicyRequestError> {
    profile(limits, charge)?;
    cap(scanned.encoded.len(), admission)?;
    visit(charge, 192)?;
    if scanned.digest != recomputed {
        return Err(CompletePolicyRequestError::IntentMismatch);
    }
    scanned.restore(recomputed, charge)
}

fn profile(limits: PolicySchemaLimits, charge: &PolicySchemaCharge) -> Result<(), Error> {
    if charge.profile() != limits {
        return Err(Error::Schema(PolicySchemaError::ProfileMismatch));
    }
    Ok(())
}

fn cap(length: usize, admission: AdmissionLimits) -> Result<(), Error> {
    if length > MAX_FRAME || length > admission.decision_bytes() {
        return Err(Error::ByteLimit);
    }
    Ok(())
}

fn check_scope(
    tenant: &[u8],
    group: &[u8],
    recovery: &[u8],
) -> Result<usize, CompletePolicyRequestError> {
    let length = add(18, add(tenant.len(), add(group.len(), recovery.len())?)?)?;
    if tenant.is_empty()
        || group.is_empty()
        || recovery.is_empty()
        || tenant.len() > 8192
        || group.len() > 256
        || recovery.len() > 256
    {
        return Err(Error::Malformed.into());
    }
    if length > 8192 {
        return Err(CompletePolicyRequestError::ScopeLimit);
    }
    Ok(length)
}

fn copy(bytes: &[u8], charge: &mut PolicySchemaCharge) -> Result<Vec<u8>, Error> {
    visit(charge, add(128, bytes.len())?)?;
    let mut output = Vec::new();
    output
        .try_reserve_exact(bytes.len())
        .map_err(|_| Error::AllocationFailure)?;
    output.extend_from_slice(bytes);
    Ok(output)
}

fn preflight(
    input: Raw<'_>,
    depth: usize,
    metrics: &mut (usize, usize, usize),
    charge: &mut PolicySchemaCharge,
) -> Result<(), Error> {
    visit(charge, 128)?;
    metrics.0 = add(metrics.0, 1)?;
    if metrics.0 > 65_536 || depth > 64 {
        return Err(Error::ItemLimit);
    }
    metrics.1 = metrics.1.max(depth);
    match input {
        Raw::Nil | Raw::Integer(_) => (),
        Raw::Atom(bytes) => visible(bytes.len(), &mut metrics.2, charge)?,
        Raw::Binary(bytes) => visible(bytes.len(), &mut metrics.2, charge)?,
        Raw::BigInteger { little_endian, .. } => {
            visible(little_endian.len(), &mut metrics.2, charge)?;
        }
        Raw::Map(rows) => {
            for &(key, value) in rows {
                preflight(key, depth + 1, metrics, charge)?;
                preflight(value, depth + 1, metrics, charge)?;
            }
        }
        Raw::List(rows) | Raw::Tuple(rows) => {
            for &child in rows {
                preflight(child, depth + 1, metrics, charge)?;
            }
        }
        Raw::ImproperList { prefix, tail } => {
            if prefix.is_empty() {
                return Err(Error::Malformed);
            }
            for &child in prefix {
                preflight(child, depth + 1, metrics, charge)?;
            }
            preflight(*tail, depth + 1, metrics, charge)?;
        }
        Raw::Other => return Err(Error::UnsupportedTag),
    }
    Ok(())
}

fn visible(length: usize, bytes: &mut usize, charge: &mut PolicySchemaCharge) -> Result<(), Error> {
    *bytes = add(*bytes, length)?;
    if *bytes > MAX_BYTES {
        return Err(Error::ByteLimit);
    }
    visit(charge, length)
}

fn visit(charge: &mut PolicySchemaCharge, length: usize) -> Result<(), Error> {
    charge_schema(charge, u64::try_from(length).map_err(|_| overflow())?, 0).map_err(Error::Schema)
}
fn add(left: usize, right: usize) -> Result<usize, Error> {
    left.checked_add(right).ok_or_else(overflow)
}
fn overflow() -> Error {
    Error::Schema(PolicySchemaError::LengthOverflow)
}

fn write_header(
    input: &OriginalFlowPolicyReplaceInput<'_>,
    ilen: usize,
    slen: usize,
    sink: &mut codec::FlowFrameSink<'_>,
    charge: &mut PolicySchemaCharge,
) -> Result<usize, Error> {
    visit(charge, 128)?;
    sink.write(b"FWCP", charge)?;
    sink.write(&1_u16.to_be_bytes(), charge)?;
    sink.write(&[8], charge)?;
    for bytes in [
        input.scope.tenant().as_bytes(),
        input.scope.group().as_bytes(),
        input.scope.recovery().as_bytes(),
        input.request.as_bytes(),
    ] {
        sink.raw(bytes, charge)?;
    }
    sink.write(&3_u16.to_be_bytes(), charge)?;
    let digest = sink.position();
    sink.write(&[0; 32], charge)?;
    sink.write(&input.stamp.hlc().physical().get().to_be_bytes(), charge)?;
    sink.write(&input.stamp.hlc().logical().to_be_bytes(), charge)?;
    sink.write(&input.stamp.wall().get().to_be_bytes(), charge)?;
    sink.u32(ilen, charge)?;
    sink.u32(slen, charge)?;
    Ok(digest)
}
