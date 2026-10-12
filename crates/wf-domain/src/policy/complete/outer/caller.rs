//! One paid caller walk and closed prepayments before owner movement.

use crate::policy::definition::{PolicyVersion, StateMode};
use crate::policy::{Backoff, BackoffKind, RetryOverride};
use crate::retention::RetentionOverride;
use crate::workflow::admission::AdmissionLimits;
use crate::workflow::request::RequestScope;

use super::super::admission::charge_schema;
use super::super::{
    CompletePolicyData, CompletePolicyEncodingError, CompleteStatePolicyData, PolicySchemaCharge,
    PolicySchemaError, PolicySchemaLimits,
};
use super::{CompletePolicyReplaceCaller, CompletePolicyReplaceInput, CompletePolicyRequestError};
use measure::{IntentMeasure, MeasuredCompletePolicyIntent};

mod core_patch;
mod governance;
mod measure;

pub(super) use core_patch::{prepare_core_patch_caller, prepare_core_patch_frame_caller};

pub(super) fn prepare_caller(
    input: CompletePolicyReplaceInput,
    limits: PolicySchemaLimits,
    admission: AdmissionLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<CompletePolicyReplaceCaller, CompletePolicyRequestError> {
    if charge.profile() != limits {
        return Err(PolicySchemaError::ProfileMismatch.into());
    }
    {
        let mut measure = IntentMeasure::new(charge);
        measure.node()?;
        check_header(input.scope())?;
        let measured = measure_intent(&input, limits, measure)?;
        prepay_intent(&measured, charge)?;
    }
    Ok(CompletePolicyReplaceCaller {
        input,
        schema_limits: limits,
        admission_limits: admission,
    })
}

fn check_header(scope: &RequestScope) -> Result<(), CompletePolicyRequestError> {
    // The immutable typed identities and stamp already enforce their individual invariants.
    let length = 18_usize
        .checked_add(scope.tenant().as_bytes().len())
        .and_then(|n| n.checked_add(scope.group().as_bytes().len()))
        .and_then(|n| n.checked_add(scope.recovery().as_bytes().len()))
        .ok_or(PolicySchemaError::LengthOverflow)?;
    if length > 8_192 {
        return Err(CompletePolicyRequestError::ScopeLimit);
    }
    Ok(())
}

fn measure_intent<'i>(
    input: &'i CompletePolicyReplaceInput,
    limits: PolicySchemaLimits,
    mut measure: IntentMeasure<'_>,
) -> Result<MeasuredCompletePolicyIntent<'i>, CompletePolicyRequestError> {
    let mut measured = MeasuredCompletePolicyIntent {
        tenant: input.scope().tenant().as_bytes(),
        data: input.data(),
        expected_generation: input.expected_generation(),
        bytes: 0,
        nodes: 0,
    };
    measure.write(b"WF-INTENT\0COMPLETE-REPLACE\0V2\0")?;
    measure.raw(measured.tenant, 8_192)?;
    measure.optional(measured.expected_generation.is_some())?;
    if let Some(expected) = measured.expected_generation {
        measure.u64(expected)?;
    }
    measure_data(measured.data, limits, &mut measure)?;
    (measured.bytes, measured.nodes) = measure.finish();
    Ok(measured)
}

fn prepay_intent(
    measured: &MeasuredCompletePolicyIntent<'_>,
    charge: &mut PolicySchemaCharge,
) -> Result<(), CompletePolicyRequestError> {
    let bytes = u64::try_from(measured.bytes).map_err(|_| PolicySchemaError::LengthOverflow)?;
    let stream = measured
        .nodes
        .checked_mul(128)
        .and_then(|n| n.checked_add(bytes))
        .ok_or(PolicySchemaError::LengthOverflow)?;
    charge_schema(charge, stream, 0)?;
    let hash = bytes
        .checked_add(128)
        .ok_or(PolicySchemaError::LengthOverflow)?;
    charge_schema(charge, hash, 0)?;
    Ok(())
}

fn measure_data(
    data: &CompletePolicyData,
    limits: PolicySchemaLimits,
    measure: &mut IntentMeasure<'_>,
) -> Result<(), CompletePolicyRequestError> {
    measure.node()?;
    measure.raw(&data.workflow_type, 262_144)?;
    measure.optional(data.version.is_some())?;
    if let Some(version) = &data.version {
        measure_version(version, measure)?;
    }
    measure.optional(data.max_active_ms.is_some())?;
    if let Some(maximum) = data.max_active_ms {
        measure.u64(maximum)?;
    }
    measure.optional(data.retry.is_some())?;
    if let Some(retry) = &data.retry {
        measure_retry(retry, measure)?;
    }
    measure.optional(data.retention.is_some())?;
    if let Some(retention) = data.retention {
        measure_retention(retention, measure)?;
    }
    measure.count(data.states.len(), limits.policy_limits().max_states())?;
    for (name, state) in &data.states {
        measure.node()?;
        measure.raw(name, 262_144)?;
        measure_state(state, measure)?;
    }
    measure.optional(data.indexed_attributes.is_some())?;
    if let Some(names) = &data.indexed_attributes {
        measure_names(names, 3, 64, measure)?;
    }
    measure.optional(data.indexed_state_meta.is_some())?;
    if let Some(name) = &data.indexed_state_meta {
        measure.raw(name, 64)?;
    }
    measure.optional(data.governance.is_some())?;
    if let Some(governance) = &data.governance {
        governance::measure_governance(governance, measure)?;
    }
    Ok(())
}

fn measure_state(
    state: &CompleteStatePolicyData,
    measure: &mut IntentMeasure<'_>,
) -> Result<(), CompletePolicyRequestError> {
    measure.optional(state.mode.is_some())?;
    if let Some(mode) = state.mode {
        measure.u8(match mode {
            StateMode::Parallel => 0,
            StateMode::Fifo => 1,
        })?;
    }
    measure.optional(state.retry.is_some())?;
    if let Some(retry) = &state.retry {
        measure_retry(retry, measure)?;
    }
    measure.optional(state.retention.is_some())?;
    if let Some(retention) = state.retention {
        measure_retention(retention, measure)?;
    }
    measure.optional(state.governance.is_some())?;
    if let Some(governance) = &state.governance {
        governance::measure_governance(governance, measure)?;
    }
    Ok(())
}

fn measure_version(
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

fn measure_retry(
    retry: &RetryOverride,
    measure: &mut IntentMeasure<'_>,
) -> Result<(), CompletePolicyRequestError> {
    measure.node()?;
    measure.optional(retry.max_retries.is_some())?;
    if let Some(maximum) = retry.max_retries {
        measure.u16(maximum)?;
    }
    measure.optional(retry.backoff.is_some())?;
    if let Some(backoff) = &retry.backoff {
        measure_backoff(backoff, measure)?;
    }
    measure.optional(retry.exhausted_to.is_some())?;
    if let Some(target) = &retry.exhausted_to {
        measure.raw(target, 262_144)?;
    }
    Ok(())
}

fn measure_backoff(
    backoff: &Backoff,
    measure: &mut IntentMeasure<'_>,
) -> Result<(), CompletePolicyRequestError> {
    measure.node()?;
    measure.u8(match backoff.kind {
        BackoffKind::None => 0,
        BackoffKind::Fixed => 1,
        BackoffKind::Linear => 2,
        BackoffKind::Exponential => 3,
    })?;
    measure.u64(backoff.base_ms)?;
    measure.u64(backoff.max_ms)?;
    measure.u8(backoff.jitter_pct)
}

fn measure_retention(
    retention: RetentionOverride,
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

fn measure_names(
    names: &[Vec<u8>],
    maximum_count: usize,
    maximum_bytes: usize,
    measure: &mut IntentMeasure<'_>,
) -> Result<(), CompletePolicyRequestError> {
    measure.count(names.len(), maximum_count)?;
    for name in names {
        measure.node()?;
        measure.raw(name, maximum_bytes)?;
    }
    Ok(())
}

pub(super) fn prepare_replace_frame_caller(
    input: CompletePolicyReplaceInput,
    limits: PolicySchemaLimits,
    admission: AdmissionLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<(CompletePolicyReplaceCaller, usize, usize), CompletePolicyRequestError> {
    if charge.profile() != limits {
        return Err(PolicySchemaError::ProfileMismatch.into());
    }
    let (intent_bytes, frame_bytes) = {
        let mut measure = IntentMeasure::new(charge);
        measure.node()?;
        check_header(input.scope())?;
        let measured = measure_intent(&input, limits, measure)?;
        let (header_bytes, header_nodes) =
            measure_replace_frame_header(&input, measured.bytes, charge)?;
        let frame_bytes = header_bytes
            .checked_add(measured.bytes)
            .ok_or(PolicySchemaError::LengthOverflow)?;
        if frame_bytes > admission.decision_bytes() {
            return Err(CompletePolicyEncodingError::ByteLimit.into());
        }
        super::replace_frame::check_replace_frame_lists(measured.data, charge)?;
        prepay_intent(&measured, charge)?;
        prepay_replace_frame(&measured, header_bytes, header_nodes, frame_bytes, charge)?;
        (measured.bytes, frame_bytes)
    };
    Ok((
        CompletePolicyReplaceCaller {
            input,
            schema_limits: limits,
            admission_limits: admission,
        },
        intent_bytes,
        frame_bytes,
    ))
}

fn measure_replace_frame_header(
    input: &CompletePolicyReplaceInput,
    intent_bytes: usize,
    charge: &mut PolicySchemaCharge,
) -> Result<(usize, u64), CompletePolicyRequestError> {
    let mut measure = IntentMeasure::new(charge);
    measure.node()?;
    measure.write(b"FWCP")?;
    measure.u16(1)?;
    measure.u8(6)?;
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

fn prepay_replace_frame(
    measured: &MeasuredCompletePolicyIntent<'_>,
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
