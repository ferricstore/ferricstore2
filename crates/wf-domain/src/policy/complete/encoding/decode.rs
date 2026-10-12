//! Paid semantic materialization after complete borrowed structural preflight.

use std::collections::BTreeMap;

use crate::policy::definition::{PolicyVersion, StateMode};
use crate::policy::installation::MAX_POLICY_SNAPSHOT_BYTES;
use crate::policy::{Backoff, BackoffKind, RetryOverride};
use crate::retention::RetentionOverride;

use super::super::admission::charge_schema;
use super::super::{CompletePolicyData, CompleteStatePolicyData};
use super::{CompletePolicyEncodingError, PolicySchemaError, Reader};

mod governance;

pub(super) fn decode_snapshot(
    reader: &mut Reader<'_, '_>,
) -> Result<CompletePolicyData, CompletePolicyEncodingError> {
    reader.node()?; // Structural snapshot.
    reader.node()?; // Owning normalized Data root.
    if reader.take(4)? != b"FWPS" {
        return Err(CompletePolicyEncodingError::Malformed);
    }
    if reader.u16()? != 2 {
        return Err(CompletePolicyEncodingError::UnsupportedVersion);
    }
    decode_data_fields(reader)
}

pub(in crate::policy::complete) fn decode_data_fields(
    reader: &mut Reader<'_, '_>,
) -> Result<CompletePolicyData, CompletePolicyEncodingError> {
    let bytes = reader.raw(MAX_POLICY_SNAPSHOT_BYTES)?;
    let workflow_type = copy_leaf(bytes, reader)?;
    let version = reader.optional(read_version)?;
    let max_active_ms = reader.optional(Reader::u64)?;
    let retry = reader.optional(read_retry)?;
    let retention = reader.optional(read_retention)?;
    let count = reader.count(reader.limits.policy_limits().max_states())?;
    reader.node()?; // Owning state map.
    let mut states = BTreeMap::new();
    let mut previous = None;
    for _ in 0..count {
        reader.node()?; // Structural row.
        reader.node()?; // Owning row.
        let raw = reader.raw(MAX_POLICY_SNAPSHOT_BYTES)?;
        if let Some(previous) = previous {
            reader.keys(previous, raw, MAX_POLICY_SNAPSHOT_BYTES)?;
        }
        previous = Some(raw);
        let key = copy_leaf(raw, reader)?;
        let state = read_state(reader)?;
        states.insert(key, state);
    }
    let indexed_attributes = reader.optional(|reader| read_names(reader, 3, 64))?;
    let indexed_state_meta = reader.optional(|reader| {
        let bytes = reader.raw(64)?;
        copy_leaf(bytes, reader)
    })?;
    let governance = reader.optional(governance::read_governance)?;
    Ok(CompletePolicyData {
        workflow_type,
        version,
        max_active_ms,
        retry,
        retention,
        states,
        indexed_attributes,
        indexed_state_meta,
        governance,
    })
}

fn read_state(
    reader: &mut Reader<'_, '_>,
) -> Result<CompleteStatePolicyData, CompletePolicyEncodingError> {
    let mode = reader.optional(|reader| match reader.byte()? {
        0 => Ok(StateMode::Parallel),
        1 => Ok(StateMode::Fifo),
        _ => Err(CompletePolicyEncodingError::UnsupportedTag),
    })?;
    let retry = reader.optional(read_retry)?;
    let retention = reader.optional(read_retention)?;
    let governance = reader.optional(governance::read_governance)?;
    Ok(CompleteStatePolicyData {
        mode,
        retry,
        retention,
        governance,
    })
}

fn read_version(reader: &mut Reader<'_, '_>) -> Result<PolicyVersion, CompletePolicyEncodingError> {
    match reader.byte()? {
        0 => {
            let bytes = reader.raw(MAX_POLICY_SNAPSHOT_BYTES)?;
            Ok(PolicyVersion::Bytes(copy_leaf(bytes, reader)?))
        }
        1 => Ok(PolicyVersion::Number(reader.u64()?)),
        _ => Err(CompletePolicyEncodingError::UnsupportedTag),
    }
}

fn read_retry(reader: &mut Reader<'_, '_>) -> Result<RetryOverride, CompletePolicyEncodingError> {
    reader.node()?;
    reader.node()?; // Owning structured override.
    let max_retries = reader.optional(Reader::u16)?;
    let backoff = reader.optional(read_backoff)?;
    let exhausted_to = reader.optional(|reader| {
        let bytes = reader.raw(MAX_POLICY_SNAPSHOT_BYTES)?;
        copy_leaf(bytes, reader)
    })?;
    Ok(RetryOverride {
        max_retries,
        backoff,
        exhausted_to,
    })
}

fn read_backoff(reader: &mut Reader<'_, '_>) -> Result<Backoff, CompletePolicyEncodingError> {
    reader.node()?;
    reader.node()?;
    let kind = match reader.byte()? {
        0 => BackoffKind::None,
        1 => BackoffKind::Fixed,
        2 => BackoffKind::Linear,
        3 => BackoffKind::Exponential,
        _ => return Err(CompletePolicyEncodingError::UnsupportedTag),
    };
    Ok(Backoff {
        kind,
        base_ms: reader.u64()?,
        max_ms: reader.u64()?,
        jitter_pct: reader.byte()?,
    })
}

fn read_retention(
    reader: &mut Reader<'_, '_>,
) -> Result<RetentionOverride, CompletePolicyEncodingError> {
    reader.node()?;
    reader.node()?;
    Ok(RetentionOverride {
        ttl_ms: reader.optional(Reader::u64)?,
        history_max_events: reader.optional(Reader::u32)?,
    })
}

pub(super) fn read_names(
    reader: &mut Reader<'_, '_>,
    maximum_count: usize,
    maximum_bytes: usize,
) -> Result<Vec<Vec<u8>>, CompletePolicyEncodingError> {
    let count = reader.count(maximum_count)?;
    reader.node()?; // Owning list.
    let work = u64::try_from(count)
        .ok()
        .and_then(|n| n.checked_mul(128))
        .ok_or(CompletePolicyEncodingError::Schema(
            PolicySchemaError::LengthOverflow,
        ))?;
    charge_schema(reader.charge, work, 0).map_err(CompletePolicyEncodingError::Schema)?;
    let mut names = Vec::new();
    names
        .try_reserve_exact(count)
        .map_err(|_| CompletePolicyEncodingError::AllocationFailure)?;
    for _ in 0..count {
        reader.node()?;
        let bytes = reader.raw(maximum_bytes)?;
        names.push(copy_leaf(bytes, reader)?);
    }
    Ok(names)
}

pub(in crate::policy::complete) fn copy_leaf(
    bytes: &[u8],
    reader: &mut Reader<'_, '_>,
) -> Result<Vec<u8>, CompletePolicyEncodingError> {
    let work = u64::try_from(bytes.len())
        .ok()
        .and_then(|n| n.checked_add(128))
        .ok_or(CompletePolicyEncodingError::Schema(
            PolicySchemaError::LengthOverflow,
        ))?;
    charge_schema(reader.charge, work, 0).map_err(CompletePolicyEncodingError::Schema)?;
    let mut copied = Vec::new();
    copied
        .try_reserve_exact(bytes.len())
        .map_err(|_| CompletePolicyEncodingError::AllocationFailure)?;
    copied.extend_from_slice(bytes);
    Ok(copied)
}
