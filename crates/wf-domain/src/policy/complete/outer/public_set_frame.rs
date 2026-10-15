//! Both-mode public identity, bounded canonical patch and server capture transport.

use super::super::admission::charge_schema;
use super::super::{
    CompletePolicyEncodingError as Error, OriginalPolicySetMapOrders, PolicySchemaCharge,
    PolicySchemaError, PolicySchemaLimits,
};
use super::CompletePolicyRequestError;
use crate::workflow::admission::AdmissionLimits;
use codec::PolicySetFrameSink;
use model::write_header;
use value::PolicySetFrameValue as Value;

mod canonical;
mod codec;
mod model;
mod scan;
mod value;

pub use model::{
    OriginalPolicySetFrameHeader, OriginalPolicySetFrameInput, ScannedOriginalPolicySetFrame,
};
pub(in crate::policy::complete) use value::PolicySetFrameCursor;

const PREFIX: &[u8] = b"WF-INTENT\0ORIGINAL-POLICY-SET\0V1\0";
const MAX_BYTES: usize = 64 * 1024 * 1024;
const MAX_PATCH: usize = 262_144;
const MAX_NODES: usize = 131_072;
const MAX_FRAME: usize = 128 * 1024 * 1024;

/// Final encoded owner, intent-digest offset and exact intent/patch hash ranges.
pub type OriginalPolicySetFrameBytes = (
    Vec<u8>,
    usize,
    std::ops::Range<usize>,
    std::ops::Range<usize>,
);

/// Produces one final owner and exact intent/patch ranges, without hashing.
///
/// Prospective work covers the real ports hash order and final capture comparison.
/// # Errors
/// Preserves profile, structural bounds, capture, allocation and work refusals.
pub fn prepare_original_policy_set_frame_bytes(
    input: OriginalPolicySetFrameInput<'_, '_, '_>,
    limits: PolicySchemaLimits,
    admission: AdmissionLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<OriginalPolicySetFrameBytes, CompletePolicyRequestError> {
    profile(limits, charge)?;
    visit(charge, 128)?;
    visit(charge, 128)?;
    check_scope(
        input.scope.tenant().as_bytes(),
        input.scope.group().as_bytes(),
        input.scope.recovery().as_bytes(),
    )?;
    let key = input.prepared.policy_key();
    visit(charge, key.len())?;
    if key.is_empty() || key.len() > 65_535 {
        return Err(Error::ByteLimit.into());
    }
    let mut metrics = (0, 0, 0);
    let original = (
        input.prepared.reservation_count(),
        input.prepared.reservation_depth(),
    );
    value::preflight(
        Value::Prepared(input.prepared.patch()),
        0,
        original,
        &mut metrics,
        charge,
    )?;
    let backing = input.orders.admit_patch(metrics.0, charge)?;
    let arena = add(
        original.0.checked_mul(72).ok_or_else(overflow)?,
        add(key.len(), 256)?,
    )?;
    let header = header_backing(&input)?;
    let scratch = add(
        add(
            metrics.0.checked_mul(8).ok_or_else(overflow)?,
            metrics.1.checked_mul(128).ok_or_else(overflow)?,
        )?,
        256,
    )?;
    let live = add(add(arena, header)?, add(backing, scratch)?)?;
    peak(charge, live)?;
    visit(
        charge,
        add(128, metrics.0.checked_mul(8).ok_or_else(overflow)?)?,
    )?;
    let mut slots = Vec::new();
    slots
        .try_reserve_exact(metrics.0)
        .map_err(|_| Error::AllocationFailure)?;
    let prepared = prepare_frame(&input, admission, live, &mut slots, charge);
    drop(input);
    prepared
}

/// Scans one exact borrowed frame without constructing an adapter tree.
/// # Errors
/// Preserves profile, frame, canonical grammar and prospective work errors.
pub fn scan_original_policy_set_frame<'frame>(
    encoded: &'frame [u8],
    limits: PolicySchemaLimits,
    admission: AdmissionLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<ScannedOriginalPolicySetFrame<'frame>, CompletePolicyRequestError> {
    profile(limits, charge)?;
    cap(encoded.len(), admission)?;
    let (scanned, tenant) = scan::frame(encoded, limits, charge)?;
    visit(charge, add(128, add(tenant.len(), scanned.tenant.len())?)?)?;
    if tenant != scanned.tenant {
        return Err(Error::Malformed.into());
    }
    visit(charge, add(128, scanned.original_intent.len())?)?;
    visit(charge, add(128, scanned.canonical_patch().len())?)?;
    Ok(scanned)
}

/// Compares fresh hashes/capture facts before restoring independent header owners.
/// # Errors
/// Preserves profile, digest, correspondence, work and allocation refusals.
pub fn restore_original_policy_set_frame_header<'frame>(
    scanned: ScannedOriginalPolicySetFrame<'frame>,
    intent: [u8; 32],
    patch: [u8; 32],
    orders: &OriginalPolicySetMapOrders<'_>,
    limits: PolicySchemaLimits,
    admission: AdmissionLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<OriginalPolicySetFrameHeader<'frame>, CompletePolicyRequestError> {
    profile(limits, charge)?;
    cap(scanned.encoded.len(), admission)?;
    visit(charge, 192)?;
    if scanned.digest != intent {
        return Err(CompletePolicyRequestError::IntentMismatch);
    }
    let backing = orders.admit_patch(scanned.patch_bounds.0, charge)?;
    orders.bind_patch(patch, charge)?;
    correspondence(scanned.patch, orders, charge)?;
    scanned.restore(intent, backing, charge)
}

fn correspondence(
    frame: PolicySetFrameCursor<'_>,
    orders: &OriginalPolicySetMapOrders<'_>,
    charge: &mut PolicySchemaCharge,
) -> Result<(), Error> {
    let bytes = frame.sidecar;
    let maps = value::word(bytes, 0).ok_or(Error::Malformed)?;
    for id in 0..maps {
        let start = value::word(bytes, add(4, id.checked_mul(4).ok_or_else(overflow)?)?)
            .ok_or(Error::Malformed)?;
        let pairs = value::word(bytes, add(start, 4)?).ok_or(Error::Malformed)?;
        let (p, q) =
            orders.patch_record(u32::try_from(id).map_err(|_| overflow())?, pairs, charge)?;
        for (half, expected) in [p, q].into_iter().enumerate() {
            for (index, &rank) in expected.iter().enumerate() {
                visit(charge, 16)?;
                let offset = add(
                    add(start, 8)?,
                    add(half.checked_mul(pairs).ok_or_else(overflow)?, index)?
                        .checked_mul(4)
                        .ok_or_else(overflow)?,
                )?;
                if value::word(bytes, offset) != usize::try_from(rank).ok() {
                    return Err(authority());
                }
            }
        }
    }
    visit(charge, 16)?;
    if maps != orders.patch_count() {
        return Err(authority());
    }
    Ok(())
}

fn header_backing(input: &OriginalPolicySetFrameInput<'_, '_, '_>) -> Result<usize, Error> {
    [
        input.scope.tenant().as_bytes(),
        input.scope.group().as_bytes(),
        input.scope.recovery().as_bytes(),
        input.request.as_bytes(),
    ]
    .into_iter()
    .try_fold(512, |n, bytes| add(n, bytes.len()))
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
fn check_scope(t: &[u8], g: &[u8], r: &[u8]) -> Result<usize, CompletePolicyRequestError> {
    let length = add(18, add(t.len(), add(g.len(), r.len())?)?)?;
    if t.is_empty()
        || g.is_empty()
        || r.is_empty()
        || t.len() > 8192
        || g.len() > 256
        || r.len() > 256
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
    let mut out = Vec::new();
    out.try_reserve_exact(bytes.len())
        .map_err(|_| Error::AllocationFailure)?;
    out.extend_from_slice(bytes);
    Ok(out)
}
fn visit(charge: &mut PolicySchemaCharge, n: usize) -> Result<(), Error> {
    charge_schema(charge, u64::try_from(n).map_err(|_| overflow())?, 0).map_err(Error::Schema)
}
fn peak(charge: &mut PolicySchemaCharge, n: usize) -> Result<(), Error> {
    charge_schema(charge, 0, u64::try_from(n).map_err(|_| overflow())?).map_err(Error::Schema)
}
fn add(a: usize, b: usize) -> Result<usize, Error> {
    a.checked_add(b).ok_or_else(overflow)
}
fn overflow() -> Error {
    Error::Schema(PolicySchemaError::LengthOverflow)
}
fn authority() -> Error {
    Error::Schema(PolicySchemaError::InvalidAuthority)
}

fn prepare_frame(
    input: &OriginalPolicySetFrameInput<'_, '_, '_>,
    admission: AdmissionLimits,
    live: usize,
    slots: &mut Vec<u32>,
    charge: &mut PolicySchemaCharge,
) -> Result<OriginalPolicySetFrameBytes, CompletePolicyRequestError> {
    let mut measure = PolicySetFrameSink::Measure {
        position: 0,
        maps: 0,
        records: 0,
        payload: 0,
    };
    let digest = write_header(input, 0, 0, &mut measure, charge)?;
    let begin = measure.position();
    let patch = codec::intent(input, &mut measure, slots, charge)?;
    let end = measure.position();
    let count = measure.maps();
    measure.side_root(count, charge)?;
    let PolicySetFrameSink::Measure {
        position, records, ..
    } = measure
    else {
        return Err(Error::Malformed.into());
    };
    let side_length = add(add(8, count.checked_mul(4).ok_or_else(overflow)?)?, records)?;
    let frame_length = add(position, records)?;
    cap(frame_length, admission)?;
    let intent_length = end - begin;
    if intent_length > MAX_FRAME {
        return Err(Error::ByteLimit.into());
    }
    visit(charge, 16)?;
    if count != input.orders.patch_count() {
        return Err(authority().into());
    }
    peak(charge, add(live, frame_length)?)?;
    visit(charge, add(128, frame_length)?)?;
    let mut output = Vec::new();
    output
        .try_reserve_exact(frame_length)
        .map_err(|_| Error::AllocationFailure)?;
    visit(charge, frame_length)?;
    output.resize(frame_length, 0);
    let mut emit = PolicySetFrameSink::Emit {
        output: &mut output,
        position: 0,
        maps: 0,
        intent: begin,
        side: end,
        record: add(end, add(8, count.checked_mul(4).ok_or_else(overflow)?)?)?,
    };
    write_header(input, intent_length, side_length, &mut emit, charge)?;
    emit.side_root(count, charge)?;
    let emitted_patch = codec::intent(input, &mut emit, slots, charge)?;
    if emitted_patch != patch {
        return Err(Error::Malformed.into());
    }
    let PolicySetFrameSink::Emit {
        position,
        maps,
        record,
        ..
    } = &emit
    else {
        return Err(Error::Malformed.into());
    };
    if *position != end || *maps != count || *record != frame_length {
        return Err(Error::Malformed.into());
    }
    emit.patch(
        add(end, add(4, count.checked_mul(4).ok_or_else(overflow)?)?)?,
        side_length,
        charge,
    )?;
    visit(charge, add(128, patch.len())?)?;
    visit(charge, add(128, intent_length)?)?;
    visit(charge, 160)?;
    visit(charge, add(128, frame_length)?)?;
    visit(charge, 192)?;
    Ok((output, digest, begin..end, patch))
}
