//! Singleton intent7/item11 codec with borrowed validation before materialization.

use sha2::{Digest, Sha256};
use wf_domain::workflow::request::{ClientIntent, RequestScope};
use wf_domain::workflow::{CommittedCommand, Operation, StartAndClaim};
use wf_types::identity::{GroupId, PartitionId, RecordId, RecoveryId, RequestId, TenantId};
use wf_types::version::{CommittedTime, HlcTimestamp};

use crate::consensus::{PreparedBatchItem, PreparedOperation, PreparedRequest};
use crate::log::common::RawHeader;
use crate::log::reader::{Reader, copy};
use crate::log::writer::Writer;
use crate::log::{CodecError, CodecLimits, DecodedRequest, PreparedBytes, common, digest, keys};

use super::super::{RecordBatchApplyError, admit, check_decoded, decoder_bound, reserve, sum};

/// Derives the singleton caller identity and measures the exact new frame.
/// # Errors
/// Refuses unsupported operations, actual field caps and prospective workspace.
pub fn encode_atomic_start(
    mut request: PreparedRequest,
    limits: CodecLimits,
    workspace_limit: usize,
) -> Result<PreparedBytes, RecordBatchApplyError> {
    let (intent, input_bytes) = derive_intent(&request)?;
    request.intent = intent;
    let mut measure = Writer::measure(limits.admission.decision_bytes());
    write(&request, &mut measure, limits)?;
    admit(
        sum(&[input_bytes, 1_024, 512, measure.len()])?,
        workspace_limit,
    )?;
    let mut output = Writer::allocate(measure.len(), limits.admission.decision_bytes())?;
    write(&request, &mut output, limits)?;
    let bytes = output.finish()?;
    Ok(PreparedBytes {
        digest: digest(&bytes),
        bytes,
    })
}

/// Admits the complete borrowed structure and decoded residency before allocation.
/// # Errors
/// Rejects unsupported profiles, malformed fields/intent and workspace excess.
pub fn prepare_atomic_start(
    encoded: &[u8],
    limits: CodecLimits,
    workspace_limit: usize,
) -> Result<DecodedRequest<'_>, RecordBatchApplyError> {
    preflight(encoded, limits)?;
    admit(decoder_bound(encoded.len(), 1, limits)?, workspace_limit)?;
    let decoded = materialize(encoded, limits)?;
    check_decoded(&decoded, limits, workspace_limit, 7)?;
    Ok(decoded)
}

fn derive_intent(request: &PreparedRequest) -> Result<(ClientIntent, usize), CodecError> {
    let PreparedOperation::Batch(items) = &request.operation else {
        return Err(CodecError::UnsupportedTag);
    };
    let [item] = items.as_slice() else {
        return Err(CodecError::ItemLimit);
    };
    let Operation::StartAndClaim(value) = &item.command.operation else {
        return Err(CodecError::UnsupportedTag);
    };
    let mut hash = Sha256::new();
    let mut bytes = 0;
    hash.update(b"WF-ATOMIC-START\0V7\0");
    for field in [
        request.scope.tenant().as_bytes(),
        item.partition.as_bytes(),
        item.command.id.as_bytes(),
        value.workflow_type.as_slice(),
        value.state.as_slice(),
        value.worker.as_slice(),
    ] {
        hash_raw(&mut hash, &mut bytes, field)?;
    }
    hash.update(value.lease_ms.to_be_bytes());
    hash.update([item.create_priority, u8::from(value.payload.is_some())]);
    if let Some(payload) = &value.payload {
        hash_raw(&mut hash, &mut bytes, payload)?;
    }
    for field in [
        request.scope.group().as_bytes(),
        request.scope.recovery().as_bytes(),
        request.request.as_bytes(),
    ] {
        bytes = bytes
            .checked_add(field.len())
            .ok_or(CodecError::LengthOverflow)?;
    }
    Ok((ClientIntent::atomic_start(hash.finalize().into()), bytes))
}

fn write(
    request: &PreparedRequest,
    writer: &mut Writer<'_>,
    limits: CodecLimits,
) -> Result<(), CodecError> {
    if request.intent.version() != 7 {
        return Err(CodecError::UnsupportedVersion);
    }
    let PreparedOperation::Batch(items) = &request.operation else {
        return Err(CodecError::UnsupportedTag);
    };
    let [item] = items.as_slice() else {
        return Err(CodecError::ItemLimit);
    };
    let Operation::StartAndClaim(value) = &item.command.operation else {
        return Err(CodecError::UnsupportedTag);
    };
    let scope = [
        request.scope.tenant().as_bytes(),
        request.scope.group().as_bytes(),
        request.scope.recovery().as_bytes(),
    ];
    let scope_length = keys::scope_length(scope)?;
    let mut derived = scope_length;
    keys::charge(
        &mut derived,
        keys::state_length(
            scope_length,
            item.partition.as_bytes().len(),
            item.command.id.as_bytes().len(),
        )?,
        limits.admission.decision_bytes(),
    )?;
    writer.write(b"FWCP")?;
    writer.u16(1)?;
    writer.byte(1)?;
    for field in scope {
        writer.raw(field, 8_192)?;
    }
    writer.raw(request.request.as_bytes(), 256)?;
    writer.u16(7)?;
    writer.write(request.intent.digest())?;
    writer.u64(request.stamp.hlc().physical().get())?;
    writer.u16(request.stamp.hlc().logical())?;
    writer.u64(request.stamp.wall().get())?;
    writer.u32(1)?;
    writer.raw(item.partition.as_bytes(), 65_535)?;
    writer.raw(item.command.id.as_bytes(), 65_535)?;
    writer.byte(item.create_priority)?;
    writer.u64(item.command.now.get())?;
    writer.byte(11)?;
    for field in [&value.workflow_type, &value.state, &value.worker] {
        writer.raw(field, limits.admission.inline_bytes())?;
    }
    writer.u64(value.lease_ms)?;
    writer.optional(value.payload.as_deref(), |writer, bytes| {
        writer.raw(bytes, limits.admission.inline_bytes())
    })
}

fn header<'input>(reader: &mut Reader<'input>) -> Result<RawHeader<'input>, CodecError> {
    if reader.take(4)? != b"FWCP" {
        return Err(CodecError::Malformed);
    }
    if reader.u16()? != 1 {
        return Err(CodecError::UnsupportedVersion);
    }
    if reader.byte()? != 1 {
        return Err(CodecError::UnsupportedTag);
    }
    let tenant = reader.nonempty(8_192)?;
    let group = reader.nonempty(256)?;
    let recovery = reader.nonempty(256)?;
    let request = reader.nonempty(256)?;
    if reader.u16()? != 7 {
        return Err(CodecError::UnsupportedVersion);
    }
    let intent = ClientIntent::atomic_start(reader.fixed()?);
    let hlc = HlcTimestamp::new(reader.u64()?, reader.u16()?).map_err(|_| CodecError::Malformed)?;
    let stamp = CommittedTime::new(hlc, reader.u64()?).map_err(|_| CodecError::Malformed)?;
    Ok(RawHeader {
        operation_tag: 1,
        tenant,
        group,
        recovery,
        request,
        intent,
        stamp,
    })
}

fn preflight(encoded: &[u8], limits: CodecLimits) -> Result<(), CodecError> {
    if encoded.len() > limits.admission.decision_bytes() {
        return Err(CodecError::ByteLimit);
    }
    let mut reader = Reader::new(encoded, limits);
    let header = header(&mut reader)?;
    let scope = keys::scope_length(header.scope())?;
    let mut derived = scope;
    if common::count(&mut reader)? != 1 {
        return Err(CodecError::ItemLimit);
    }
    let item = common::item(&mut reader)?;
    keys::charge(
        &mut derived,
        keys::state_length(scope, item.partition.len(), item.id.len())?,
        limits.admission.decision_bytes(),
    )?;
    if item.tag != 11 {
        return Err(CodecError::UnsupportedTag);
    }
    for _ in 0..3 {
        reader.field()?;
    }
    reader.u64()?;
    reader.optional(Reader::field)?;
    reader.finish()
}

fn materialize(encoded: &[u8], limits: CodecLimits) -> Result<DecodedRequest<'_>, CodecError> {
    let mut reader = Reader::new(encoded, limits);
    let header = header(&mut reader)?;
    let isolation = keys::scope(header.scope())?;
    let scope = RequestScope::new(
        TenantId::new(copy(header.tenant)?).map_err(|_| CodecError::Malformed)?,
        GroupId::new(copy(header.group)?).map_err(|_| CodecError::Malformed)?,
        RecoveryId::new(copy(header.recovery)?).map_err(|_| CodecError::Malformed)?,
    );
    let request = RequestId::new(copy(header.request)?).map_err(|_| CodecError::Malformed)?;
    if common::count(&mut reader)? != 1 {
        return Err(CodecError::ItemLimit);
    }
    let item = common::item(&mut reader)?;
    let mut state_keys = reserve(1)?;
    state_keys.push(keys::state(&isolation, item.partition, item.id)?);
    let mut items = reserve(1)?;
    items.push(PreparedBatchItem {
        partition: PartitionId::new(copy(item.partition)?).map_err(|_| CodecError::Malformed)?,
        command: CommittedCommand {
            id: RecordId::new(copy(item.id)?).map_err(|_| CodecError::Malformed)?,
            now: item.now,
            operation: Operation::StartAndClaim(StartAndClaim {
                workflow_type: reader.owned()?,
                state: reader.owned()?,
                worker: reader.owned()?,
                lease_ms: reader.u64()?,
                payload: reader.optional(Reader::owned)?,
            }),
        },
        create_priority: item.priority,
    });
    reader.finish()?;
    let request = PreparedRequest {
        scope,
        request,
        intent: header.intent,
        stamp: header.stamp,
        operation: PreparedOperation::Batch(items),
    };
    let record_batch_intent = derive_intent(&request).map(|(intent, _)| intent);
    Ok(DecodedRequest {
        request,
        encoded,
        digest: digest(encoded),
        isolation,
        state_keys,
        claim_filters: None,
        limits,
        record_batch_intent,
    })
}

fn hash_raw(hash: &mut Sha256, bytes: &mut usize, value: &[u8]) -> Result<(), CodecError> {
    let length = u32::try_from(value.len()).map_err(|_| CodecError::LengthOverflow)?;
    *bytes = bytes
        .checked_add(value.len())
        .ok_or(CodecError::LengthOverflow)?;
    hash.update(length.to_be_bytes());
    hash.update(value);
    Ok(())
}
