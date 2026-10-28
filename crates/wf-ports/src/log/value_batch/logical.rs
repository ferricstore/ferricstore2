//! Bind an owned logical caller to one explicit stored-value frame.

mod model;
mod ownership;

pub use model::{LogicalMutationFrameContext, LogicalMutationFrameLimits};

use wf_domain::workflow::caller::PreparedLogicalMutation;
use wf_domain::workflow::request::ClientIntent;
use wf_types::identity::PartitionId;

use super::{budget, encode_profile_with_intent};
use crate::consensus::{PreparedBatchItem, PreparedOperation, PreparedWorkflowValueBatch};
use crate::log::{CodecError, PreparedBytes, public_mutation, reader};

/// Encode one logical caller as stored intent9, retaining its original evidence.
///
/// Context values are explicit observations, without authorization, placement,
/// submission or persistence authority. The owned caller is consumed.
///
/// # Errors
/// Rejects contradictory metadata, invalid positive caps, resource or arithmetic
/// limits and fallible reservation failure. This performs no I/O or submission.
pub fn encode_logical_mutation_frame(
    caller: PreparedLogicalMutation,
    context: LogicalMutationFrameContext,
    limits: LogicalMutationFrameLimits,
) -> Result<(PreparedBytes, ClientIntent), CodecError> {
    let preparation = (caller.work_units(), caller.temporary_bytes());
    let (partition, command, create_priority, evidence) = caller.into_stored_parts();
    let mut owned = ownership::admit(&command, partition.as_ref(), &context, preparation, limits)?;
    let partition = bind_partition(partition, command.id.as_bytes(), &mut owned, limits)?;
    let mut inputs = budget::reserve(1, limits.codec.admission().batch_items())?;
    ownership::input_capacity(&mut owned, inputs.capacity(), limits.maximum_owned)?;
    inputs.push(context.input);
    let batch = PreparedWorkflowValueBatch {
        scope: context.scope,
        request: context.request,
        stamp: context.stamp,
        profile: 1,
        policy_references_captured: context.policy_references_captured,
        defaults: context.defaults,
        policies: context.policies,
        operation: PreparedOperation::PublicMutation {
            item: Box::new(PreparedBatchItem {
                partition,
                command,
                create_priority,
            }),
            evidence,
        },
        inputs,
    };
    let remaining = limits
        .maximum_owned
        .checked_sub(owned)
        .ok_or(CodecError::OwnedLimit)?;
    let (frame, intent) = encode_profile_with_intent(&batch, limits.codec, 9, Some(remaining))?;
    budget::admit(frame.bytes.capacity(), remaining)?;
    Ok((frame, intent))
}

fn bind_partition(
    partition: Option<Vec<u8>>,
    id: &[u8],
    owned: &mut usize,
    limits: LogicalMutationFrameLimits,
) -> Result<PartitionId, CodecError> {
    let bytes = if let Some(bytes) = partition {
        bytes
    } else {
        let mut buffer = [0; 17];
        let length = public_mutation::auto_partition(id, &mut buffer);
        let bytes = reader::copy(&buffer[..length])?;
        // The prospective census already admitted the maximum 17-byte request.
        ownership::add(
            owned,
            bytes.capacity().saturating_sub(17),
            limits.maximum_owned,
        )?;
        bytes
    };
    PartitionId::new(bytes).map_err(|_| CodecError::Malformed)
}
