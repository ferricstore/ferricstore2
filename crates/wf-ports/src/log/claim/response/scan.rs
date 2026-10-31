//! Borrowed one-pass grammar validation and identity-only uniqueness rescans.

use wf_domain::workflow::MAX_EXACT_MILLIS;
use wf_domain::workflow::admission::{AdmissionLimits, MAX_INLINE_BYTES};
use wf_domain::workflow::request::RetainedStatus;
use wf_types::version::TimestampMillis;

use crate::log::{CodecError, CodecLimits, reader::Reader, writer::Writer};

pub(in crate::log) fn summary(
    response: &[u8],
    original_time: TimestampMillis,
    admission: AdmissionLimits,
) -> Result<(RetainedStatus, Option<usize>), CodecError> {
    if response.len() > admission.decision_bytes() {
        return Err(CodecError::ByteLimit);
    }
    if original_time.get() > MAX_EXACT_MILLIS {
        return Err(CodecError::Malformed);
    }
    let mut reader = Reader::new(response, CodecLimits::new(admission));
    if reader.take(4)? != b"FWCR" || reader.u16()? != 1 {
        return Err(CodecError::Malformed);
    }
    match reader.byte()? {
        1 => {
            let reason = super::read_rejection(&mut reader)?;
            reader.finish()?;
            return Ok((RetainedStatus::ClaimRejected(reason), None));
        }
        0 => {}
        _ => return Err(CodecError::Malformed),
    }
    let stop = reader.byte()?;
    let restart = reader.byte()?;
    let scanned = reader.u32()?;
    let heads = reader.u32()?;
    let count = usize::try_from(reader.u32()?).map_err(|_| CodecError::LengthOverflow)?;
    if stop > 2
        || restart != u8::from(stop != 0)
        || scanned > 16_384
        || heads > 4_096
        || count > 1_000
        || count > usize::try_from(scanned).map_err(|_| CodecError::LengthOverflow)?
        || (stop != 0 && scanned == 0)
        || (stop == 1 && count == 0)
    {
        return Err(CodecError::Malformed);
    }
    if count > admission.batch_items() {
        return Err(CodecError::ItemLimit);
    }
    if count > reader.remaining() / 92 {
        return Err(CodecError::Malformed);
    }
    let mut phase = None;
    for _ in 0..count {
        let actual = row(&mut reader, original_time)?;
        if phase.is_some_and(|first| first != actual) {
            return Err(CodecError::Malformed);
        }
        phase = Some(actual);
    }
    reader.finish()?;
    unique(
        response.get(21..).ok_or(CodecError::Malformed)?,
        count,
        admission,
    )?;
    Ok((RetainedStatus::Accepted, Some(count)))
}

fn row<'a>(
    reader: &mut Reader<'a>,
    original_time: TimestampMillis,
) -> Result<(&'a [u8], &'a [u8], u64), CodecError> {
    typed(reader)?;
    typed(reader)?;
    let workflow_type = required(reader)?;
    required(reader)?;
    let worker = required(reader)?;
    let token = required(reader)?;
    let version = reader.u64()?;
    let fence = reader.u64()?;
    let deadline = reader.u64()?;
    let created = reader.u64()?;
    let updated = reader.u64()?;
    reader.u64()?; // attempts preserves the complete source u64 value.
    let priority = reader.byte()?;
    reader.take(16)?; // Exact source u128 sequence, without an invented ceiling.
    if version == 0
        || fence == 0
        || deadline <= original_time.get()
        || deadline > MAX_EXACT_MILLIS
        || created > MAX_EXACT_MILLIS
        || updated != original_time.get()
        || priority > 2
    {
        return Err(CodecError::Malformed);
    }
    canonical_token(token, worker, original_time.get(), fence)?;
    for _ in 0..3 {
        reader.optional(inline)?;
    }
    Ok((workflow_type, worker, deadline))
}

fn typed<'a>(reader: &mut Reader<'a>) -> Result<&'a [u8], CodecError> {
    let length = usize::try_from(reader.u32()?).map_err(|_| CodecError::LengthOverflow)?;
    if length == 0 || length > 65_535 {
        return Err(CodecError::Malformed);
    }
    reader.take(length)
}

fn required<'a>(reader: &mut Reader<'a>) -> Result<&'a [u8], CodecError> {
    let bytes = inline(reader)?;
    if bytes.is_empty() {
        Err(CodecError::Malformed)
    } else {
        Ok(bytes)
    }
}

fn inline<'a>(reader: &mut Reader<'a>) -> Result<&'a [u8], CodecError> {
    let length = usize::try_from(reader.u32()?).map_err(|_| CodecError::LengthOverflow)?;
    if length > MAX_INLINE_BYTES {
        return Err(CodecError::Malformed);
    }
    if length > reader.limits.admission().inline_bytes() {
        return Err(CodecError::ValueLimit);
    }
    reader.take(length)
}

fn canonical_token(token: &[u8], worker: &[u8], time: u64, fence: u64) -> Result<(), CodecError> {
    let mut writer = Writer::verify(token, usize::MAX)?;
    writer.write(worker)?;
    for mut value in [time, fence] {
        writer.byte(b':')?;
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
    }
    writer.finish_verification()
}

fn unique(rows: &[u8], count: usize, admission: AdmissionLimits) -> Result<(), CodecError> {
    let limits = CodecLimits::new(admission);
    let mut outer = Reader::new(rows, limits);
    for previous in 0..count {
        let identity = identity_and_skip(&mut outer)?;
        let mut inner = Reader::new(rows, limits);
        for _ in 0..previous {
            if identity_and_skip(&mut inner)? == identity {
                return Err(CodecError::Malformed);
            }
        }
    }
    Ok(())
}

fn identity_and_skip<'a>(reader: &mut Reader<'a>) -> Result<[&'a [u8]; 2], CodecError> {
    let identity = [reader.raw(usize::MAX)?, reader.raw(usize::MAX)?];
    // The full pass already checked these lengths, scalars and presence tags.
    for _ in 0..4 {
        reader.raw(usize::MAX)?;
    }
    reader.take(65)?;
    for _ in 0..3 {
        reader.optional(|reader| reader.raw(usize::MAX))?;
    }
    Ok(identity)
}
