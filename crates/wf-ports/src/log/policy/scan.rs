//! Complete borrowed shape, raw-byte, count and normalized semantic preflight.

use wf_domain::policy::installation::{
    MAX_ENCODED_POLICY_BYTES, MAX_POLICY_GENERATION, MAX_POLICY_SNAPSHOT_BYTES,
};
use wf_domain::policy::{Backoff, BackoffKind, RetryOverride};
use wf_domain::retention::{MAX_HISTORY_EVENTS, MAX_RETENTION_MILLIS, RetentionOverride};

use super::super::{CodecError, reader::Reader};

fn raw<'a>(reader: &mut Reader<'a>, total: &mut usize) -> Result<&'a [u8], CodecError> {
    let value = reader.raw(MAX_POLICY_SNAPSHOT_BYTES)?;
    *total = total
        .checked_add(value.len())
        .ok_or(CodecError::LengthOverflow)?;
    if *total > MAX_POLICY_SNAPSHOT_BYTES {
        Err(CodecError::ByteLimit)
    } else {
        Ok(value)
    }
}
fn field(reader: &mut Reader<'_>, total: &mut usize) -> Result<(), CodecError> {
    raw(reader, total).map(|_| ())
}
fn patch(
    reader: &mut Reader<'_>,
    read: impl FnOnce(&mut Reader<'_>) -> Result<(), CodecError>,
) -> Result<(), CodecError> {
    match reader.byte()? {
        0 | 2 => Ok(()),
        1 => read(reader),
        _ => Err(CodecError::UnsupportedTag),
    }
}
fn kind(reader: &mut Reader<'_>) -> Result<BackoffKind, CodecError> {
    match reader.byte()? {
        0 => Ok(BackoffKind::None),
        1 => Ok(BackoffKind::Fixed),
        2 => Ok(BackoffKind::Linear),
        3 => Ok(BackoffKind::Exponential),
        _ => Err(CodecError::UnsupportedTag),
    }
}
fn mode(reader: &mut Reader<'_>) -> Result<(), CodecError> {
    if reader.byte()? > 1 {
        Err(CodecError::UnsupportedTag)
    } else {
        Ok(())
    }
}
fn version(reader: &mut Reader<'_>, total: &mut usize, normalized: bool) -> Result<(), CodecError> {
    match reader.byte()? {
        0 => {
            let bytes = raw(reader, total)?;
            if normalized && bytes.is_empty() {
                Err(CodecError::Malformed)
            } else {
                Ok(())
            }
        }
        1 => reader.u64().map(|_| ()),
        _ => Err(CodecError::UnsupportedTag),
    }
}
fn retention(reader: &mut Reader<'_>, normalized: bool) -> Result<(), CodecError> {
    let value = RetentionOverride {
        ttl_ms: reader.optional(Reader::u64)?,
        history_max_events: reader.optional(Reader::u32)?,
    };
    if normalized {
        value
            .validate(MAX_HISTORY_EVENTS)
            .map_err(|_| CodecError::Malformed)
    } else {
        Ok(())
    }
}
fn retry_patch(reader: &mut Reader<'_>, total: &mut usize) -> Result<(), CodecError> {
    reader.optional(Reader::u16)?;
    reader.optional(|reader| {
        reader.optional(kind)?;
        reader.optional(Reader::u64)?;
        reader.optional(Reader::u64)?;
        reader.optional(Reader::byte).map(|_| ())
    })?;
    reader.optional(|reader| field(reader, total)).map(|_| ())
}
fn retry(reader: &mut Reader<'_>, total: &mut usize) -> Result<(), CodecError> {
    let max_retries = reader.optional(Reader::u16)?;
    let backoff = reader.optional(|reader| {
        Ok(Backoff {
            kind: kind(reader)?,
            base_ms: reader.u64()?,
            max_ms: reader.u64()?,
            jitter_pct: reader.byte()?,
        })
    })?;
    let target = reader.optional(|reader| raw(reader, total))?;
    if target.is_some_and(|target| target.is_empty() || target == b"running") {
        return Err(CodecError::Malformed);
    }
    RetryOverride {
        max_retries,
        backoff,
        exhausted_to: None,
    }
    .validate()
    .map_err(|_| CodecError::Malformed)
}
fn count(reader: &mut Reader<'_>, minimum: usize) -> Result<usize, CodecError> {
    let count = usize::try_from(reader.u32()?).map_err(|_| CodecError::LengthOverflow)?;
    if count > reader.limits.policy_limits().max_states() {
        return Err(CodecError::ItemLimit);
    }
    if count > reader.remaining() / minimum {
        return Err(CodecError::Malformed);
    }
    Ok(count)
}
fn ordered<'a>(
    reader: &mut Reader<'a>,
    total: &mut usize,
    previous: &mut Option<&'a [u8]>,
) -> Result<&'a [u8], CodecError> {
    let name = raw(reader, total)?;
    if previous.is_some_and(|previous| previous >= name) {
        return Err(CodecError::Malformed);
    }
    *previous = Some(name);
    Ok(name)
}

pub(super) fn command(reader: &mut Reader<'_>) -> Result<(), CodecError> {
    let mut total = 0;
    field(reader, &mut total)?;
    reader.optional(Reader::u64)?;
    mode(reader)?;
    patch(reader, |reader| version(reader, &mut total, false))?;
    patch(reader, |reader| reader.u64().map(|_| ()))?;
    patch(reader, |reader| retry_patch(reader, &mut total))?;
    patch(reader, |reader| retention(reader, false))?;
    let count = count(reader, 5)?;
    let mut previous = None;
    for _ in 0..count {
        ordered(reader, &mut total, &mut previous)?;
        match reader.byte()? {
            0 => {}
            1 => {
                patch(reader, mode)?;
                patch(reader, |reader| retry_patch(reader, &mut total))?;
                patch(reader, |reader| retention(reader, false))?;
            }
            _ => return Err(CodecError::UnsupportedTag),
        }
    }
    Ok(())
}

fn magic(reader: &mut Reader<'_>, expected: [u8; 4]) -> Result<(), CodecError> {
    if reader.fixed::<4>()? != expected {
        return Err(CodecError::Malformed);
    }
    if reader.u16()? != 1 {
        return Err(CodecError::UnsupportedVersion);
    }
    Ok(())
}

pub(super) fn value(reader: &mut Reader<'_>) -> Result<(), CodecError> {
    let bytes = reader.raw(MAX_ENCODED_POLICY_BYTES)?;
    let mut envelope = Reader::new(bytes, reader.limits);
    magic(&mut envelope, *b"FWPV")?;
    let generation = envelope.u64()?;
    if generation == 0 || generation > MAX_POLICY_GENERATION {
        return Err(CodecError::Malformed);
    }
    let bytes = envelope.raw(MAX_POLICY_SNAPSHOT_BYTES)?;
    envelope.finish()?;
    let mut snapshot = Reader::new(bytes, reader.limits);
    magic(&mut snapshot, *b"FWPS")?;
    let mut total = 0;
    if raw(&mut snapshot, &mut total)?.is_empty() {
        return Err(CodecError::Malformed);
    }
    snapshot.optional(|reader| version(reader, &mut total, true))?;
    if snapshot
        .optional(Reader::u64)?
        .is_some_and(|value| value == 0 || value > MAX_RETENTION_MILLIS)
    {
        return Err(CodecError::Malformed);
    }
    retry(&mut snapshot, &mut total)?;
    retention(&mut snapshot, true)?;
    let count = count(&mut snapshot, 10)?;
    let mut previous = None;
    for _ in 0..count {
        let name = ordered(&mut snapshot, &mut total, &mut previous)?;
        if name.is_empty() || name == b"running" {
            return Err(CodecError::Malformed);
        }
        snapshot.optional(mode)?;
        retry(&mut snapshot, &mut total)?;
        retention(&mut snapshot, true)?;
    }
    snapshot.finish()
}
