//! A-N1: borrowed measurement and emission of the exact represented FWCP1 grammar.

use wf_domain::claim::Claim;
use wf_domain::lease::{LeaseGuard, LeaseToken};
use wf_domain::policy::{BackoffKind, RetryOverride};
use wf_domain::retry::{PolicyRetry, Retry, RetryTime};
use wf_domain::workflow::{Cancel, Create, Operation, Terminal, Transition};
use wf_types::version::TimestampMillis;

use crate::consensus::{PreparedOperation, PreparedRequest};

use super::{CodecError, CodecLimits, claim, keys, policy, writer::Writer};

pub(super) fn write(
    request: &PreparedRequest,
    writer: &mut Writer<'_>,
    limits: CodecLimits,
) -> Result<(), CodecError> {
    if matches!(request.operation, PreparedOperation::PublicMutation { .. }) {
        return super::public_mutation::write_owned(request, writer, limits);
    }
    if request.intent.version() != 1
        && !(matches!(request.intent.version(), 5 | 6)
            && matches!(request.operation, PreparedOperation::Batch(_)))
    {
        return Err(CodecError::UnsupportedVersion);
    }
    let scope = [
        request.scope.tenant().as_bytes(),
        request.scope.group().as_bytes(),
        request.scope.recovery().as_bytes(),
    ];
    let scope_length = keys::scope_length(scope)?;
    let mut derived = scope_length;
    writer.write(b"FWCP")?;
    writer.u16(1)?;
    writer.byte(match request.operation {
        PreparedOperation::Batch(_) => 1,
        PreparedOperation::InstallPolicy { .. } => 2,
        PreparedOperation::ClaimPhase(_) => 3,
        PreparedOperation::PublicMutation { .. } => return Err(CodecError::UnsupportedTag),
    })?;
    for value in scope {
        writer.raw(value, 8_192)?;
    }
    writer.raw(request.request.as_bytes(), 256)?;
    writer.u16(request.intent.version())?;
    writer.write(request.intent.digest())?;
    writer.u64(request.stamp.hlc().physical().get())?;
    writer.u16(request.stamp.hlc().logical())?;
    writer.u64(request.stamp.wall().get())?;
    let items = match &request.operation {
        PreparedOperation::Batch(items) => items,
        PreparedOperation::InstallPolicy { command, value } => {
            return policy::write(command, value, writer, limits);
        }
        PreparedOperation::ClaimPhase(phase) => return claim::write(phase, writer, limits),
        PreparedOperation::PublicMutation { .. } => return Err(CodecError::UnsupportedTag),
    };
    if items.is_empty() || items.len() > limits.admission.batch_items() {
        return Err(CodecError::ItemLimit);
    }
    writer.u32(u32::try_from(items.len()).map_err(|_| CodecError::LengthOverflow)?)?;
    for item in items {
        keys::charge(
            &mut derived,
            keys::state_length(
                scope_length,
                item.partition.as_bytes().len(),
                item.command.id.as_bytes().len(),
            )?,
            limits.admission.decision_bytes(),
        )?;
        writer.raw(item.partition.as_bytes(), 65_535)?;
        writer.raw(item.command.id.as_bytes(), 65_535)?;
        writer.byte(item.create_priority)?;
        writer.u64(item.command.now.get())?;
        operation(
            &item.command.operation,
            writer,
            limits.admission.inline_bytes(),
        )?;
    }
    Ok(())
}

fn bytes(value: Option<&[u8]>, writer: &mut Writer<'_>, maximum: usize) -> Result<(), CodecError> {
    writer.optional(value, |writer, value| writer.raw(value, maximum))
}
fn time(value: Option<TimestampMillis>, writer: &mut Writer<'_>) -> Result<(), CodecError> {
    writer.optional(value.as_ref(), |writer, value| writer.u64(value.get()))
}
fn token(
    value: Option<&LeaseToken>,
    writer: &mut Writer<'_>,
    maximum: usize,
) -> Result<(), CodecError> {
    writer.optional(value, |writer, value| writer.raw(value.as_bytes(), maximum))
}
fn guard(value: &LeaseGuard, writer: &mut Writer<'_>, maximum: usize) -> Result<(), CodecError> {
    writer.raw(value.token.as_bytes(), maximum)?;
    writer.u64(value.fence.get())
}
fn create(value: &Create, writer: &mut Writer<'_>, maximum: usize) -> Result<(), CodecError> {
    writer.raw(&value.workflow_type, maximum)?;
    writer.raw(&value.state, maximum)?;
    time(value.run_at, writer)?;
    bytes(value.payload.as_deref(), writer, maximum)
}
fn claim(value: &Claim, writer: &mut Writer<'_>, maximum: usize) -> Result<(), CodecError> {
    writer.raw(&value.expected_state, maximum)?;
    writer.raw(&value.worker, maximum)?;
    writer.raw(value.token.as_bytes(), maximum)?;
    writer.u64(value.lease_ms)
}
fn transition(
    value: &Transition,
    writer: &mut Writer<'_>,
    maximum: usize,
) -> Result<(), CodecError> {
    writer.raw(&value.from, maximum)?;
    writer.raw(&value.to, maximum)?;
    writer.u64(value.fence.get())?;
    token(value.lease_token.as_ref(), writer, maximum)?;
    time(value.run_at, writer)?;
    bytes(value.payload.as_deref(), writer, maximum)
}
fn terminal(value: &Terminal, writer: &mut Writer<'_>, maximum: usize) -> Result<(), CodecError> {
    guard(&value.guard, writer, maximum)?;
    bytes(value.payload.as_deref(), writer, maximum)?;
    bytes(value.value.as_deref(), writer, maximum)
}
fn retry(value: &Retry, writer: &mut Writer<'_>, maximum: usize) -> Result<(), CodecError> {
    guard(&value.guard, writer, maximum)?;
    writer.byte(match value.run_at {
        RetryTime::Explicit(_) => 0,
        RetryTime::PolicyResolved(_) => 1,
    })?;
    writer.u64(value.run_at.committed().get())?;
    bytes(value.payload.as_deref(), writer, maximum)?;
    bytes(value.error.as_deref(), writer, maximum)
}
fn retry_override(
    value: &RetryOverride,
    writer: &mut Writer<'_>,
    maximum: usize,
) -> Result<(), CodecError> {
    writer.optional(value.max_retries.as_ref(), |writer, value| {
        writer.u16(*value)
    })?;
    writer.optional(value.backoff.as_ref(), |writer, value| {
        writer.byte(match value.kind {
            BackoffKind::None => 0,
            BackoffKind::Fixed => 1,
            BackoffKind::Linear => 2,
            BackoffKind::Exponential => 3,
        })?;
        writer.u64(value.base_ms)?;
        writer.u64(value.max_ms)?;
        writer.byte(value.jitter_pct)
    })?;
    bytes(value.exhausted_to.as_deref(), writer, maximum)
}
fn policy_retry(
    value: &PolicyRetry,
    writer: &mut Writer<'_>,
    maximum: usize,
) -> Result<(), CodecError> {
    guard(&value.guard, writer, maximum)?;
    time(value.run_at, writer)?;
    retry_override(&value.policy_override, writer, maximum)?;
    writer.u32(value.jitter.get())?;
    bytes(value.payload.as_deref(), writer, maximum)?;
    bytes(value.error.as_deref(), writer, maximum)
}
fn cancel(value: &Cancel, writer: &mut Writer<'_>, maximum: usize) -> Result<(), CodecError> {
    writer.u64(value.fence.get())?;
    token(value.lease_token.as_ref(), writer, maximum)?;
    bytes(value.expected_state.as_deref(), writer, maximum)?;
    writer.optional(value.expected_version.as_ref(), |writer, value| {
        writer.u64(*value)
    })?;
    bytes(value.error.as_deref(), writer, maximum)
}

fn operation(value: &Operation, writer: &mut Writer<'_>, maximum: usize) -> Result<(), CodecError> {
    writer.byte(match value {
        Operation::Create(_) => 1,
        Operation::Claim(_) => 2,
        Operation::Reclaim(_) => 3,
        Operation::Transition(_) => 4,
        Operation::StartAndClaim(_) | Operation::StepContinue(_) => {
            return Err(CodecError::UnsupportedTag);
        }
        Operation::Complete(_) => 5,
        Operation::Fail(_) => 6,
        Operation::Retry(_) => 7,
        Operation::RetryWithPolicy(_) => 8,
        Operation::ExtendLease(_) => 9,
        Operation::Cancel(_) => 10,
    })?;
    match value {
        Operation::Create(value) => create(value, writer, maximum),
        Operation::Claim(value) | Operation::Reclaim(value) => claim(value, writer, maximum),
        Operation::Transition(value) => transition(value, writer, maximum),
        Operation::StartAndClaim(_) | Operation::StepContinue(_) => Err(CodecError::UnsupportedTag),
        Operation::Complete(value) | Operation::Fail(value) => terminal(value, writer, maximum),
        Operation::Retry(value) => retry(value, writer, maximum),
        Operation::RetryWithPolicy(value) => policy_retry(value, writer, maximum),
        Operation::ExtendLease(value) => {
            guard(&value.guard, writer, maximum)?;
            writer.u64(value.lease_ms)
        }
        Operation::Cancel(value) => cancel(value, writer, maximum),
    }
}
