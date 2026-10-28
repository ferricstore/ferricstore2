//! Admit reported preparation and incoming owners before traversal or growth.

use std::mem::size_of;

use wf_domain::lease::LeaseToken;
use wf_domain::workflow::{CommittedCommand, Operation};

use super::{LogicalMutationFrameContext as Context, LogicalMutationFrameLimits as Limits};
use crate::consensus::{PreparedBatchItem, PreparedWorkflowValueBatch, WorkflowValueItemInput};
use crate::log::CodecError;
use crate::log::value_batch::budget;

pub(super) fn admit(
    command: &CommittedCommand,
    partition: Option<&Vec<u8>>,
    context: &Context,
    preparation: (usize, usize),
    limits: Limits,
) -> Result<usize, CodecError> {
    if limits.maximum_owned == 0 || limits.maximum_work == 0 {
        return Err(CodecError::Malformed);
    }
    if context.policies.is_empty()
        || context.policies.len() > limits.codec.admission().batch_items()
    {
        return Err(CodecError::ItemLimit);
    }
    // Fixed binding/codec control cohort plus explicitly requested pointees.
    // This is a conservative named-owner sum, not a native stack/RSS theorem.
    let mut owned = preparation.1;
    for amount in [
        8_192,
        size_of::<Context>(),
        size_of::<PreparedWorkflowValueBatch>(),
        size_of::<PreparedBatchItem>(),
        size_of::<WorkflowValueItemInput>(),
        context.scope.tenant().as_bytes().len(),
        context.scope.group().as_bytes().len(),
        context.scope.recovery().as_bytes().len(),
        context.request.as_bytes().len(),
        command_owned(command)?,
        partition.map_or(17, Vec::capacity),
        partition.map_or(17, Vec::len),
    ] {
        add(&mut owned, amount, limits.maximum_owned)?;
    }
    let spine = context
        .policies
        .capacity()
        .checked_mul(size_of::<crate::consensus::WorkflowValuePolicyWitness>())
        .ok_or(CodecError::LengthOverflow)?;
    add(&mut owned, spine, limits.maximum_owned)?;
    // Count and the full declared spine capacity were admitted before traversal.
    for policy in &context.policies {
        add(
            &mut owned,
            policy.workflow_type.capacity(),
            limits.maximum_owned,
        )?;
    }
    let (_, codec_work) = budget::limits(limits.codec, false)?;
    let work = owned
        .checked_mul(32)
        .and_then(|value| value.checked_add(preparation.0))
        .and_then(|value| value.checked_add(codec_work))
        .and_then(|value| value.checked_add(budget::BASE_WORK))
        .ok_or(CodecError::LengthOverflow)?;
    if work > limits.maximum_work {
        return Err(CodecError::WorkLimit);
    }
    Ok(owned)
}

pub(super) fn add(total: &mut usize, amount: usize, maximum: usize) -> Result<(), CodecError> {
    *total = total
        .checked_add(amount)
        .ok_or(CodecError::LengthOverflow)?;
    budget::admit(*total, maximum)
}

pub(super) fn input_capacity(
    total: &mut usize,
    capacity: usize,
    maximum: usize,
) -> Result<(), CodecError> {
    let extra = capacity
        .saturating_sub(1)
        .checked_mul(size_of::<WorkflowValueItemInput>())
        .ok_or(CodecError::LengthOverflow)?;
    add(total, extra, maximum)
}

fn command_owned(command: &CommittedCommand) -> Result<usize, CodecError> {
    let capacities = match &command.operation {
        Operation::Create(value) => [
            value.workflow_type.capacity(),
            value.state.capacity(),
            optional(value.payload.as_ref()),
            0,
        ],
        Operation::Transition(value) => [
            value.from.capacity(),
            value.to.capacity(),
            optional(value.payload.as_ref()),
            token(value.lease_token.as_ref()),
        ],
        Operation::Complete(value) | Operation::Fail(value) => [
            value.guard.token.as_bytes().len(),
            optional(value.payload.as_ref()),
            optional(value.value.as_ref()),
            0,
        ],
        Operation::RetryWithPolicy(value) => [
            value.guard.token.as_bytes().len(),
            optional(value.payload.as_ref()),
            optional(value.error.as_ref()),
            optional(value.policy_override.exhausted_to.as_ref()),
        ],
        Operation::ExtendLease(value) => [value.guard.token.as_bytes().len(), 0, 0, 0],
        Operation::Cancel(value) => [
            token(value.lease_token.as_ref()),
            optional(value.expected_state.as_ref()),
            optional(value.error.as_ref()),
            0,
        ],
        _ => return Err(CodecError::UnsupportedTag),
    };
    capacities
        .into_iter()
        .try_fold(command.id.as_bytes().len(), |sum, capacity| {
            sum.checked_add(capacity).ok_or(CodecError::LengthOverflow)
        })
}

fn optional(value: Option<&Vec<u8>>) -> usize {
    value.map_or(0, Vec::capacity)
}

fn token(value: Option<&LeaseToken>) -> usize {
    value.map_or(0, |token| token.as_bytes().len())
}
