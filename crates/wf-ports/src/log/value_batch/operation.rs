//! Existing owned operation bytes, with one explicit contained reducer mode.

use wf_domain::workflow::caller::PublicMutationEvidence;
use wf_domain::workflow::{CommittedCommand, Operation, StartAndClaim};
use wf_types::identity::{PartitionId, RecordId};

use super::budget::{self, ITEM_WORK, ValueBatchCharge};
use crate::consensus::{PreparedBatchItem, PreparedOperation};
use crate::log::{
    CodecError, CodecLimits, common, decode, encode, keys,
    reader::{self, Reader},
    scan,
    writer::Writer,
};

pub(super) fn write(
    operation: &PreparedOperation,
    mode: u8,
    writer: &mut Writer<'_>,
    limits: CodecLimits,
) -> Result<(), CodecError> {
    let items = match (mode, operation) {
        (1, PreparedOperation::PublicMutation { item, evidence }) => {
            writer.byte(
                u8::from(evidence.auto_partition)
                    | (u8::from(evidence.requested_now_ms.is_some()) << 1)
                    | (u8::from(evidence.retry_override_present) << 2),
            )?;
            if let Some(now) = evidence.requested_now_ms {
                writer.u64(now)?;
            }
            std::slice::from_ref(item.as_ref())
        }
        (5..=7, PreparedOperation::Batch(items)) => items.as_slice(),
        _ => return Err(CodecError::UnsupportedTag),
    };
    if items.is_empty()
        || items.len() > limits.admission().batch_items()
        || (matches!(mode, 1 | 7) && items.len() != 1)
    {
        return Err(CodecError::ItemLimit);
    }
    writer.u32(u32::try_from(items.len()).map_err(|_| CodecError::LengthOverflow)?)?;
    for item in items {
        writer.raw(item.partition.as_bytes(), 65_535)?;
        writer.raw(item.command.id.as_bytes(), 65_535)?;
        writer.byte(item.create_priority)?;
        writer.u64(item.command.now.get())?;
        if mode == 7 {
            let Operation::StartAndClaim(start) = &item.command.operation else {
                return Err(CodecError::UnsupportedTag);
            };
            writer.byte(11)?;
            for bytes in [&start.workflow_type, &start.state, &start.worker] {
                writer.raw(bytes, limits.admission().inline_bytes())?;
            }
            writer.u64(start.lease_ms)?;
            writer.optional(start.payload.as_deref(), |writer, bytes| {
                writer.raw(bytes, limits.admission().inline_bytes())
            })?;
        } else {
            if matches!(
                item.command.operation,
                Operation::Claim(_)
                    | Operation::Reclaim(_)
                    | Operation::StartAndClaim(_)
                    | Operation::StepContinue(_)
            ) || (mode == 1 && matches!(item.command.operation, Operation::Retry(_)))
            {
                return Err(CodecError::UnsupportedTag);
            }
            encode::operation(
                &item.command.operation,
                writer,
                limits.admission().inline_bytes(),
            )?;
        }
    }
    Ok(())
}

pub(super) fn scan(
    reader: &mut Reader<'_>,
    mode: u8,
    scope_length: usize,
    charge: &mut ValueBatchCharge,
) -> Result<(usize, usize), CodecError> {
    if mode == 1 {
        let flags = reader.byte()?;
        if flags & !7 != 0 {
            return Err(CodecError::UnsupportedTag);
        }
        if flags & 2 != 0 {
            reader.u64()?;
        }
    }
    let count = common::count(reader)?;
    if matches!(mode, 1 | 7) && count != 1 {
        return Err(CodecError::ItemLimit);
    }
    charge.debit(
        count
            .checked_mul(ITEM_WORK)
            .ok_or(CodecError::LengthOverflow)?,
    )?;
    let mut derived = scope_length;
    for _ in 0..count {
        let item = common::item(reader)?;
        keys::charge(
            &mut derived,
            keys::state_length(scope_length, item.partition.len(), item.id.len())?,
            reader.limits.admission().decision_bytes(),
        )?;
        if mode == 7 {
            if item.tag != 11 {
                return Err(CodecError::UnsupportedTag);
            }
            for _ in 0..3 {
                reader.field()?;
            }
            reader.u64()?;
            reader.optional(Reader::field)?;
        } else {
            if !matches!(item.tag, 1 | 4 | 5..=7 | 8 | 9 | 10) || (mode == 1 && item.tag == 7) {
                return Err(CodecError::UnsupportedTag);
            }
            scan::operation(reader, item.tag, false)?;
        }
        if item.priority > 2 || (item.tag != 1 && item.tag != 11 && item.priority != 0) {
            return Err(CodecError::Malformed);
        }
    }
    Ok((count, derived))
}

pub(super) fn read(
    reader: &mut Reader<'_>,
    mode: u8,
    limits: CodecLimits,
) -> Result<PreparedOperation, CodecError> {
    let evidence = if mode == 1 {
        let flags = reader.byte()?;
        if flags & !7 != 0 {
            return Err(CodecError::UnsupportedTag);
        }
        Some(PublicMutationEvidence {
            requested_now_ms: if flags & 2 != 0 {
                Some(reader.u64()?)
            } else {
                None
            },
            auto_partition: flags & 1 != 0,
            retry_override_present: flags & 4 != 0,
        })
    } else {
        None
    };
    let count = common::count(reader)?;
    if let Some(evidence) = evidence {
        if count != 1 {
            return Err(CodecError::ItemLimit);
        }
        return Ok(PreparedOperation::PublicMutation {
            item: Box::new(read_item(reader, mode)?),
            evidence,
        });
    }
    let mut items = budget::reserve(count, limits.admission().batch_items())?;
    for _ in 0..count {
        items.push(read_item(reader, mode)?);
    }
    Ok(PreparedOperation::Batch(items))
}

pub(super) fn read_item(
    reader: &mut Reader<'_>,
    mode: u8,
) -> Result<PreparedBatchItem, CodecError> {
    let item = common::item(reader)?;
    let operation = if mode == 7 {
        if item.tag != 11 {
            return Err(CodecError::UnsupportedTag);
        }
        Operation::StartAndClaim(StartAndClaim {
            workflow_type: reader.owned()?,
            state: reader.owned()?,
            worker: reader.owned()?,
            lease_ms: reader.u64()?,
            payload: reader.optional(Reader::owned)?,
        })
    } else {
        decode::operation(reader, item.tag, false)?
    };
    Ok(PreparedBatchItem {
        partition: PartitionId::new(reader::copy(item.partition)?)
            .map_err(|_| CodecError::Malformed)?,
        command: CommittedCommand {
            id: RecordId::new(reader::copy(item.id)?).map_err(|_| CodecError::Malformed)?,
            now: item.now,
            operation,
        },
        create_priority: item.priority,
    })
}
