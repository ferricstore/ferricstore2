//! Allocation-free complete FWST1 scan bound to full original log and time.

use wf_domain::workflow::MAX_EXACT_MILLIS;
use wf_domain::workflow::admission::AdmissionLimits;
use wf_domain::workflow::indexed::IndexRejection;
use wf_domain::workflow::request::RetainedStatus;
use wf_types::version::{LogIdentity, TimestampMillis};

use crate::log::{CodecError, CodecLimits, reader::Reader};

pub(super) fn summary(
    response: &[u8],
    position: LogIdentity,
    time: TimestampMillis,
    limits: AdmissionLimits,
) -> Result<(RetainedStatus, Option<usize>), CodecError> {
    if response.len() > limits.decision_bytes() {
        return Err(CodecError::ByteLimit);
    }
    if time.get() > MAX_EXACT_MILLIS {
        return Err(CodecError::Malformed);
    }
    let mut reader = Reader::new(response, CodecLimits::new(limits));
    if reader.take(4)? != b"FWST" {
        return Err(CodecError::Malformed);
    }
    if reader.u16()? != 1 {
        return Err(CodecError::UnsupportedVersion);
    }
    for expected in [
        position.term(),
        position.leader_node(),
        position.index().get(),
        time.get(),
    ] {
        if reader.u64()? != expected {
            return Err(CodecError::Malformed);
        }
    }
    let result = match reader.byte()? {
        0 => {
            if limits.inline_bytes() < b"running".len() {
                return Err(CodecError::ValueLimit);
            }
            record(&mut reader, position, time)?;
            (RetainedStatus::Accepted, Some(1))
        }
        1 => {
            let body = response.get(38..).ok_or(CodecError::Malformed)?;
            let (reason, length) = if body.starts_with(&[1, 7, 0]) {
                (IndexRejection::DirectRunningFifoEntry, 3)
            } else {
                crate::log::record_batch::response::rejection(body)?
            };
            reader.take(length.checked_sub(1).ok_or(CodecError::Malformed)?)?;
            (RetainedStatus::IndexedRejected(reason), None)
        }
        _ => return Err(CodecError::UnsupportedTag),
    };
    reader.finish()?;
    Ok(result)
}

fn record(
    reader: &mut Reader<'_>,
    position: LogIdentity,
    time: TimestampMillis,
) -> Result<(), CodecError> {
    reader.nonempty(65_535)?;
    reader.nonempty(65_535)?;
    let maximum = reader.limits.admission.inline_bytes();
    reader.nonempty(maximum)?;
    let logical = reader.nonempty(maximum)?;
    let worker = reader.nonempty(maximum)?;
    let token = reader.nonempty(maximum)?;
    let version = reader.u64()?;
    let fence = reader.u64()?;
    let deadline = reader.u64()?;
    let created = reader.u64()?;
    let updated = reader.u64()?;
    reader.u64()?; // Attempts preserve valid restored zero and every u64 value.
    let priority = reader.byte()?;
    let sequence = u128::from_be_bytes(reader.fixed()?);
    let maximum_sequence = (u128::from(position.index().get()) << 32) | u128::from(u32::MAX);
    if matches!(
        logical,
        b"running" | b"completed" | b"failed" | b"cancelled"
    ) || version < 2
        || fence == 0
        || deadline <= time.get()
        || deadline > MAX_EXACT_MILLIS
        || created > MAX_EXACT_MILLIS
        || updated != time.get()
        || priority > 2
        || sequence > maximum_sequence
    {
        return Err(CodecError::Malformed);
    }
    super::write::canonical_token(token, worker, time, fence)?;
    for _ in 0..3 {
        reader.optional(Reader::field)?;
    }
    Ok(())
}
