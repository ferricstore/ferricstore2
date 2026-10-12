//! Private original core-Patch visitation for intent hashing and frame emission.

use wf_domain::policy::BackoffKind;
use wf_domain::policy::complete::{
    CompleteCorePolicyPatchCaller, CompleteCorePolicyPatchInput, CompletePolicyRequestError,
};
use wf_domain::policy::definition::{
    BackoffPatch, FieldPatch, PolicyVersion, RetryPatch, StateChange, StateMode, StatePolicyPatch,
};
use wf_domain::retention::RetentionOverride;

use super::intent::{CompletePolicyIntentSink, IntentHasher, map_codec_error};

mod frame;

pub use frame::{CompleteCorePolicyPatchRequestPlan, decide_complete_core_policy_patch_frame};
pub use frame::{
    DecodedCompleteCorePolicyPatchFrame, VerifiedCompleteCorePolicyPatchFrame,
    decode_complete_core_policy_patch_frame, prepare_complete_core_policy_patch_frame,
    verify_complete_core_policy_patch_frame,
};

pub(super) fn hash_core_patch_intent(
    caller: &CompleteCorePolicyPatchCaller,
) -> Result<[u8; 32], CompletePolicyRequestError> {
    let mut writer = IntentHasher::new();
    write_core_patch_intent(caller, &mut writer)?;
    Ok(writer.finish())
}

pub(super) fn write_core_patch_intent<W: CompletePolicyIntentSink>(
    caller: &CompleteCorePolicyPatchCaller,
    writer: &mut W,
) -> Result<(), CompletePolicyRequestError> {
    let input = caller.input();
    let command = input.command();
    writer.write(b"WF-INTENT\0COMPLETE-CORE-PATCH\0V2\0")?;
    writer.raw(input.scope().tenant().as_bytes())?;
    writer.optional(command.expected_generation.is_some())?;
    if let Some(expected) = command.expected_generation {
        writer.u64(expected)?;
    }
    write_core_patch(command, writer)
}

fn write_core_patch<W: CompletePolicyIntentSink>(
    command: &CompleteCorePolicyPatchInput,
    writer: &mut W,
) -> Result<(), CompletePolicyRequestError> {
    writer.raw(&command.workflow_type)?;
    match &command.patch.version {
        FieldPatch::Unchanged => writer.u8(0)?,
        FieldPatch::Set(version) => {
            writer.u8(1)?;
            write_core_patch_version(version, writer)?;
        }
        FieldPatch::Clear => writer.u8(2)?,
    }
    match &command.patch.max_active_ms {
        FieldPatch::Unchanged => writer.u8(0)?,
        FieldPatch::Set(maximum) => {
            writer.u8(1)?;
            writer.u64(*maximum)?;
        }
        FieldPatch::Clear => writer.u8(2)?,
    }
    match &command.patch.retry {
        FieldPatch::Unchanged => writer.u8(0)?,
        FieldPatch::Set(retry) => {
            writer.u8(1)?;
            write_core_patch_retry(retry, writer)?;
        }
        FieldPatch::Clear => writer.u8(2)?,
    }
    match &command.patch.retention {
        FieldPatch::Unchanged => writer.u8(0)?,
        FieldPatch::Set(retention) => {
            writer.u8(1)?;
            write_core_patch_retention(retention, writer)?;
        }
        FieldPatch::Clear => writer.u8(2)?,
    }
    writer.count(command.patch.states.len())?;
    for (key, change) in &command.patch.states {
        writer.raw(key)?;
        match change {
            StateChange::Remove => writer.u8(0)?,
            StateChange::Patch(state) => {
                writer.u8(1)?;
                write_core_patch_state(state, writer)?;
            }
        }
    }
    Ok(())
}

fn write_core_patch_state<W: CompletePolicyIntentSink>(
    state: &StatePolicyPatch,
    writer: &mut W,
) -> Result<(), CompletePolicyRequestError> {
    match &state.mode {
        FieldPatch::Unchanged => writer.u8(0)?,
        FieldPatch::Set(mode) => {
            writer.u8(1)?;
            writer.u8(match mode {
                StateMode::Parallel => 0,
                StateMode::Fifo => 1,
            })?;
        }
        FieldPatch::Clear => writer.u8(2)?,
    }
    match &state.retry {
        FieldPatch::Unchanged => writer.u8(0)?,
        FieldPatch::Set(retry) => {
            writer.u8(1)?;
            write_core_patch_retry(retry, writer)?;
        }
        FieldPatch::Clear => writer.u8(2)?,
    }
    match &state.retention {
        FieldPatch::Unchanged => writer.u8(0)?,
        FieldPatch::Set(retention) => {
            writer.u8(1)?;
            write_core_patch_retention(retention, writer)?;
        }
        FieldPatch::Clear => writer.u8(2)?,
    }
    Ok(())
}

fn write_core_patch_version<W: CompletePolicyIntentSink>(
    version: &PolicyVersion,
    writer: &mut W,
) -> Result<(), CompletePolicyRequestError> {
    match version {
        PolicyVersion::Bytes(bytes) => {
            writer.u8(0)?;
            writer.raw(bytes)
        }
        PolicyVersion::Number(number) => {
            writer.u8(1)?;
            writer.u64(*number)
        }
    }
}

fn write_core_patch_retry<W: CompletePolicyIntentSink>(
    retry: &RetryPatch,
    writer: &mut W,
) -> Result<(), CompletePolicyRequestError> {
    writer.optional(retry.max_retries.is_some())?;
    if let Some(maximum) = retry.max_retries {
        writer.u16(maximum)?;
    }
    writer.optional(retry.backoff.is_some())?;
    if let Some(backoff) = &retry.backoff {
        write_core_patch_backoff(backoff, writer)?;
    }
    writer.optional(retry.exhausted_to.is_some())?;
    if let Some(target) = &retry.exhausted_to {
        writer.raw(target)?;
    }
    Ok(())
}

fn write_core_patch_backoff<W: CompletePolicyIntentSink>(
    backoff: &BackoffPatch,
    writer: &mut W,
) -> Result<(), CompletePolicyRequestError> {
    writer.optional(backoff.kind.is_some())?;
    if let Some(kind) = backoff.kind {
        writer.u8(match kind {
            BackoffKind::None => 0,
            BackoffKind::Fixed => 1,
            BackoffKind::Linear => 2,
            BackoffKind::Exponential => 3,
        })?;
    }
    writer.optional(backoff.base_ms.is_some())?;
    if let Some(base) = backoff.base_ms {
        writer.u64(base)?;
    }
    writer.optional(backoff.max_ms.is_some())?;
    if let Some(maximum) = backoff.max_ms {
        writer.u64(maximum)?;
    }
    writer.optional(backoff.jitter_pct.is_some())?;
    if let Some(jitter) = backoff.jitter_pct {
        writer.u8(jitter)?;
    }
    Ok(())
}

fn write_core_patch_retention<W: CompletePolicyIntentSink>(
    retention: &RetentionOverride,
    writer: &mut W,
) -> Result<(), CompletePolicyRequestError> {
    writer.optional(retention.ttl_ms.is_some())?;
    if let Some(ttl) = retention.ttl_ms {
        writer.u64(ttl)?;
    }
    writer.optional(retention.history_max_events.is_some())?;
    if let Some(maximum) = retention.history_max_events {
        writer.u32(maximum)?;
    }
    Ok(())
}
