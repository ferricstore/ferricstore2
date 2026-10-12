//! Original partial core-Patch intent and frame work over borrowed operands.

use crate::policy::BackoffKind;
use crate::policy::definition::{
    BackoffPatch, FieldPatch, PolicyVersion, RetryPatch, StateChange, StateMode, StatePolicyPatch,
};
use crate::retention::RetentionOverride;
use crate::workflow::admission::AdmissionLimits;

use super::super::super::admission::charge_schema;
use super::super::super::{
    CompleteCorePolicyPatchInput, CompletePolicyEncodingError, PolicySchemaCharge,
    PolicySchemaError, PolicySchemaLimits,
};
use super::super::{
    CompleteCorePolicyPatchCaller, CompleteCorePolicyPatchRequestInput, CompletePolicyRequestError,
};
use super::check_header;
use super::measure::IntentMeasure;

struct MeasuredCompleteCorePolicyPatchIntent<'i> {
    tenant: &'i [u8],
    command: &'i CompleteCorePolicyPatchInput,
    bytes: usize,
    nodes: u64,
}

pub(in crate::policy::complete::outer) fn prepare_core_patch_caller(
    input: CompleteCorePolicyPatchRequestInput,
    limits: PolicySchemaLimits,
    admission: AdmissionLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<CompleteCorePolicyPatchCaller, CompletePolicyRequestError> {
    if charge.profile() != limits {
        return Err(PolicySchemaError::ProfileMismatch.into());
    }
    {
        let mut measure = IntentMeasure::new(charge);
        measure.node()?;
        check_header(input.scope())?;
        let measured = measure_core_patch_intent(&input, measure)?;
        prepay_core_patch_intent(&measured, charge)?;
    }
    Ok(CompleteCorePolicyPatchCaller {
        input,
        schema_limits: limits,
        admission_limits: admission,
    })
}

fn measure_core_patch_intent<'i>(
    input: &'i CompleteCorePolicyPatchRequestInput,
    mut measure: IntentMeasure<'_>,
) -> Result<MeasuredCompleteCorePolicyPatchIntent<'i>, CompletePolicyRequestError> {
    let mut measured = MeasuredCompleteCorePolicyPatchIntent {
        tenant: input.scope().tenant().as_bytes(),
        command: input.command(),
        bytes: 0,
        nodes: 0,
    };
    measure.write(b"WF-INTENT\0COMPLETE-CORE-PATCH\0V2\0")?;
    measure.raw(measured.tenant, 8_192)?;
    measure.optional(measured.command.expected_generation.is_some())?;
    if let Some(expected) = measured.command.expected_generation {
        measure.u64(expected)?;
    }
    measure_core_patch(measured.command, &mut measure)?;
    (measured.bytes, measured.nodes) = measure.finish();
    Ok(measured)
}

fn prepay_core_patch_intent(
    measured: &MeasuredCompleteCorePolicyPatchIntent<'_>,
    charge: &mut PolicySchemaCharge,
) -> Result<(), CompletePolicyRequestError> {
    let bytes = u64::try_from(measured.bytes).map_err(|_| PolicySchemaError::LengthOverflow)?;
    let stream = measured
        .nodes
        .checked_mul(128)
        .ok_or(PolicySchemaError::LengthOverflow)?
        .checked_add(bytes)
        .ok_or(PolicySchemaError::LengthOverflow)?;
    charge_schema(charge, stream, 0)?;
    let hash = bytes
        .checked_add(128)
        .ok_or(PolicySchemaError::LengthOverflow)?;
    charge_schema(charge, hash, 0)?;
    Ok(())
}

fn measure_core_patch(
    command: &CompleteCorePolicyPatchInput,
    measure: &mut IntentMeasure<'_>,
) -> Result<(), CompletePolicyRequestError> {
    measure.node()?;
    measure.raw(&command.workflow_type, 262_144)?;
    match &command.patch.version {
        FieldPatch::Unchanged => measure.u8(0)?,
        FieldPatch::Set(version) => {
            measure.u8(1)?;
            measure.node()?;
            measure_core_patch_version(version, measure)?;
        }
        FieldPatch::Clear => measure.u8(2)?,
    }
    match &command.patch.max_active_ms {
        FieldPatch::Unchanged => measure.u8(0)?,
        FieldPatch::Set(maximum) => {
            measure.u8(1)?;
            measure.node()?;
            measure.u64(*maximum)?;
        }
        FieldPatch::Clear => measure.u8(2)?,
    }
    match &command.patch.retry {
        FieldPatch::Unchanged => measure.u8(0)?,
        FieldPatch::Set(retry) => {
            measure.u8(1)?;
            measure.node()?;
            measure_core_patch_retry(retry, measure)?;
        }
        FieldPatch::Clear => measure.u8(2)?,
    }
    match &command.patch.retention {
        FieldPatch::Unchanged => measure.u8(0)?,
        FieldPatch::Set(retention) => {
            measure.u8(1)?;
            measure.node()?;
            measure_core_patch_retention(retention, measure)?;
        }
        FieldPatch::Clear => measure.u8(2)?,
    }
    measure.count(command.patch.states.len(), usize::MAX)?;
    for (key, change) in &command.patch.states {
        measure.node()?;
        measure.raw(key, 262_144)?;
        match change {
            StateChange::Remove => measure.u8(0)?,
            StateChange::Patch(state) => {
                measure.u8(1)?;
                measure.node()?;
                measure_core_patch_state(state, measure)?;
            }
        }
    }
    Ok(())
}

fn measure_core_patch_state(
    state: &StatePolicyPatch,
    measure: &mut IntentMeasure<'_>,
) -> Result<(), CompletePolicyRequestError> {
    match &state.mode {
        FieldPatch::Unchanged => measure.u8(0)?,
        FieldPatch::Set(mode) => {
            measure.u8(1)?;
            measure.node()?;
            measure.u8(match mode {
                StateMode::Parallel => 0,
                StateMode::Fifo => 1,
            })?;
        }
        FieldPatch::Clear => measure.u8(2)?,
    }
    match &state.retry {
        FieldPatch::Unchanged => measure.u8(0)?,
        FieldPatch::Set(retry) => {
            measure.u8(1)?;
            measure.node()?;
            measure_core_patch_retry(retry, measure)?;
        }
        FieldPatch::Clear => measure.u8(2)?,
    }
    match &state.retention {
        FieldPatch::Unchanged => measure.u8(0)?,
        FieldPatch::Set(retention) => {
            measure.u8(1)?;
            measure.node()?;
            measure_core_patch_retention(retention, measure)?;
        }
        FieldPatch::Clear => measure.u8(2)?,
    }
    Ok(())
}

fn measure_core_patch_version(
    version: &PolicyVersion,
    measure: &mut IntentMeasure<'_>,
) -> Result<(), CompletePolicyRequestError> {
    match version {
        PolicyVersion::Bytes(bytes) => {
            measure.u8(0)?;
            measure.raw(bytes, 262_144)
        }
        PolicyVersion::Number(number) => {
            measure.u8(1)?;
            measure.u64(*number)
        }
    }
}

fn measure_core_patch_retry(
    retry: &RetryPatch,
    measure: &mut IntentMeasure<'_>,
) -> Result<(), CompletePolicyRequestError> {
    measure.node()?;
    measure.optional(retry.max_retries.is_some())?;
    if let Some(maximum) = retry.max_retries {
        measure.u16(maximum)?;
    }
    measure.optional(retry.backoff.is_some())?;
    if let Some(backoff) = &retry.backoff {
        measure_core_patch_backoff(backoff, measure)?;
    }
    measure.optional(retry.exhausted_to.is_some())?;
    if let Some(target) = &retry.exhausted_to {
        measure.raw(target, 262_144)?;
    }
    Ok(())
}

fn measure_core_patch_backoff(
    backoff: &BackoffPatch,
    measure: &mut IntentMeasure<'_>,
) -> Result<(), CompletePolicyRequestError> {
    measure.node()?;
    measure.optional(backoff.kind.is_some())?;
    if let Some(kind) = backoff.kind {
        measure.u8(match kind {
            BackoffKind::None => 0,
            BackoffKind::Fixed => 1,
            BackoffKind::Linear => 2,
            BackoffKind::Exponential => 3,
        })?;
    }
    measure.optional(backoff.base_ms.is_some())?;
    if let Some(base) = backoff.base_ms {
        measure.u64(base)?;
    }
    measure.optional(backoff.max_ms.is_some())?;
    if let Some(maximum) = backoff.max_ms {
        measure.u64(maximum)?;
    }
    measure.optional(backoff.jitter_pct.is_some())?;
    if let Some(jitter) = backoff.jitter_pct {
        measure.u8(jitter)?;
    }
    Ok(())
}

fn measure_core_patch_retention(
    retention: &RetentionOverride,
    measure: &mut IntentMeasure<'_>,
) -> Result<(), CompletePolicyRequestError> {
    measure.node()?;
    measure.optional(retention.ttl_ms.is_some())?;
    if let Some(ttl) = retention.ttl_ms {
        measure.u64(ttl)?;
    }
    measure.optional(retention.history_max_events.is_some())?;
    if let Some(maximum) = retention.history_max_events {
        measure.u32(maximum)?;
    }
    Ok(())
}

pub(in crate::policy::complete::outer) fn prepare_core_patch_frame_caller(
    input: CompleteCorePolicyPatchRequestInput,
    limits: PolicySchemaLimits,
    admission: AdmissionLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<(CompleteCorePolicyPatchCaller, usize, usize), CompletePolicyRequestError> {
    if charge.profile() != limits {
        return Err(PolicySchemaError::ProfileMismatch.into());
    }
    let (intent_bytes, frame_bytes) = {
        let mut measure = IntentMeasure::new(charge);
        measure.node()?;
        check_header(input.scope())?;
        let measured = measure_core_patch_intent(&input, measure)?;
        let (header_bytes, header_nodes) =
            measure_core_patch_frame_header(&input, measured.bytes, charge)?;
        let frame_bytes = header_bytes
            .checked_add(measured.bytes)
            .ok_or(PolicySchemaError::LengthOverflow)?;
        if frame_bytes > admission.decision_bytes() {
            return Err(CompletePolicyEncodingError::ByteLimit.into());
        }
        prepay_core_patch_intent(&measured, charge)?;
        prepay_core_patch_frame(&measured, header_bytes, header_nodes, frame_bytes, charge)?;
        (measured.bytes, frame_bytes)
    };
    Ok((
        CompleteCorePolicyPatchCaller {
            input,
            schema_limits: limits,
            admission_limits: admission,
        },
        intent_bytes,
        frame_bytes,
    ))
}

fn measure_core_patch_frame_header(
    input: &CompleteCorePolicyPatchRequestInput,
    intent_bytes: usize,
    charge: &mut PolicySchemaCharge,
) -> Result<(usize, u64), CompletePolicyRequestError> {
    let mut measure = IntentMeasure::new(charge);
    measure.node()?;
    measure.write(b"FWCP")?;
    measure.u16(1)?;
    measure.u8(5)?;
    measure.raw(input.scope().tenant().as_bytes(), 8_192)?;
    measure.raw(input.scope().group().as_bytes(), 256)?;
    measure.raw(input.scope().recovery().as_bytes(), 256)?;
    measure.raw(input.request().as_bytes(), 256)?;
    measure.u16(2)?;
    measure.write(&[0; 32])?;
    measure.u64(input.stamp().hlc().physical().get())?;
    measure.u16(input.stamp().hlc().logical())?;
    measure.u64(input.stamp().wall().get())?;
    measure.node()?;
    let intent_bytes = u32::try_from(intent_bytes).or(Err(PolicySchemaError::LengthOverflow))?;
    measure.u32(intent_bytes)?;
    Ok(measure.finish())
}

fn prepay_core_patch_frame(
    measured: &MeasuredCompleteCorePolicyPatchIntent<'_>,
    header_bytes: usize,
    header_nodes: u64,
    frame_bytes: usize,
    charge: &mut PolicySchemaCharge,
) -> Result<(), CompletePolicyRequestError> {
    let frame = u64::try_from(frame_bytes)
        .ok()
        .and_then(|n| n.checked_add(128))
        .ok_or(PolicySchemaError::LengthOverflow)?;
    charge_schema(charge, frame, 0)?;
    let header_bytes = u64::try_from(header_bytes).or(Err(PolicySchemaError::LengthOverflow))?;
    let header = header_nodes
        .checked_mul(128)
        .and_then(|n| n.checked_add(header_bytes))
        .ok_or(PolicySchemaError::LengthOverflow)?;
    charge_schema(charge, header, 0)?;
    let intent_bytes = u64::try_from(measured.bytes).or(Err(PolicySchemaError::LengthOverflow))?;
    let original = measured
        .nodes
        .checked_mul(128)
        .and_then(|n| n.checked_add(intent_bytes))
        .ok_or(PolicySchemaError::LengthOverflow)?;
    charge_schema(charge, original, 0)?;
    charge_schema(charge, frame, 0)?;
    Ok(())
}
