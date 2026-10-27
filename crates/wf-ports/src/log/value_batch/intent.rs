//! Exact caller stream, with server preparation context excluded explicitly.

use sha2::{Digest, Sha256};
use wf_domain::workflow::Operation;
use wf_domain::workflow::active::{LifecycleOverrides, MaxActiveOverride};
use wf_domain::workflow::request::ClientIntent;

use crate::consensus::{
    PreparedBatchItem, PreparedOperation, PreparedWorkflowValueBatch, WorkflowValueItemInput,
};
use crate::log::{CodecError, CodecLimits, public_mutation, record_batch};

pub(super) fn derive(
    batch: &PreparedWorkflowValueBatch,
    limits: CodecLimits,
) -> Result<ClientIntent, CodecError> {
    let items = batch
        .operation
        .mutation_items()
        .ok_or(CodecError::UnsupportedTag)?;
    let mut hash = Sha256::new();
    hash.update(b"WF-WORKFLOW-VALUE\0V8\0");
    let mut size = 21_usize;
    for value in [
        batch.scope.tenant().as_bytes(),
        batch.scope.group().as_bytes(),
        batch.scope.recovery().as_bytes(),
    ] {
        raw(&mut hash, &mut size, value)?;
    }
    hash.update([batch.profile]);
    hash.update(
        u32::try_from(items.len())
            .map_err(|_| CodecError::LengthOverflow)?
            .to_be_bytes(),
    );
    size = size.checked_add(5).ok_or(CodecError::LengthOverflow)?;
    match (&batch.operation, batch.profile) {
        (PreparedOperation::PublicMutation { item, evidence }, 1) => {
            public_mutation::with_caller(item, evidence, limits, |caller| {
                public_mutation::canonical(batch.scope.tenant().as_bytes(), caller, &mut |bytes| {
                    size = size
                        .checked_add(bytes.len())
                        .ok_or(CodecError::LengthOverflow)?;
                    hash.update(bytes);
                    Ok(())
                })
            })?;
        }
        (PreparedOperation::Batch(_), 5..=7) => {
            for item in items {
                raw(&mut hash, &mut size, item.partition.as_bytes())?;
                raw(&mut hash, &mut size, item.command.id.as_bytes())?;
                if batch.profile == 7 {
                    start(item, &mut hash, &mut size)?;
                } else {
                    // The unchanged old worker counts leaves only. This fixed
                    // allowance is an overflow/work upper, never a frame cap.
                    size = size.checked_add(256).ok_or(CodecError::LengthOverflow)?;
                    let mut variable = 0;
                    record_batch::hash_operation(
                        &mut hash,
                        &mut variable,
                        &item.command.operation,
                        item.create_priority,
                    )?;
                    size = size
                        .checked_add(variable)
                        .ok_or(CodecError::LengthOverflow)?;
                }
            }
        }
        _ => return Err(CodecError::UnsupportedTag),
    }
    extensions(&batch.inputs, batch.profile, &mut hash, &mut size)?;
    Ok(ClientIntent::workflow_value_batch(hash.finalize().into()))
}

fn extensions(
    inputs: &[WorkflowValueItemInput],
    mode: u8,
    hash: &mut Sha256,
    size: &mut usize,
) -> Result<(), CodecError> {
    emit_bytes(
        hash,
        size,
        &u32::try_from(inputs.len())
            .map_err(|_| CodecError::LengthOverflow)?
            .to_be_bytes(),
    )?;
    for input in inputs {
        emit_bytes(hash, size, &[u8::from(input.birth.is_some())])?;
        if let Some(birth) = input.birth {
            birth_bytes(birth, hash, size)?;
        }
        emit_bytes(hash, size, &[u8::from(input.terminal_ttl_ms.is_some())])?;
        if let Some(value) = input.terminal_ttl_ms {
            emit_bytes(hash, size, &value.to_be_bytes())?;
        }
        if mode != 1 {
            emit_bytes(hash, size, &[u8::from(input.requested_at.is_some())])?;
            if let Some(value) = input.requested_at {
                emit_bytes(hash, size, &value.get().to_be_bytes())?;
            }
        }
    }
    Ok(())
}

pub(super) fn raw(hash: &mut Sha256, size: &mut usize, value: &[u8]) -> Result<(), CodecError> {
    let length = u32::try_from(value.len()).map_err(|_| CodecError::LengthOverflow)?;
    *size = size
        .checked_add(4)
        .and_then(|size| size.checked_add(value.len()))
        .ok_or(CodecError::LengthOverflow)?;
    hash.update(length.to_be_bytes());
    hash.update(value);
    Ok(())
}

pub(super) fn start(
    item: &PreparedBatchItem,
    hash: &mut Sha256,
    size: &mut usize,
) -> Result<(), CodecError> {
    let Operation::StartAndClaim(value) = &item.command.operation else {
        return Err(CodecError::UnsupportedTag);
    };
    *size = size.checked_add(11).ok_or(CodecError::LengthOverflow)?;
    hash.update([11, item.create_priority]);
    for bytes in [&value.workflow_type, &value.state, &value.worker] {
        raw(hash, size, bytes)?;
    }
    hash.update(value.lease_ms.to_be_bytes());
    hash.update([u8::from(value.payload.is_some())]);
    if let Some(value) = &value.payload {
        raw(hash, size, value)?;
    }
    Ok(())
}

fn emit_bytes(hash: &mut Sha256, size: &mut usize, bytes: &[u8]) -> Result<(), CodecError> {
    *size = size
        .checked_add(bytes.len())
        .ok_or(CodecError::LengthOverflow)?;
    hash.update(bytes);
    Ok(())
}

fn birth_bytes(
    birth: LifecycleOverrides,
    hash: &mut Sha256,
    size: &mut usize,
) -> Result<(), CodecError> {
    match birth.active {
        MaxActiveOverride::Inherit => emit_bytes(hash, size, &[0])?,
        MaxActiveOverride::Infinite => emit_bytes(hash, size, &[1])?,
        MaxActiveOverride::Millis(value) => {
            emit_bytes(hash, size, &[2])?;
            emit_bytes(hash, size, &value.to_be_bytes())?;
        }
    }
    for (value, width) in [
        (birth.retention.ttl_ms, 8),
        (birth.retention.history_max_events.map(u64::from), 4),
        (birth.history_hot_max_events, 8),
    ] {
        emit_bytes(hash, size, &[u8::from(value.is_some())])?;
        if let Some(value) = value {
            emit_bytes(hash, size, &value.to_be_bytes()[8 - width..])?;
        }
    }
    Ok(())
}
