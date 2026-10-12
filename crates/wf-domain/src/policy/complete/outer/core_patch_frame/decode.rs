//! Paid consuming reconstruction of the exact original partial-command owners.

use std::collections::BTreeMap;

use crate::policy::BackoffKind;
use crate::policy::definition::{
    BackoffPatch, FieldPatch, PolicyPatch, PolicyVersion, RetryPatch, StateChange, StateMode,
    StatePolicyPatch,
};
use crate::retention::RetentionOverride;
use crate::workflow::admission::AdmissionLimits;

use super::super::super::encoding::{decode::copy_leaf, reader::Reader};
use super::super::super::{
    CompleteCorePolicyPatchInput, CompletePolicyEncodingError, PolicySchemaCharge,
    PolicySchemaError, PolicySchemaLimits,
};
use super::super::CompleteCorePolicyPatchRequestInput;
use super::{CompleteCorePolicyPatchFrameHeader, CompletePolicyRequestError};

pub(super) fn decode_input(
    header: CompleteCorePolicyPatchFrameHeader<'_>,
    limits: PolicySchemaLimits,
    admission: AdmissionLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<CompleteCorePolicyPatchRequestInput, CompletePolicyRequestError> {
    if charge.profile() != limits {
        return Err(PolicySchemaError::ProfileMismatch.into());
    }
    if header.encoded.len() > admission.decision_bytes() {
        return Err(CompletePolicyEncodingError::ByteLimit.into());
    }
    let CompleteCorePolicyPatchFrameHeader {
        original_intent,
        scope,
        request,
        stamp,
        ..
    } = header;
    let command = {
        let mut reader = Reader::new(original_intent, limits, charge);
        let command = read_command(&mut reader)?;
        reader.finish()?;
        command
    };
    Ok(CompleteCorePolicyPatchRequestInput::new(
        scope, request, stamp, command,
    ))
}

fn read_command(
    reader: &mut Reader<'_, '_>,
) -> Result<CompleteCorePolicyPatchInput, CompletePolicyEncodingError> {
    reader.node()?; // Structural original intent.
    reader.node()?; // Owning original command.
    if reader.take(33)? != b"WF-INTENT\0COMPLETE-CORE-PATCH\0V2\0" {
        return Err(CompletePolicyEncodingError::Malformed);
    }
    if reader.raw(8_192)?.is_empty() {
        return Err(CompletePolicyEncodingError::Malformed);
    }
    let expected_generation = reader.optional(Reader::u64)?;
    reader.node()?; // Structural partial policy.
    let workflow_type = read_owned(reader)?;
    let patch = PolicyPatch {
        version: read_field_patch(reader, read_version)?,
        max_active_ms: read_field_patch(reader, Reader::u64)?,
        retry: read_field_patch(reader, read_retry)?,
        retention: read_field_patch(reader, read_retention)?,
        states: read_state_changes(reader)?,
    };
    Ok(CompleteCorePolicyPatchInput {
        workflow_type,
        expected_generation,
        patch,
    })
}

fn read_field_patch<'b, 'c, T>(
    reader: &mut Reader<'b, 'c>,
    read: impl FnOnce(&mut Reader<'b, 'c>) -> Result<T, CompletePolicyEncodingError>,
) -> Result<FieldPatch<T>, CompletePolicyEncodingError> {
    match reader.byte()? {
        0 => Ok(FieldPatch::Unchanged),
        1 => {
            reader.node()?;
            read(reader).map(FieldPatch::Set)
        }
        2 => Ok(FieldPatch::Clear),
        _ => Err(CompletePolicyEncodingError::UnsupportedTag),
    }
}

fn read_owned(reader: &mut Reader<'_, '_>) -> Result<Vec<u8>, CompletePolicyEncodingError> {
    let bytes = reader.raw(262_144)?;
    copy_leaf(bytes, reader)
}

fn read_version(reader: &mut Reader<'_, '_>) -> Result<PolicyVersion, CompletePolicyEncodingError> {
    match reader.byte()? {
        0 => read_owned(reader).map(PolicyVersion::Bytes),
        1 => reader.u64().map(PolicyVersion::Number),
        _ => Err(CompletePolicyEncodingError::UnsupportedTag),
    }
}

fn read_retry(reader: &mut Reader<'_, '_>) -> Result<RetryPatch, CompletePolicyEncodingError> {
    reader.node()?; // Structural retry group.
    reader.node()?; // Owning retry patch.
    Ok(RetryPatch {
        max_retries: reader.optional(Reader::u16)?,
        backoff: reader.optional(read_backoff)?,
        exhausted_to: reader.optional(read_owned)?,
    })
}

fn read_backoff(reader: &mut Reader<'_, '_>) -> Result<BackoffPatch, CompletePolicyEncodingError> {
    reader.node()?; // Structural backoff group.
    reader.node()?; // Owning partial backoff.
    Ok(BackoffPatch {
        kind: reader.optional(read_kind)?,
        base_ms: reader.optional(Reader::u64)?,
        max_ms: reader.optional(Reader::u64)?,
        jitter_pct: reader.optional(Reader::byte)?,
    })
}

fn read_kind(reader: &mut Reader<'_, '_>) -> Result<BackoffKind, CompletePolicyEncodingError> {
    match reader.byte()? {
        0 => Ok(BackoffKind::None),
        1 => Ok(BackoffKind::Fixed),
        2 => Ok(BackoffKind::Linear),
        3 => Ok(BackoffKind::Exponential),
        _ => Err(CompletePolicyEncodingError::UnsupportedTag),
    }
}

fn read_retention(
    reader: &mut Reader<'_, '_>,
) -> Result<RetentionOverride, CompletePolicyEncodingError> {
    reader.node()?; // Structural retention group.
    reader.node()?; // Owning retention override.
    Ok(RetentionOverride {
        ttl_ms: reader.optional(Reader::u64)?,
        history_max_events: reader.optional(Reader::u32)?,
    })
}

fn read_state_changes(
    reader: &mut Reader<'_, '_>,
) -> Result<BTreeMap<Vec<u8>, StateChange>, CompletePolicyEncodingError> {
    let count = reader.count(usize::MAX)?;
    reader.node()?; // Owning original state-change map.
    let mut states = BTreeMap::new();
    let mut previous = None;
    for _ in 0..count {
        reader.node()?; // Structural row.
        reader.node()?; // Owning row.
        let raw = reader.raw(262_144)?;
        if let Some(previous) = previous {
            reader.keys(previous, raw, 262_144)?;
        }
        previous = Some(raw);
        let key = copy_leaf(raw, reader)?;
        let change = match reader.byte()? {
            0 => StateChange::Remove,
            1 => StateChange::Patch(read_state_patch(reader)?),
            _ => return Err(CompletePolicyEncodingError::UnsupportedTag),
        };
        states.insert(key, change);
    }
    Ok(states)
}

fn read_state_patch(
    reader: &mut Reader<'_, '_>,
) -> Result<StatePolicyPatch, CompletePolicyEncodingError> {
    reader.node()?; // Structural state patch.
    reader.node()?; // Owning state patch.
    Ok(StatePolicyPatch {
        mode: read_field_patch(reader, read_mode)?,
        retry: read_field_patch(reader, read_retry)?,
        retention: read_field_patch(reader, read_retention)?,
    })
}

fn read_mode(reader: &mut Reader<'_, '_>) -> Result<StateMode, CompletePolicyEncodingError> {
    match reader.byte()? {
        0 => Ok(StateMode::Parallel),
        1 => Ok(StateMode::Fifo),
        _ => Err(CompletePolicyEncodingError::UnsupportedTag),
    }
}
