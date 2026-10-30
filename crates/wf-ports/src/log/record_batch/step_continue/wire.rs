//! One measure/emit grammar and complete borrowed scan before owned fields.

use wf_domain::lease::{LeaseGuard, LeaseToken};
use wf_domain::workflow::request::{ClientIntent, RequestScope};
use wf_domain::workflow::{CommittedCommand, Operation, StepContinue};
use wf_types::identity::{GroupId, PartitionId, RecordId, RecoveryId, RequestId, TenantId};
use wf_types::version::Fence;

use crate::consensus::{PreparedBatchItem, PreparedOperation, PreparedRequest};
use crate::log::{
    CodecError, CodecLimits, DecodedRequest, common, digest, keys,
    reader::{Reader, copy},
    writer::Writer,
};

use super::super::reserve;
use super::input::StepInput;

pub(super) fn write(
    input: &StepInput<'_>,
    intent: ClientIntent,
    writer: &mut Writer<'_>,
    limits: CodecLimits,
) -> Result<(), CodecError> {
    let scope_length = keys::scope_length(input.scope)?;
    let mut derived = scope_length;
    keys::charge(
        &mut derived,
        keys::state_length(scope_length, input.partition.len(), input.id.len())?,
        limits.admission.decision_bytes(),
    )?;
    writer.write(b"FWCP")?;
    writer.u16(1)?;
    writer.byte(1)?;
    for (bytes, maximum) in input.scope.into_iter().zip([8192, 256, 256]) {
        writer.raw(bytes, maximum)?;
    }
    writer.raw(input.request, 256)?;
    writer.u16(intent.version())?;
    writer.write(intent.digest())?;
    writer.u64(input.stamp.hlc().physical().get())?;
    writer.u16(input.stamp.hlc().logical())?;
    writer.u64(input.stamp.wall().get())?;
    writer.u32(1)?;
    writer.raw(input.partition, 65_535)?;
    writer.raw(input.id, 65_535)?;
    writer.byte(input.priority)?;
    writer.u64(input.now.get())?;
    writer.byte(12)?;
    let maximum = limits.admission.inline_bytes();
    writer.raw(input.token, maximum)?;
    writer.u64(input.fence)?;
    writer.raw(input.from, maximum)?;
    writer.raw(input.to, maximum)?;
    writer.optional(input.worker, |writer, bytes| writer.raw(bytes, maximum))?;
    writer.u64(input.lease_ms)?;
    writer.optional(input.payload, |writer, bytes| writer.raw(bytes, maximum))
}

pub(super) fn scan(
    encoded: &[u8],
    limits: CodecLimits,
    version: u16,
) -> Result<StepInput<'_>, CodecError> {
    if encoded.len() > limits.admission.decision_bytes() {
        return Err(CodecError::ByteLimit);
    }
    let mut reader = Reader::new(encoded, limits);
    let header = common::singleton_header(&mut reader, version)?;
    let scope_length = keys::scope_length(header.scope())?;
    if common::count(&mut reader)? != 1 {
        return Err(CodecError::ItemLimit);
    }
    let item = common::item(&mut reader)?;
    let mut derived = scope_length;
    keys::charge(
        &mut derived,
        keys::state_length(scope_length, item.partition.len(), item.id.len())?,
        limits.admission.decision_bytes(),
    )?;
    if item.tag != 12 {
        return Err(CodecError::UnsupportedTag);
    }
    let token = reader.nonempty(limits.admission.inline_bytes())?;
    let fence = reader.u64()?;
    let from = reader.field()?;
    let to = reader.field()?;
    let worker = reader.optional(Reader::field)?;
    let lease_ms = reader.u64()?;
    let payload = reader.optional(Reader::field)?;
    reader.finish()?;
    Ok(StepInput {
        scope: [header.tenant, header.group, header.recovery],
        request: header.request,
        intent: header.intent,
        stamp: header.stamp,
        partition: item.partition,
        id: item.id,
        priority: item.priority,
        now: item.now,
        token,
        fence,
        from,
        to,
        worker,
        lease_ms,
        payload,
    })
}

pub(super) fn materialize<'a>(
    input: &StepInput<'_>,
    encoded: &'a [u8],
    intent: ClientIntent,
    limits: CodecLimits,
) -> Result<DecodedRequest<'a>, CodecError> {
    let isolation = keys::scope(input.scope)?;
    let scope = RequestScope::new(
        TenantId::new(copy(input.scope[0])?).map_err(|_| CodecError::Malformed)?,
        GroupId::new(copy(input.scope[1])?).map_err(|_| CodecError::Malformed)?,
        RecoveryId::new(copy(input.scope[2])?).map_err(|_| CodecError::Malformed)?,
    );
    let request = RequestId::new(copy(input.request)?).map_err(|_| CodecError::Malformed)?;
    let mut state_keys = reserve(1)?;
    state_keys.push(keys::state(&isolation, input.partition, input.id)?);
    let mut items = reserve(1)?;
    items.push(PreparedBatchItem {
        partition: PartitionId::new(copy(input.partition)?).map_err(|_| CodecError::Malformed)?,
        command: CommittedCommand {
            id: RecordId::new(copy(input.id)?).map_err(|_| CodecError::Malformed)?,
            now: input.now,
            operation: Operation::StepContinue(StepContinue {
                guard: LeaseGuard {
                    token: LeaseToken::new(copy(input.token)?)
                        .map_err(|_| CodecError::Malformed)?,
                    fence: Fence::new(input.fence),
                },
                from: copy(input.from)?,
                to: copy(input.to)?,
                worker: input.worker.map(copy).transpose()?,
                lease_ms: input.lease_ms,
                payload: input.payload.map(copy).transpose()?,
            }),
        },
        create_priority: input.priority,
    });
    Ok(DecodedRequest {
        request: PreparedRequest {
            scope,
            request,
            intent,
            stamp: input.stamp,
            operation: PreparedOperation::Batch(items),
        },
        encoded,
        digest: digest(encoded),
        isolation,
        state_keys,
        claim_filters: None,
        value_metadata: None,
        limits,
        record_batch_intent: Ok(intent),
    })
}
