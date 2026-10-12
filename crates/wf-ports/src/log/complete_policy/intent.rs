//! Incremental Complete Replace intent hashing over original admitted owners.

use sha2::{Digest, Sha256};
use wf_domain::policy::complete::{
    CompletePolicyData, CompletePolicyEncodingError, CompletePolicyReplaceCaller,
    CompletePolicyRequestError, CompleteStatePolicyData, PolicySchemaError,
};
use wf_domain::policy::definition::{PolicyVersion, StateMode};
use wf_domain::policy::{Backoff, BackoffKind, RetryOverride};
use wf_domain::retention::RetentionOverride;

use super::super::{CodecError, writer::Writer};

mod governance;

pub(super) trait CompletePolicyIntentSink {
    fn write(&mut self, bytes: &[u8]) -> Result<(), CompletePolicyRequestError>;

    fn raw(&mut self, bytes: &[u8]) -> Result<(), CompletePolicyRequestError> {
        self.count(bytes.len())?;
        self.write(bytes)
    }

    fn count(&mut self, count: usize) -> Result<(), CompletePolicyRequestError> {
        self.u32(u32::try_from(count).or(Err(PolicySchemaError::LengthOverflow))?)
    }

    fn optional(&mut self, present: bool) -> Result<(), CompletePolicyRequestError> {
        self.u8(u8::from(present))
    }

    fn u8(&mut self, value: u8) -> Result<(), CompletePolicyRequestError> {
        self.write(&[value])
    }

    fn u16(&mut self, value: u16) -> Result<(), CompletePolicyRequestError> {
        self.write(&value.to_be_bytes())
    }

    fn u32(&mut self, value: u32) -> Result<(), CompletePolicyRequestError> {
        self.write(&value.to_be_bytes())
    }

    fn u64(&mut self, value: u64) -> Result<(), CompletePolicyRequestError> {
        self.write(&value.to_be_bytes())
    }
}

impl CompletePolicyIntentSink for IntentHasher {
    fn write(&mut self, bytes: &[u8]) -> Result<(), CompletePolicyRequestError> {
        IntentHasher::write(self, bytes)
    }
}

impl CompletePolicyIntentSink for Writer<'_> {
    fn write(&mut self, bytes: &[u8]) -> Result<(), CompletePolicyRequestError> {
        Writer::write(self, bytes).map_err(map_codec_error)
    }
}

pub(super) fn map_codec_error(error: CodecError) -> CompletePolicyRequestError {
    match error {
        CodecError::ScopeLimit => CompletePolicyRequestError::ScopeLimit,
        CodecError::LengthOverflow => PolicySchemaError::LengthOverflow.into(),
        CodecError::Allocation => CompletePolicyEncodingError::AllocationFailure.into(),
        CodecError::ByteLimit | CodecError::ValueLimit => {
            CompletePolicyEncodingError::ByteLimit.into()
        }
        _ => CompletePolicyEncodingError::Malformed.into(),
    }
}

pub(super) struct IntentHasher {
    hasher: Sha256,
    bytes: usize,
}

impl IntentHasher {
    pub(super) fn new() -> Self {
        Self {
            hasher: Sha256::new(),
            bytes: 0,
        }
    }

    pub(super) fn write(&mut self, bytes: &[u8]) -> Result<(), CompletePolicyRequestError> {
        let next = self
            .bytes
            .checked_add(bytes.len())
            .ok_or(PolicySchemaError::LengthOverflow)?;
        if next > 1_048_576 {
            return Err(CompletePolicyEncodingError::ByteLimit.into());
        }
        self.hasher.update(bytes);
        self.bytes = next;
        Ok(())
    }

    pub(super) fn finish(self) -> [u8; 32] {
        self.hasher.finalize().into()
    }
}

pub(super) fn hash_intent(
    caller: &CompletePolicyReplaceCaller,
) -> Result<[u8; 32], CompletePolicyRequestError> {
    let mut writer = IntentHasher::new();
    write_replace_intent(caller, &mut writer)?;
    Ok(writer.finish())
}

pub(super) fn write_replace_intent<W: CompletePolicyIntentSink>(
    caller: &CompletePolicyReplaceCaller,
    writer: &mut W,
) -> Result<(), CompletePolicyRequestError> {
    let input = caller.input();
    writer.write(b"WF-INTENT\0COMPLETE-REPLACE\0V2\0")?;
    writer.raw(input.scope().tenant().as_bytes())?;
    writer.optional(input.expected_generation().is_some())?;
    if let Some(generation) = input.expected_generation() {
        writer.u64(generation)?;
    }
    write_data(input.data(), writer)
}

fn write_data<W: CompletePolicyIntentSink>(
    data: &CompletePolicyData,
    writer: &mut W,
) -> Result<(), CompletePolicyRequestError> {
    writer.raw(&data.workflow_type)?;
    write_version(data.version.as_ref(), writer)?;
    writer.optional(data.max_active_ms.is_some())?;
    if let Some(value) = data.max_active_ms {
        writer.u64(value)?;
    }
    write_retry(data.retry.as_ref(), writer)?;
    write_retention(data.retention.as_ref(), writer)?;
    writer.count(data.states.len())?;
    for (key, state) in &data.states {
        writer.raw(key)?;
        write_state(state, writer)?;
    }
    write_names(data.indexed_attributes.as_deref(), writer)?;
    writer.optional(data.indexed_state_meta.is_some())?;
    if let Some(name) = &data.indexed_state_meta {
        writer.raw(name)?;
    }
    governance::write_governance(data.governance.as_ref(), writer)
}

fn write_state<W: CompletePolicyIntentSink>(
    state: &CompleteStatePolicyData,
    writer: &mut W,
) -> Result<(), CompletePolicyRequestError> {
    writer.optional(state.mode.is_some())?;
    if let Some(mode) = state.mode {
        writer.u8(match mode {
            StateMode::Parallel => 0,
            StateMode::Fifo => 1,
        })?;
    }
    write_retry(state.retry.as_ref(), writer)?;
    write_retention(state.retention.as_ref(), writer)?;
    governance::write_governance(state.governance.as_ref(), writer)
}

fn write_version<W: CompletePolicyIntentSink>(
    version: Option<&PolicyVersion>,
    writer: &mut W,
) -> Result<(), CompletePolicyRequestError> {
    writer.optional(version.is_some())?;
    match version {
        None => Ok(()),
        Some(PolicyVersion::Bytes(bytes)) => {
            writer.u8(0)?;
            writer.raw(bytes)
        }
        Some(PolicyVersion::Number(value)) => {
            writer.u8(1)?;
            writer.u64(*value)
        }
    }
}

fn write_retry<W: CompletePolicyIntentSink>(
    retry: Option<&RetryOverride>,
    writer: &mut W,
) -> Result<(), CompletePolicyRequestError> {
    writer.optional(retry.is_some())?;
    let Some(retry) = retry else {
        return Ok(());
    };
    writer.optional(retry.max_retries.is_some())?;
    if let Some(value) = retry.max_retries {
        writer.u16(value)?;
    }
    write_backoff(retry.backoff.as_ref(), writer)?;
    writer.optional(retry.exhausted_to.is_some())?;
    if let Some(target) = &retry.exhausted_to {
        writer.raw(target)?;
    }
    Ok(())
}

fn write_backoff<W: CompletePolicyIntentSink>(
    backoff: Option<&Backoff>,
    writer: &mut W,
) -> Result<(), CompletePolicyRequestError> {
    writer.optional(backoff.is_some())?;
    let Some(backoff) = backoff else {
        return Ok(());
    };
    writer.u8(match backoff.kind {
        BackoffKind::None => 0,
        BackoffKind::Fixed => 1,
        BackoffKind::Linear => 2,
        BackoffKind::Exponential => 3,
    })?;
    writer.u64(backoff.base_ms)?;
    writer.u64(backoff.max_ms)?;
    writer.u8(backoff.jitter_pct)
}

fn write_retention<W: CompletePolicyIntentSink>(
    retention: Option<&RetentionOverride>,
    writer: &mut W,
) -> Result<(), CompletePolicyRequestError> {
    writer.optional(retention.is_some())?;
    let Some(retention) = retention else {
        return Ok(());
    };
    writer.optional(retention.ttl_ms.is_some())?;
    if let Some(value) = retention.ttl_ms {
        writer.u64(value)?;
    }
    writer.optional(retention.history_max_events.is_some())?;
    if let Some(value) = retention.history_max_events {
        writer.u32(value)?;
    }
    Ok(())
}

fn write_names<W: CompletePolicyIntentSink>(
    names: Option<&[Vec<u8>]>,
    writer: &mut W,
) -> Result<(), CompletePolicyRequestError> {
    writer.optional(names.is_some())?;
    let Some(names) = names else {
        return Ok(());
    };
    writer.count(names.len())?;
    for name in names {
        writer.raw(name)?;
    }
    Ok(())
}
