//! Materializes exact normalized values only after the complete borrowed scan.

use std::collections::BTreeMap;

use wf_domain::policy::definition::{
    BackoffPatch, FieldPatch, PolicyPatch, PolicyVersion, RetryPatch, StateChange, StateMode,
    StatePolicy, StatePolicyPatch,
};
use wf_domain::policy::installation::{
    InstallCommand, InstallMode, MAX_ENCODED_POLICY_BYTES, MAX_POLICY_SNAPSHOT_BYTES, PolicyValue,
};
use wf_domain::policy::restoration::{PolicySnapshotData, restore_candidate};
use wf_domain::policy::{Backoff, BackoffKind, RetryOverride};
use wf_domain::retention::RetentionOverride;

use super::super::{
    CodecError,
    reader::{self, Reader},
};

fn raw(reader: &mut Reader<'_>) -> Result<Vec<u8>, CodecError> {
    reader::copy(reader.raw(MAX_POLICY_SNAPSHOT_BYTES)?)
}
fn kind(reader: &mut Reader<'_>) -> Result<BackoffKind, CodecError> {
    match reader.byte()? {
        0 => Ok(BackoffKind::None),
        1 => Ok(BackoffKind::Fixed),
        2 => Ok(BackoffKind::Linear),
        3 => Ok(BackoffKind::Exponential),
        _ => Err(CodecError::UnsupportedTag),
    }
}
fn mode(reader: &mut Reader<'_>) -> Result<StateMode, CodecError> {
    match reader.byte()? {
        0 => Ok(StateMode::Parallel),
        1 => Ok(StateMode::Fifo),
        _ => Err(CodecError::UnsupportedTag),
    }
}
fn version(reader: &mut Reader<'_>) -> Result<PolicyVersion, CodecError> {
    match reader.byte()? {
        0 => raw(reader).map(PolicyVersion::Bytes),
        1 => reader.u64().map(PolicyVersion::Number),
        _ => Err(CodecError::UnsupportedTag),
    }
}
fn backoff(reader: &mut Reader<'_>) -> Result<Backoff, CodecError> {
    Ok(Backoff {
        kind: kind(reader)?,
        base_ms: reader.u64()?,
        max_ms: reader.u64()?,
        jitter_pct: reader.byte()?,
    })
}
fn retry(reader: &mut Reader<'_>) -> Result<RetryOverride, CodecError> {
    Ok(RetryOverride {
        max_retries: reader.optional(Reader::u16)?,
        backoff: reader.optional(backoff)?,
        exhausted_to: reader.optional(raw)?,
    })
}
fn retention(reader: &mut Reader<'_>) -> Result<RetentionOverride, CodecError> {
    Ok(RetentionOverride {
        ttl_ms: reader.optional(Reader::u64)?,
        history_max_events: reader.optional(Reader::u32)?,
    })
}
fn patch<'a, T>(
    reader: &mut Reader<'a>,
    read: impl FnOnce(&mut Reader<'a>) -> Result<T, CodecError>,
) -> Result<FieldPatch<T>, CodecError> {
    match reader.byte()? {
        0 => Ok(FieldPatch::Unchanged),
        1 => read(reader).map(FieldPatch::Set),
        2 => Ok(FieldPatch::Clear),
        _ => Err(CodecError::UnsupportedTag),
    }
}
fn backoff_patch(reader: &mut Reader<'_>) -> Result<BackoffPatch, CodecError> {
    Ok(BackoffPatch {
        kind: reader.optional(kind)?,
        base_ms: reader.optional(Reader::u64)?,
        max_ms: reader.optional(Reader::u64)?,
        jitter_pct: reader.optional(Reader::byte)?,
    })
}
fn retry_patch(reader: &mut Reader<'_>) -> Result<RetryPatch, CodecError> {
    Ok(RetryPatch {
        max_retries: reader.optional(Reader::u16)?,
        backoff: reader.optional(backoff_patch)?,
        exhausted_to: reader.optional(raw)?,
    })
}
fn state_patch(reader: &mut Reader<'_>) -> Result<StatePolicyPatch, CodecError> {
    Ok(StatePolicyPatch {
        mode: patch(reader, mode)?,
        retry: patch(reader, retry_patch)?,
        retention: patch(reader, retention)?,
    })
}
fn count(reader: &mut Reader<'_>) -> Result<usize, CodecError> {
    let count = usize::try_from(reader.u32()?).map_err(|_| CodecError::LengthOverflow)?;
    if count > reader.limits.policy_limits().max_states() {
        Err(CodecError::ItemLimit)
    } else {
        Ok(count)
    }
}

pub(super) fn command(reader: &mut Reader<'_>) -> Result<InstallCommand, CodecError> {
    let workflow_type = raw(reader)?;
    let expected_generation = reader.optional(Reader::u64)?;
    let mode = match reader.byte()? {
        0 => InstallMode::Patch,
        1 => InstallMode::Replace,
        _ => return Err(CodecError::UnsupportedTag),
    };
    let version = patch(reader, version)?;
    let max_active_ms = patch(reader, Reader::u64)?;
    let retry = patch(reader, retry_patch)?;
    let retention = patch(reader, retention)?;
    let count = count(reader)?;
    let mut states = BTreeMap::new();
    for _ in 0..count {
        let name = raw(reader)?;
        let change = match reader.byte()? {
            0 => StateChange::Remove,
            1 => StateChange::Patch(state_patch(reader)?),
            _ => return Err(CodecError::UnsupportedTag),
        };
        if states.insert(name, change).is_some() {
            return Err(CodecError::Malformed);
        }
    }
    Ok(InstallCommand {
        workflow_type,
        expected_generation,
        mode,
        patch: PolicyPatch {
            version,
            max_active_ms,
            retry,
            retention,
            states,
        },
    })
}

pub(super) fn value(reader: &mut Reader<'_>) -> Result<PolicyValue, CodecError> {
    let bytes = reader.raw(MAX_ENCODED_POLICY_BYTES)?;
    let limits = reader.limits;
    let mut envelope = Reader::new(bytes, limits);
    envelope.take(6)?;
    let generation = envelope.u64()?;
    let snapshot_bytes = envelope.raw(MAX_POLICY_SNAPSHOT_BYTES)?;
    envelope.finish()?;
    let mut snapshot = Reader::new(snapshot_bytes, limits);
    snapshot.take(6)?;
    let workflow_type = raw(&mut snapshot)?;
    let version = snapshot.optional(version)?;
    let max_active_ms = snapshot.optional(Reader::u64)?;
    let retry_value = retry(&mut snapshot)?;
    let retention_value = retention(&mut snapshot)?;
    let count = count(&mut snapshot)?;
    let mut states = BTreeMap::new();
    for _ in 0..count {
        let name = raw(&mut snapshot)?;
        let state = StatePolicy {
            mode: snapshot.optional(mode)?,
            retry: retry(&mut snapshot)?,
            retention: retention(&mut snapshot)?,
        };
        if states.insert(name, state).is_some() {
            return Err(CodecError::Malformed);
        }
    }
    snapshot.finish()?;
    let candidate = restore_candidate(
        generation,
        PolicySnapshotData {
            workflow_type,
            version,
            max_active_ms,
            retry: retry_value,
            retention: retention_value,
            states,
        },
        limits.policy_limits(),
    )
    .map_err(|_| CodecError::Malformed)?;
    candidate
        .bind_encoded_snapshot(
            18..bytes.len(),
            reader::copy(bytes)?,
            limits.policy_limits(),
        )
        .map_err(|_| CodecError::Malformed)
}
