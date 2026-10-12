//! Collision-free private scope/state keys with complete preallocation bounds.

use super::{CodecError, writer::Writer};

fn prefixed_length(parts: &[&[u8]], overhead: usize) -> Result<usize, CodecError> {
    parts.iter().try_fold(overhead, |length, part| {
        length
            .checked_add(4)
            .and_then(|sum| sum.checked_add(part.len()))
            .ok_or(CodecError::LengthOverflow)
    })
}

pub(super) fn scope_length(parts: [&[u8]; 3]) -> Result<usize, CodecError> {
    let length = prefixed_length(&parts, 6)?;
    if length > 8_192 {
        Err(CodecError::ScopeLimit)
    } else {
        Ok(length)
    }
}

pub(super) fn state_length(scope: usize, partition: usize, id: usize) -> Result<usize, CodecError> {
    let length = scope
        .checked_add(partition)
        .and_then(|sum| sum.checked_add(id))
        .and_then(|sum| sum.checked_add(18))
        .ok_or(CodecError::LengthOverflow)?;
    if length > 65_535 {
        Err(CodecError::KeyLimit)
    } else {
        Ok(length)
    }
}

pub(super) fn charge(total: &mut usize, length: usize, maximum: usize) -> Result<(), CodecError> {
    *total = total
        .checked_add(length)
        .ok_or(CodecError::LengthOverflow)?;
    if *total > maximum {
        Err(CodecError::ByteLimit)
    } else {
        Ok(())
    }
}

pub(super) fn scope(parts: [&[u8]; 3]) -> Result<Vec<u8>, CodecError> {
    let mut writer = Writer::allocate(scope_length(parts)?, 8_192)?;
    writer.write(b"FWSC")?;
    writer.u16(1)?;
    for part in parts {
        writer.raw(part, 8_192)?;
    }
    writer.finish()
}

pub(super) fn state(scope: &[u8], partition: &[u8], id: &[u8]) -> Result<Vec<u8>, CodecError> {
    let mut writer = Writer::allocate(
        state_length(scope.len(), partition.len(), id.len())?,
        65_535,
    )?;
    writer.write(b"FWSK")?;
    writer.u16(1)?;
    for part in [scope, partition, id] {
        writer.raw(part, 65_535)?;
    }
    writer.finish()
}
