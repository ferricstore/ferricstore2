//! Borrowed patch and normalized snapshot emission with explicit stable widths.

use wf_domain::policy::definition::{
    BackoffPatch, FieldPatch, PolicySnapshot, PolicyVersion, RetryPatch, StateChange, StateMode,
    StatePolicyPatch,
};
use wf_domain::policy::installation::{
    InstallCommand, InstallMode, MAX_POLICY_GENERATION, MAX_POLICY_SNAPSHOT_BYTES,
};
use wf_domain::policy::{Backoff, BackoffKind, RetryOverride};
use wf_domain::retention::RetentionOverride;

use super::super::{CodecError, CodecLimits, writer::Writer};

pub(super) fn envelope(
    generation: u64,
    value: &PolicySnapshot,
    writer: &mut Writer<'_>,
    limits: CodecLimits,
) -> Result<(), CodecError> {
    if generation == 0 || generation > MAX_POLICY_GENERATION {
        return Err(CodecError::Malformed);
    }
    let mut snapshot_size = Writer::measure(MAX_POLICY_SNAPSHOT_BYTES);
    snapshot(value, &mut snapshot_size, limits)?;
    writer.write(b"FWPV")?;
    writer.u16(1)?;
    writer.u64(generation)?;
    writer.u32(u32::try_from(snapshot_size.len()).map_err(|_| CodecError::LengthOverflow)?)?;
    snapshot(value, writer, limits)
}

fn raw(value: &[u8], writer: &mut Writer<'_>, total: &mut usize) -> Result<(), CodecError> {
    *total = total
        .checked_add(value.len())
        .ok_or(CodecError::LengthOverflow)?;
    if *total > MAX_POLICY_SNAPSHOT_BYTES {
        return Err(CodecError::ByteLimit);
    }
    writer.raw(value, MAX_POLICY_SNAPSHOT_BYTES)
}

fn optional_raw(
    value: Option<&[u8]>,
    writer: &mut Writer<'_>,
    total: &mut usize,
) -> Result<(), CodecError> {
    writer.optional(value, |writer, value| raw(value, writer, total))
}

fn kind(value: BackoffKind, writer: &mut Writer<'_>) -> Result<(), CodecError> {
    writer.byte(match value {
        BackoffKind::None => 0,
        BackoffKind::Fixed => 1,
        BackoffKind::Linear => 2,
        BackoffKind::Exponential => 3,
    })
}

fn mode(value: StateMode, writer: &mut Writer<'_>) -> Result<(), CodecError> {
    writer.byte(match value {
        StateMode::Parallel => 0,
        StateMode::Fifo => 1,
    })
}

fn version(
    value: &PolicyVersion,
    writer: &mut Writer<'_>,
    total: &mut usize,
) -> Result<(), CodecError> {
    match value {
        PolicyVersion::Bytes(bytes) => {
            writer.byte(0)?;
            raw(bytes, writer, total)
        }
        PolicyVersion::Number(number) => {
            writer.byte(1)?;
            writer.u64(*number)
        }
    }
}

fn backoff(value: &Backoff, writer: &mut Writer<'_>) -> Result<(), CodecError> {
    kind(value.kind, writer)?;
    writer.u64(value.base_ms)?;
    writer.u64(value.max_ms)?;
    writer.byte(value.jitter_pct)
}

fn retry(
    value: &RetryOverride,
    writer: &mut Writer<'_>,
    total: &mut usize,
) -> Result<(), CodecError> {
    writer.optional(value.max_retries.as_ref(), |writer, value| {
        writer.u16(*value)
    })?;
    writer.optional(value.backoff.as_ref(), |writer, value| {
        backoff(value, writer)
    })?;
    optional_raw(value.exhausted_to.as_deref(), writer, total)
}

fn retention(value: RetentionOverride, writer: &mut Writer<'_>) -> Result<(), CodecError> {
    writer.optional(value.ttl_ms.as_ref(), |writer, value| writer.u64(*value))?;
    writer.optional(value.history_max_events.as_ref(), |writer, value| {
        writer.u32(*value)
    })
}

fn count(count: usize, writer: &mut Writer<'_>, limits: CodecLimits) -> Result<(), CodecError> {
    if count > limits.policy_limits().max_states() {
        return Err(CodecError::ItemLimit);
    }
    writer.u32(u32::try_from(count).map_err(|_| CodecError::LengthOverflow)?)
}

fn snapshot(
    value: &PolicySnapshot,
    writer: &mut Writer<'_>,
    limits: CodecLimits,
) -> Result<(), CodecError> {
    let mut total = 0;
    writer.write(b"FWPS")?;
    writer.u16(1)?;
    raw(value.workflow_type(), writer, &mut total)?;
    writer.optional(value.version(), |writer, value| {
        version(value, writer, &mut total)
    })?;
    writer.optional(value.max_active_ms().as_ref(), |writer, value| {
        writer.u64(*value)
    })?;
    retry(value.type_retry(), writer, &mut total)?;
    retention(value.type_retention(), writer)?;
    count(value.states().len(), writer, limits)?;
    for (name, state) in value.states() {
        raw(name, writer, &mut total)?;
        writer.optional(state.mode.as_ref(), |writer, value| mode(*value, writer))?;
        retry(&state.retry, writer, &mut total)?;
        retention(state.retention, writer)?;
    }
    Ok(())
}

fn patch<T>(
    value: &FieldPatch<T>,
    writer: &mut Writer<'_>,
    write: impl FnOnce(&T, &mut Writer<'_>) -> Result<(), CodecError>,
) -> Result<(), CodecError> {
    match value {
        FieldPatch::Unchanged => writer.byte(0),
        FieldPatch::Set(value) => {
            writer.byte(1)?;
            write(value, writer)
        }
        FieldPatch::Clear => writer.byte(2),
    }
}

fn backoff_patch(value: &BackoffPatch, writer: &mut Writer<'_>) -> Result<(), CodecError> {
    writer.optional(value.kind.as_ref(), |writer, value| kind(*value, writer))?;
    writer.optional(value.base_ms.as_ref(), |writer, value| writer.u64(*value))?;
    writer.optional(value.max_ms.as_ref(), |writer, value| writer.u64(*value))?;
    writer.optional(value.jitter_pct.as_ref(), |writer, value| {
        writer.byte(*value)
    })
}

fn retry_patch(
    value: &RetryPatch,
    writer: &mut Writer<'_>,
    total: &mut usize,
) -> Result<(), CodecError> {
    writer.optional(value.max_retries.as_ref(), |writer, value| {
        writer.u16(*value)
    })?;
    writer.optional(value.backoff.as_ref(), |writer, value| {
        backoff_patch(value, writer)
    })?;
    optional_raw(value.exhausted_to.as_deref(), writer, total)
}

fn state_patch(
    value: &StatePolicyPatch,
    writer: &mut Writer<'_>,
    total: &mut usize,
) -> Result<(), CodecError> {
    patch(&value.mode, writer, |value, writer| mode(*value, writer))?;
    patch(&value.retry, writer, |value, writer| {
        retry_patch(value, writer, total)
    })?;
    patch(&value.retention, writer, |value, writer| {
        retention(*value, writer)
    })
}

pub(super) fn command(
    value: &InstallCommand,
    writer: &mut Writer<'_>,
    limits: CodecLimits,
) -> Result<(), CodecError> {
    let mut total = 0;
    raw(&value.workflow_type, writer, &mut total)?;
    writer.optional(value.expected_generation.as_ref(), |writer, value| {
        writer.u64(*value)
    })?;
    writer.byte(match value.mode {
        InstallMode::Patch => 0,
        InstallMode::Replace => 1,
    })?;
    patch(&value.patch.version, writer, |value, writer| {
        version(value, writer, &mut total)
    })?;
    patch(&value.patch.max_active_ms, writer, |value, writer| {
        writer.u64(*value)
    })?;
    patch(&value.patch.retry, writer, |value, writer| {
        retry_patch(value, writer, &mut total)
    })?;
    patch(&value.patch.retention, writer, |value, writer| {
        retention(*value, writer)
    })?;
    count(value.patch.states.len(), writer, limits)?;
    for (name, change) in &value.patch.states {
        raw(name, writer, &mut total)?;
        match change {
            StateChange::Remove => writer.byte(0)?,
            StateChange::Patch(value) => {
                writer.byte(1)?;
                state_patch(value, writer, &mut total)?;
            }
        }
    }
    Ok(())
}
