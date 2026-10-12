//! One immutable measurement followed by emission or streamed comparison.

use crate::policy::definition::{PolicyVersion, StateMode};
use crate::policy::installation::{MAX_ENCODED_POLICY_BYTES, MAX_POLICY_SNAPSHOT_BYTES};
use crate::policy::{Backoff, BackoffKind, RetryOverride};
use crate::retention::RetentionOverride;

use super::super::{CompletePolicyData, CompleteStatePolicyData};
use super::{
    CompletePolicyEncodingError, MeasuredPolicyV2, PolicySchemaCharge, PolicySchemaError,
    PolicySchemaLimits, Writer,
};

mod governance;

pub(super) fn measure_value<'v>(
    generation: u64,
    data: &'v CompletePolicyData,
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<MeasuredPolicyV2<'v>, CompletePolicyEncodingError> {
    let maximum = 18_usize.checked_add(MAX_POLICY_SNAPSHOT_BYTES).ok_or(
        CompletePolicyEncodingError::Schema(PolicySchemaError::LengthOverflow),
    )?;
    let mut writer = Writer::measure(maximum, limits, charge);
    writer.node()?;
    writer.write(b"FWPV")?;
    writer.u16(2)?;
    writer.u64(generation)?;
    writer.u32(0)?;
    write_snapshot(data, &mut writer)?;
    let envelope_len = writer.position();
    let snapshot_len = envelope_len
        .checked_sub(18)
        .ok_or(CompletePolicyEncodingError::Schema(
            PolicySchemaError::LengthOverflow,
        ))?;
    if snapshot_len > MAX_POLICY_SNAPSHOT_BYTES {
        return Err(CompletePolicyEncodingError::Schema(
            PolicySchemaError::SnapshotBytes,
        ));
    }
    if envelope_len > MAX_ENCODED_POLICY_BYTES {
        return Err(CompletePolicyEncodingError::Schema(
            PolicySchemaError::EnvelopeBytes,
        ));
    }
    writer.finish(envelope_len)?;
    Ok(MeasuredPolicyV2 {
        generation,
        data,
        snapshot_len,
        envelope_len,
    })
}

pub(super) fn emit_value(
    measured: &MeasuredPolicyV2<'_>,
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<Vec<u8>, CompletePolicyEncodingError> {
    let mut writer = Writer::output(
        measured.envelope_len,
        MAX_ENCODED_POLICY_BYTES,
        limits,
        charge,
    )?;
    writer.node()?;
    writer.write(b"FWPV")?;
    writer.u16(2)?;
    writer.u64(measured.generation)?;
    writer
        .u32(u32::try_from(measured.snapshot_len).map_err(|_| {
            CompletePolicyEncodingError::Schema(PolicySchemaError::LengthOverflow)
        })?)?;
    write_snapshot(measured.data, &mut writer)?;
    writer.finish(measured.envelope_len)?;
    writer.into_output()
}

pub(super) fn compare_value(
    measured: &MeasuredPolicyV2<'_>,
    expected: &[u8],
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<(), CompletePolicyEncodingError> {
    if expected.len() != measured.envelope_len {
        return Err(CompletePolicyEncodingError::EncodingMismatch);
    }
    let mut writer = Writer::compare(expected, measured.envelope_len, limits, charge);
    writer.node()?;
    writer.write(b"FWPV")?;
    writer.u16(2)?;
    writer.u64(measured.generation)?;
    writer
        .u32(u32::try_from(measured.snapshot_len).map_err(|_| {
            CompletePolicyEncodingError::Schema(PolicySchemaError::LengthOverflow)
        })?)?;
    write_snapshot(measured.data, &mut writer)?;
    writer.finish(measured.envelope_len)
}

fn write_snapshot(
    data: &CompletePolicyData,
    writer: &mut Writer<'_, '_>,
) -> Result<(), CompletePolicyEncodingError> {
    writer.node()?;
    writer.write(b"FWPS")?;
    writer.u16(2)?;
    writer.raw(&data.workflow_type, MAX_POLICY_SNAPSHOT_BYTES)?;
    writer.optional(data.version.as_ref(), write_version)?;
    writer.optional(data.max_active_ms.as_ref(), |value, writer| {
        writer.u64(*value)
    })?;
    writer.optional(data.retry.as_ref(), write_retry)?;
    writer.optional(data.retention.as_ref(), |value, writer| {
        write_retention(*value, writer)
    })?;
    writer.count(
        data.states.len(),
        writer.limits.policy_limits().max_states(),
    )?;
    for (key, state) in &data.states {
        writer.node()?;
        writer.raw(key, MAX_POLICY_SNAPSHOT_BYTES)?;
        write_state(state, writer)?;
    }
    writer.optional(data.indexed_attributes.as_ref(), |names, writer| {
        write_names(names, 3, 64, writer)
    })?;
    writer.optional(data.indexed_state_meta.as_ref(), |name, writer| {
        writer.raw(name, 64)
    })?;
    writer.optional(data.governance.as_ref(), governance::write_governance)
}

fn write_state(
    state: &CompleteStatePolicyData,
    writer: &mut Writer<'_, '_>,
) -> Result<(), CompletePolicyEncodingError> {
    writer.optional(state.mode.as_ref(), |mode, writer| {
        writer.byte(match mode {
            StateMode::Parallel => 0,
            StateMode::Fifo => 1,
        })
    })?;
    writer.optional(state.retry.as_ref(), write_retry)?;
    writer.optional(state.retention.as_ref(), |value, writer| {
        write_retention(*value, writer)
    })?;
    writer.optional(state.governance.as_ref(), governance::write_governance)
}

fn write_version(
    version: &PolicyVersion,
    writer: &mut Writer<'_, '_>,
) -> Result<(), CompletePolicyEncodingError> {
    match version {
        PolicyVersion::Bytes(bytes) => {
            writer.byte(0)?;
            writer.raw(bytes, MAX_POLICY_SNAPSHOT_BYTES)
        }
        PolicyVersion::Number(value) => {
            writer.byte(1)?;
            writer.u64(*value)
        }
    }
}

fn write_retry(
    retry: &RetryOverride,
    writer: &mut Writer<'_, '_>,
) -> Result<(), CompletePolicyEncodingError> {
    writer.node()?;
    writer.optional(retry.max_retries.as_ref(), |value, writer| {
        writer.u16(*value)
    })?;
    writer.optional(retry.backoff.as_ref(), write_backoff)?;
    writer.optional(retry.exhausted_to.as_ref(), |value, writer| {
        writer.raw(value, MAX_POLICY_SNAPSHOT_BYTES)
    })
}

fn write_backoff(
    backoff: &Backoff,
    writer: &mut Writer<'_, '_>,
) -> Result<(), CompletePolicyEncodingError> {
    writer.node()?;
    writer.byte(match backoff.kind {
        BackoffKind::None => 0,
        BackoffKind::Fixed => 1,
        BackoffKind::Linear => 2,
        BackoffKind::Exponential => 3,
    })?;
    writer.u64(backoff.base_ms)?;
    writer.u64(backoff.max_ms)?;
    writer.byte(backoff.jitter_pct)
}

fn write_retention(
    retention: RetentionOverride,
    writer: &mut Writer<'_, '_>,
) -> Result<(), CompletePolicyEncodingError> {
    writer.node()?;
    writer.optional(retention.ttl_ms.as_ref(), |value, writer| {
        writer.u64(*value)
    })?;
    writer.optional(retention.history_max_events.as_ref(), |value, writer| {
        writer.u32(*value)
    })
}

pub(super) fn write_names(
    names: &[Vec<u8>],
    maximum_count: usize,
    maximum_bytes: usize,
    writer: &mut Writer<'_, '_>,
) -> Result<(), CompletePolicyEncodingError> {
    writer.count(names.len(), maximum_count)?;
    for name in names {
        writer.node()?;
        writer.raw(name, maximum_bytes)?;
    }
    Ok(())
}
