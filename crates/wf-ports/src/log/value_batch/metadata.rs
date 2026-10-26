//! Borrowed census and single owned materialization of value metadata.

use wf_domain::retention::RetentionOverride;
use wf_domain::workflow::Operation;
use wf_domain::workflow::active::{LifecycleOverrides, MaxActiveOverride};
use wf_domain::workflow::control::PreparedDigest;
use wf_types::version::TimestampMillis;

use super::budget::{self, POLICY_WORK, ValueBatchCharge};
use crate::consensus::{
    PreparedOperation, PreparedWorkflowValueBatch, WorkflowValueItemInput as Input,
    WorkflowValuePolicyWitness as Witness, WorkflowValueTargetGuard as Target,
};
use crate::log::{
    CodecError, CodecLimits, public_mutation,
    reader::{self, Reader},
    writer::Writer,
};

pub(super) fn write_policies(
    policies: &[Witness],
    writer: &mut Writer<'_>,
) -> Result<(), CodecError> {
    writer.u32(u32::try_from(policies.len()).map_err(|_| CodecError::LengthOverflow)?)?;
    for policy in policies {
        writer.raw(&policy.workflow_type, usize::MAX)?;
        writer.byte(policy.profile)?;
        writer.u64(policy.generation)?;
        writer.write(policy.digest.as_bytes())?;
    }
    Ok(())
}

pub(super) fn scan_policies(
    reader: &mut Reader<'_>,
    charge: &mut ValueBatchCharge,
) -> Result<usize, CodecError> {
    let count = usize::try_from(reader.u32()?).map_err(|_| CodecError::LengthOverflow)?;
    if count == 0 || count > reader.limits.admission().batch_items() {
        return Err(CodecError::ItemLimit);
    }
    if count > reader.remaining() / 46 {
        return Err(CodecError::ItemLimit);
    }
    charge.debit(
        count
            .checked_mul(POLICY_WORK)
            .ok_or(CodecError::LengthOverflow)?,
    )?;
    let mut previous: Option<&[u8]> = None;
    for _ in 0..count {
        let kind = reader.nonempty(reader.limits.admission().inline_bytes())?;
        if previous.is_some_and(|old| old >= kind) {
            return Err(CodecError::Malformed);
        }
        previous = Some(kind);
        if reader.byte()? > 2 {
            return Err(CodecError::UnsupportedTag);
        }
        reader.u64()?;
        reader.take(32)?;
    }
    Ok(count)
}

pub(super) fn read_policies(reader: &mut Reader<'_>) -> Result<Vec<Witness>, CodecError> {
    let count = usize::try_from(reader.u32()?).map_err(|_| CodecError::LengthOverflow)?;
    let mut policies = budget::reserve(count, reader.limits.admission().batch_items())?;
    for _ in 0..count {
        policies.push(Witness {
            workflow_type: reader::copy(
                reader.nonempty(reader.limits.admission().inline_bytes())?,
            )?,
            profile: reader.byte()?,
            generation: reader.u64()?,
            digest: PreparedDigest::new(reader.fixed()?),
        });
    }
    Ok(policies)
}

pub(super) fn write_inputs(inputs: &[Input], writer: &mut Writer<'_>) -> Result<(), CodecError> {
    writer.u32(u32::try_from(inputs.len()).map_err(|_| CodecError::LengthOverflow)?)?;
    for input in inputs {
        writer.u32(input.source_index)?;
        writer.u32(input.policy_index)?;
        match input.target {
            Target::Unspecified => writer.byte(0)?,
            Target::Absent => writer.byte(1)?,
            Target::Present { incarnation } => {
                writer.byte(2)?;
                writer.write(&incarnation.to_be_bytes())?;
            }
        }
        writer.optional(input.birth.as_ref(), |writer, birth| {
            match birth.active {
                MaxActiveOverride::Inherit => writer.byte(0)?,
                MaxActiveOverride::Infinite => writer.byte(1)?,
                MaxActiveOverride::Millis(value) => {
                    writer.byte(2)?;
                    writer.u64(value)?;
                }
            }
            writer.optional(birth.retention.ttl_ms.as_ref(), |writer, value| {
                writer.u64(*value)
            })?;
            writer.optional(
                birth.retention.history_max_events.as_ref(),
                |writer, value| writer.u32(*value),
            )?;
            writer.optional(birth.history_hot_max_events.as_ref(), |writer, value| {
                writer.u64(*value)
            })
        })?;
        writer.optional(input.terminal_ttl_ms.as_ref(), |writer, value| {
            writer.write(&value.to_be_bytes())
        })?;
        writer.optional(input.requested_at.as_ref(), |writer, value| {
            writer.u64(value.get())
        })?;
    }
    Ok(())
}

pub(super) fn read_input(reader: &mut Reader<'_>) -> Result<Input, CodecError> {
    let source_index = reader.u32()?;
    let policy_index = reader.u32()?;
    let target = match reader.byte()? {
        0 => Target::Unspecified,
        1 => Target::Absent,
        2 => Target::Present {
            incarnation: u128::from_be_bytes(reader.fixed()?),
        },
        _ => return Err(CodecError::UnsupportedTag),
    };
    let birth = reader.optional(|reader| {
        let active = match reader.byte()? {
            0 => MaxActiveOverride::Inherit,
            1 => MaxActiveOverride::Infinite,
            2 => MaxActiveOverride::Millis(reader.u64()?),
            _ => return Err(CodecError::UnsupportedTag),
        };
        Ok(LifecycleOverrides {
            active,
            retention: RetentionOverride {
                ttl_ms: reader.optional(Reader::u64)?,
                history_max_events: reader.optional(Reader::u32)?,
            },
            history_hot_max_events: reader.optional(Reader::u64)?,
        })
    })?;
    let terminal_ttl_ms = reader.optional(|reader| Ok(i64::from_be_bytes(reader.fixed()?)))?;
    let requested_at = reader.optional(|reader| Ok(TimestampMillis::new(reader.u64()?)))?;
    Ok(Input {
        source_index,
        policy_index,
        target,
        birth,
        terminal_ttl_ms,
        requested_at,
    })
}

pub(super) fn scan_inputs(
    reader: &mut Reader<'_>,
    count: usize,
    policies: usize,
) -> Result<(), CodecError> {
    if usize::try_from(reader.u32()?).map_err(|_| CodecError::LengthOverflow)? != count {
        return Err(CodecError::Malformed);
    }
    for ordinal in 0..count {
        let input = read_input(reader)?;
        if usize::try_from(input.source_index).map_err(|_| CodecError::LengthOverflow)? != ordinal
            || usize::try_from(input.policy_index).map_err(|_| CodecError::LengthOverflow)?
                >= policies
        {
            return Err(CodecError::Malformed);
        }
    }
    Ok(())
}

pub(super) fn read_inputs(reader: &mut Reader<'_>, count: usize) -> Result<Vec<Input>, CodecError> {
    if usize::try_from(reader.u32()?).map_err(|_| CodecError::LengthOverflow)? != count {
        return Err(CodecError::Malformed);
    }
    let mut inputs = budget::reserve(count, reader.limits.admission().batch_items())?;
    for _ in 0..count {
        inputs.push(read_input(reader)?);
    }
    Ok(inputs)
}

pub(super) fn check(
    batch: &PreparedWorkflowValueBatch,
    limits: CodecLimits,
) -> Result<(), CodecError> {
    batch.defaults.to_bytes()?;
    let items = batch
        .operation
        .mutation_items()
        .ok_or(CodecError::UnsupportedTag)?;
    if items.is_empty()
        || items.len() > limits.admission().batch_items()
        || batch.policies.is_empty()
        || batch.policies.len() > limits.admission().batch_items()
    {
        return Err(CodecError::ItemLimit);
    }
    if batch.inputs.len() != items.len() {
        return Err(CodecError::Malformed);
    }
    let mut previous: Option<&[u8]> = None;
    for policy in &batch.policies {
        if policy.workflow_type.len() > limits.admission().inline_bytes() {
            return Err(CodecError::ValueLimit);
        }
        if policy.workflow_type.is_empty()
            || previous.is_some_and(|old| old >= policy.workflow_type.as_slice())
        {
            return Err(CodecError::Malformed);
        }
        if policy.profile > 2 {
            return Err(CodecError::UnsupportedTag);
        }
        previous = Some(&policy.workflow_type);
    }
    for (ordinal, input) in batch.inputs.iter().enumerate() {
        if usize::try_from(input.source_index).map_err(|_| CodecError::LengthOverflow)? != ordinal
            || usize::try_from(input.policy_index).map_err(|_| CodecError::LengthOverflow)?
                >= batch.policies.len()
        {
            return Err(CodecError::Malformed);
        }
        if batch.profile != 1 && input.requested_at.is_some() {
            return Err(CodecError::Malformed);
        }
    }
    if let PreparedOperation::PublicMutation { item, evidence } = &batch.operation {
        if batch.profile != 1 || items.len() != 1 {
            return Err(CodecError::UnsupportedTag);
        }
        if item.create_priority > 2
            || (!matches!(item.command.operation, Operation::Create(_))
                && item.create_priority != 0)
        {
            return Err(CodecError::Malformed);
        }
        if batch.inputs[0].requested_at.map(TimestampMillis::get) != evidence.requested_now_ms
            || item.command.now.get()
                != evidence
                    .requested_now_ms
                    .unwrap_or(batch.stamp.hlc().physical().get())
        {
            return Err(CodecError::Malformed);
        }
        if evidence.auto_partition {
            let mut bytes = [0; 17];
            let len = public_mutation::auto_partition(item.command.id.as_bytes(), &mut bytes);
            if item.partition.as_bytes() != &bytes[..len] {
                return Err(CodecError::Malformed);
            }
        }
    } else if !matches!(batch.profile, 5..=7) {
        return Err(CodecError::UnsupportedTag);
    }
    Ok(())
}
