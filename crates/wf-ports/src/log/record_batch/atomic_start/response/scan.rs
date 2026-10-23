//! Allocation-free original FWAS scan; accepted-only implicit physical fields.

use wf_domain::workflow::MAX_EXACT_MILLIS;
use wf_domain::workflow::admission::AdmissionLimits;
use wf_domain::workflow::indexed::IndexRejection;
use wf_domain::workflow::request::RetainedStatus;
use wf_types::version::{LogIdentity, TimestampMillis};

use crate::log::{CodecError, CodecLimits, reader::Reader, writer::Writer};

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
    if reader.take(4)? != b"FWAS" {
        return Err(CodecError::Malformed);
    }
    if reader.u16()? != 1 {
        return Err(CodecError::UnsupportedVersion);
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
            let body = response.get(6..).ok_or(CodecError::Malformed)?;
            let (reason, length) = if body.starts_with(&[1, 7, 0]) {
                (IndexRejection::DirectRunningFifoEntry, 3)
            } else {
                super::super::super::response::rejection(body)?
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
    reader.nonempty(65_535)?; // Identity limits are independent of inline fields.
    let id = reader.nonempty(65_535)?;
    let workflow_type = reader.field()?;
    let logical = reader.field()?;
    let worker = reader.field()?;
    let token = reader.field()?;
    let version = reader.u64()?;
    let fence = reader.u64()?;
    let deadline = reader.u64()?;
    let created = reader.u64()?;
    let updated = reader.u64()?;
    let attempts = reader.u64()?;
    let priority = reader.byte()?;
    let sequence = u128::from_be_bytes(reader.fixed()?);
    if workflow_type.is_empty()
        || logical.is_empty()
        || worker.is_empty()
        || token.is_empty()
        || workflow_type == b"__ferricstore_schedule"
        || id.starts_with(b"__ferricstore_schedule__:")
        || matches!(
            logical,
            b"running" | b"completed" | b"failed" | b"cancelled"
        )
        || version != 1
        || fence != 1
        || deadline <= time.get()
        || deadline > MAX_EXACT_MILLIS
        || created != time.get()
        || updated != time.get()
        || attempts != 1
        || priority > 2
        || sequence != u128::from(position.index().get()) << 32
    {
        return Err(CodecError::Malformed);
    }
    canonical_token(token, worker, time)?;
    reader.optional(Reader::field)?;
    for _ in 0..2 {
        if reader.optional(Reader::field)?.is_some() {
            return Err(CodecError::Malformed);
        }
    }
    Ok(())
}

pub(super) fn canonical_token(
    token: &[u8],
    worker: &[u8],
    time: TimestampMillis,
) -> Result<(), CodecError> {
    let mut writer = Writer::verify(token, usize::MAX)?;
    writer.write(worker)?;
    writer.byte(b':')?;
    let mut value = time.get();
    let mut digits = [0_u8; 20];
    let mut start = digits.len();
    loop {
        start -= 1;
        digits[start] = b'0' + u8::try_from(value % 10).map_err(|_| CodecError::Malformed)?;
        value /= 10;
        if value == 0 {
            break;
        }
    }
    writer.write(&digits[start..])?;
    writer.write(b":1")?;
    writer.finish_verification()
}
