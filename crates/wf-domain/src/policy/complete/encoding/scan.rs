//! Complete borrowed preflight before semantic or envelope output.

use std::collections::BTreeSet;
use std::ops::Range;

use crate::policy::installation::{MAX_POLICY_GENERATION, MAX_POLICY_SNAPSHOT_BYTES, PolicyError};

use super::super::admission::charge_schema;
use super::{
    CompletePolicyEncodingError, PolicySchemaCharge, PolicySchemaError, PolicySchemaLimits, Reader,
};

mod governance;

pub(super) fn scan_value(
    encoded: &[u8],
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<(u64, Range<usize>), CompletePolicyEncodingError> {
    let mut reader = Reader::new(encoded, limits, charge);
    reader.node()?;
    if reader.take(4)? != b"FWPV" {
        return Err(CompletePolicyEncodingError::Malformed);
    }
    if reader.u16()? != 2 {
        return Err(CompletePolicyEncodingError::UnsupportedVersion);
    }
    let generation = reader.u64()?;
    if generation == 0 || generation > MAX_POLICY_GENERATION {
        return Err(CompletePolicyEncodingError::Schema(
            PolicySchemaError::Policy(PolicyError::InvalidGeneration),
        ));
    }
    let length = usize::try_from(reader.u32()?)
        .map_err(|_| CompletePolicyEncodingError::Schema(PolicySchemaError::LengthOverflow))?;
    if length > MAX_POLICY_SNAPSHOT_BYTES {
        return Err(CompletePolicyEncodingError::ByteLimit);
    }
    let start = reader.position();
    let end = start
        .checked_add(length)
        .ok_or(CompletePolicyEncodingError::Schema(
            PolicySchemaError::LengthOverflow,
        ))?;
    if encoded.get(start..end).is_none() {
        return Err(CompletePolicyEncodingError::Malformed);
    }
    if end != encoded.len() {
        return Err(CompletePolicyEncodingError::TrailingBytes);
    }
    let mut snapshot = reader.region(start..end)?;
    scan_snapshot(&mut snapshot)?;
    snapshot.finish()?;
    Ok((generation, start..end))
}

fn scan_snapshot(reader: &mut Reader<'_, '_>) -> Result<(), CompletePolicyEncodingError> {
    reader.node()?;
    if reader.take(4)? != b"FWPS" {
        return Err(CompletePolicyEncodingError::Malformed);
    }
    if reader.u16()? != 2 {
        return Err(CompletePolicyEncodingError::UnsupportedVersion);
    }
    scan_data_fields(reader)
}

pub(in crate::policy::complete) fn scan_data_fields(
    reader: &mut Reader<'_, '_>,
) -> Result<(), CompletePolicyEncodingError> {
    reader.raw(MAX_POLICY_SNAPSHOT_BYTES)?;
    reader.optional(scan_version)?;
    reader.optional(Reader::u64)?;
    reader.optional(scan_retry)?;
    reader.optional(scan_retention)?;
    let count = reader.count(reader.limits.policy_limits().max_states())?;
    let mut previous = None;
    for _ in 0..count {
        reader.node()?;
        let key = reader.raw(MAX_POLICY_SNAPSHOT_BYTES)?;
        if let Some(previous) = previous {
            reader.keys(previous, key, MAX_POLICY_SNAPSHOT_BYTES)?;
        }
        previous = Some(key);
        scan_state(reader)?;
    }
    reader.optional(|reader| scan_names(reader, 3, 64))?;
    reader.optional(|reader| reader.raw(64))?;
    reader.optional(governance::scan_governance)?;
    Ok(())
}

fn scan_state(reader: &mut Reader<'_, '_>) -> Result<(), CompletePolicyEncodingError> {
    reader.optional(|reader| match reader.byte()? {
        0 | 1 => Ok(()),
        _ => Err(CompletePolicyEncodingError::UnsupportedTag),
    })?;
    reader.optional(scan_retry)?;
    reader.optional(scan_retention)?;
    reader.optional(governance::scan_governance)?;
    Ok(())
}

fn scan_version(reader: &mut Reader<'_, '_>) -> Result<(), CompletePolicyEncodingError> {
    match reader.byte()? {
        0 => {
            reader.raw(MAX_POLICY_SNAPSHOT_BYTES)?;
        }
        1 => {
            reader.u64()?;
        }
        _ => return Err(CompletePolicyEncodingError::UnsupportedTag),
    }
    Ok(())
}

fn scan_retry(reader: &mut Reader<'_, '_>) -> Result<(), CompletePolicyEncodingError> {
    reader.node()?;
    reader.optional(Reader::u16)?;
    reader.optional(scan_backoff)?;
    reader.optional(|reader| reader.raw(MAX_POLICY_SNAPSHOT_BYTES))?;
    Ok(())
}

fn scan_backoff(reader: &mut Reader<'_, '_>) -> Result<(), CompletePolicyEncodingError> {
    reader.node()?;
    if reader.byte()? > 3 {
        return Err(CompletePolicyEncodingError::UnsupportedTag);
    }
    reader.u64()?;
    reader.u64()?;
    reader.byte()?;
    Ok(())
}

fn scan_retention(reader: &mut Reader<'_, '_>) -> Result<(), CompletePolicyEncodingError> {
    reader.node()?;
    reader.optional(Reader::u64)?;
    reader.optional(Reader::u32)?;
    Ok(())
}

pub(super) fn scan_names(
    reader: &mut Reader<'_, '_>,
    maximum_count: usize,
    maximum_bytes: usize,
) -> Result<(), CompletePolicyEncodingError> {
    let start = reader.position();
    let count = reader.count(maximum_count)?;
    for _ in 0..count {
        reader.node()?;
        reader.raw(maximum_bytes)?;
    }
    let end = reader.position();
    let scratch = u64::try_from(count)
        .ok()
        .and_then(|n| n.checked_mul(64))
        .ok_or(CompletePolicyEncodingError::Schema(
            PolicySchemaError::LengthOverflow,
        ))?;
    charge_schema(reader.charge, 0, scratch).map_err(CompletePolicyEncodingError::Schema)?;
    let mut replay = reader.region(start..end)?;
    let count = replay.count(maximum_count)?;
    let mut names = BTreeSet::new();
    for _ in 0..count {
        replay.node()?;
        let name = replay.raw(maximum_bytes)?;
        let work = u64::try_from(name.len())
            .ok()
            .and_then(|n| n.checked_add(128))
            .ok_or(CompletePolicyEncodingError::Schema(
                PolicySchemaError::LengthOverflow,
            ))?;
        charge_schema(replay.charge, work, 0).map_err(CompletePolicyEncodingError::Schema)?;
        if !names.insert(name) {
            return Err(CompletePolicyEncodingError::Malformed);
        }
    }
    replay.finish()
}
