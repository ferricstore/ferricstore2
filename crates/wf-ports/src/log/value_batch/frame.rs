//! Exact outer-eight frame; borrowed validation precedes owned materialization.

use sha2::{Digest, Sha256};
use wf_domain::workflow::request::{ClientIntent, RequestScope};
use wf_types::identity::{GroupId, RecoveryId, RequestId, TenantId};

use super::budget::{BYTE_WORK, ValueBatchCharge};
use super::{metadata, operation};
use crate::consensus::{PreparedWorkflowValueBatch, WorkflowValueDefaults};
use crate::log::{
    CodecError, CodecLimits, common, keys,
    reader::{self, Reader},
    writer::Writer,
};

pub(super) fn write(
    batch: &PreparedWorkflowValueBatch,
    intent: Option<ClientIntent>,
    writer: &mut Writer<'_>,
    limits: CodecLimits,
) -> Result<(), CodecError> {
    if !matches!(batch.profile, 1 | 5 | 6 | 7) {
        return Err(CodecError::UnsupportedTag);
    }
    if batch.policies.is_empty() || batch.policies.len() > limits.admission().batch_items() {
        return Err(CodecError::ItemLimit);
    }
    let scope = [
        batch.scope.tenant().as_bytes(),
        batch.scope.group().as_bytes(),
        batch.scope.recovery().as_bytes(),
    ];
    let scope_length = keys::scope_length(scope)?;
    let mut derived = scope_length;
    let items = batch
        .operation
        .mutation_items()
        .ok_or(CodecError::UnsupportedTag)?;
    if batch.inputs.len() != items.len() {
        return Err(CodecError::Malformed);
    }
    for item in items {
        keys::charge(
            &mut derived,
            keys::state_length(
                scope_length,
                item.partition.as_bytes().len(),
                item.command.id.as_bytes().len(),
            )?,
            limits.admission().decision_bytes(),
        )?;
    }
    writer.write(b"FWCP")?;
    writer.u16(1)?;
    writer.byte(1)?;
    for (bytes, maximum) in scope.into_iter().zip([8_192, 256, 256]) {
        writer.raw(bytes, maximum)?;
    }
    writer.raw(batch.request.as_bytes(), 256)?;
    writer.u16(8)?;
    writer.write(
        intent
            .as_ref()
            .map_or(&[0; 32], |value| value.digest().as_slice()),
    )?;
    writer.u64(batch.stamp.hlc().physical().get())?;
    writer.u16(batch.stamp.hlc().logical())?;
    writer.u64(batch.stamp.wall().get())?;
    writer.byte(1)?;
    writer.byte(batch.profile)?;
    writer.optional(
        batch.policy_references_captured.as_ref(),
        |writer, value| writer.byte(u8::from(*value)),
    )?;
    if intent.is_some() {
        writer.write(&batch.defaults.to_bytes()?)?;
    } else {
        writer.write(&[0; 60])?;
    }
    metadata::write_policies(&batch.policies, writer)?;
    operation::write(&batch.operation, batch.profile, writer, limits)?;
    metadata::write_inputs(&batch.inputs, writer)
}

pub(super) fn preflight(
    encoded: &[u8],
    limits: CodecLimits,
    charge: &mut ValueBatchCharge,
) -> Result<(usize, usize, usize, usize, ClientIntent), CodecError> {
    if encoded.len() > limits.admission().decision_bytes() {
        return Err(CodecError::ByteLimit);
    }
    charge.debit(
        encoded
            .len()
            .checked_mul(BYTE_WORK)
            .ok_or(CodecError::LengthOverflow)?,
    )?;
    let mut reader = Reader::new(encoded, limits);
    let header = common::header(&mut reader)?;
    if header.operation_tag != 1 || header.intent.version() != 8 {
        return Err(CodecError::UnsupportedVersion);
    }
    let scope_length = keys::scope_length(header.scope())?;
    if reader.byte()? != 1 {
        return Err(CodecError::UnsupportedVersion);
    }
    let mode = reader.byte()?;
    if !matches!(mode, 1 | 5 | 6 | 7) {
        return Err(CodecError::UnsupportedTag);
    }
    reader.optional(|reader| match reader.byte()? {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(CodecError::InvalidOptionality),
    })?;
    WorkflowValueDefaults::from_bytes(reader.fixed()?)?;
    let policies = metadata::scan_policies(&mut reader, charge)?;
    let (items, derived) = operation::scan(&mut reader, mode, scope_length, charge)?;
    metadata::scan_inputs(&mut reader, items, policies)?;
    let prefix_length = encoded
        .len()
        .checked_sub(reader.remaining())
        .ok_or(CodecError::LengthOverflow)?;
    let footer: [u8; 32] = reader.fixed()?;
    let actual: [u8; 32] = Sha256::digest(&encoded[..prefix_length]).into();
    if actual != footer {
        return Err(CodecError::Malformed);
    }
    reader.finish()?;
    Ok((items, policies, derived, encoded.len(), header.intent))
}

pub(super) fn materialize(
    encoded: &[u8],
    limits: CodecLimits,
) -> Result<PreparedWorkflowValueBatch, CodecError> {
    let mut reader = Reader::new(encoded, limits);
    let header = common::header(&mut reader)?;
    if reader.byte()? != 1 {
        return Err(CodecError::UnsupportedVersion);
    }
    let profile = reader.byte()?;
    let policy_references_captured = reader.optional(|reader| match reader.byte()? {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(CodecError::InvalidOptionality),
    })?;
    let defaults = WorkflowValueDefaults::from_bytes(reader.fixed()?)?;
    let policies = metadata::read_policies(&mut reader)?;
    let operation = operation::read(&mut reader, profile, limits)?;
    let count = operation
        .mutation_items()
        .ok_or(CodecError::UnsupportedTag)?
        .len();
    let inputs = metadata::read_inputs(&mut reader, count)?;
    reader.take(32)?;
    reader.finish()?;
    Ok(PreparedWorkflowValueBatch {
        scope: RequestScope::new(
            TenantId::new(reader::copy(header.tenant)?).map_err(|_| CodecError::Malformed)?,
            GroupId::new(reader::copy(header.group)?).map_err(|_| CodecError::Malformed)?,
            RecoveryId::new(reader::copy(header.recovery)?).map_err(|_| CodecError::Malformed)?,
        ),
        request: RequestId::new(reader::copy(header.request)?)
            .map_err(|_| CodecError::Malformed)?,
        stamp: header.stamp,
        profile,
        policy_references_captured,
        defaults,
        policies,
        operation,
        inputs,
    })
}
