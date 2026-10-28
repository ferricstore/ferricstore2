//! Borrowed singleton fields and exact private caller identity, without authority.

use sha2::{Digest, Sha256};
use wf_domain::workflow::Operation;
use wf_domain::workflow::request::ClientIntent;
use wf_types::version::{CommittedTime, TimestampMillis};

use crate::{
    consensus::{PreparedOperation, PreparedRequest},
    log::CodecError,
};

pub(super) struct StepInput<'a> {
    pub(super) scope: [&'a [u8]; 3],
    pub(super) request: &'a [u8],
    pub(super) intent: ClientIntent,
    pub(super) stamp: CommittedTime,
    pub(super) partition: &'a [u8],
    pub(super) id: &'a [u8],
    pub(super) priority: u8,
    pub(super) now: TimestampMillis,
    pub(super) token: &'a [u8],
    pub(super) fence: u64,
    pub(super) from: &'a [u8],
    pub(super) to: &'a [u8],
    pub(super) worker: Option<&'a [u8]>,
    pub(super) lease_ms: u64,
    pub(super) payload: Option<&'a [u8]>,
}

pub(super) fn borrow(request: &PreparedRequest) -> Result<StepInput<'_>, CodecError> {
    let PreparedOperation::Batch(items) = &request.operation else {
        return Err(CodecError::UnsupportedTag);
    };
    let [item] = items.as_slice() else {
        return Err(CodecError::ItemLimit);
    };
    let Operation::StepContinue(step) = &item.command.operation else {
        return Err(CodecError::UnsupportedTag);
    };
    Ok(StepInput {
        scope: [
            request.scope.tenant().as_bytes(),
            request.scope.group().as_bytes(),
            request.scope.recovery().as_bytes(),
        ],
        request: request.request.as_bytes(),
        intent: request.intent,
        stamp: request.stamp,
        partition: item.partition.as_bytes(),
        id: item.command.id.as_bytes(),
        priority: item.create_priority,
        now: item.command.now,
        token: step.guard.token.as_bytes(),
        fence: step.guard.fence.get(),
        from: &step.from,
        to: &step.to,
        worker: step.worker.as_deref(),
        lease_ms: step.lease_ms,
        payload: step.payload.as_deref(),
    })
}

pub(super) fn variable_bytes(input: &StepInput<'_>) -> Result<usize, CodecError> {
    [
        input.scope[0],
        input.scope[1],
        input.scope[2],
        input.request,
        input.partition,
        input.id,
        input.token,
        input.from,
        input.to,
        input.worker.unwrap_or(b""),
        input.payload.unwrap_or(b""),
    ]
    .into_iter()
    .try_fold(0_usize, |sum, bytes| {
        sum.checked_add(bytes.len())
            .ok_or(CodecError::LengthOverflow)
    })
}

pub(super) fn derive(input: &StepInput<'_>) -> Result<ClientIntent, CodecError> {
    let mut hash = Sha256::new();
    hash.update(b"WF-STEP-CONTINUE\0V10\0");
    for bytes in [
        input.scope[0],
        input.scope[1],
        input.scope[2],
        input.partition,
        input.id,
        input.token,
    ] {
        raw(&mut hash, bytes)?;
    }
    hash.update(input.fence.to_be_bytes());
    raw(&mut hash, input.from)?;
    raw(&mut hash, input.to)?;
    optional(&mut hash, input.worker)?;
    hash.update(input.lease_ms.to_be_bytes());
    optional(&mut hash, input.payload)?;
    Ok(ClientIntent::step_continue(hash.finalize().into()))
}

fn raw(hash: &mut Sha256, bytes: &[u8]) -> Result<(), CodecError> {
    let length = u32::try_from(bytes.len()).map_err(|_| CodecError::LengthOverflow)?;
    hash.update(length.to_be_bytes());
    hash.update(bytes);
    Ok(())
}

fn optional(hash: &mut Sha256, value: Option<&[u8]>) -> Result<(), CodecError> {
    hash.update([u8::from(value.is_some())]);
    if let Some(bytes) = value {
        raw(hash, bytes)?;
    }
    Ok(())
}
