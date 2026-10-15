//! Closed physical keys with opaque lexical ordering and bounded decoded owners.

use wf_domain::claim::{DueOrder, LaneKey};
use wf_types::version::TimestampMillis;

use super::value::{StateValueError, add, allocate, read_u8, start};

mod ordered;
mod point;

pub use ordered::{
    decode_due_key, decode_fifo_key, decode_history_key, encode_due_key, encode_due_prefix,
    encode_fifo_key, encode_fifo_prefix, encode_history_key, encode_history_prefix,
};
pub use point::{encode_control_key, encode_outcome_key, encode_record_key};

/// One independently owned decoded key; namespace checks convey no authority.
///
/// The single byte buffer contains only its four due/FIFO fields or history ID.
/// Getters borrow this owner; no slice points into its original encoded donor.
#[derive(Debug, Eq, PartialEq)]
pub struct DecodedOrderedKey {
    family: u8,
    rank: u8,
    number: u128,
    bytes: Vec<u8>,
    ranges: [(usize, usize); 4],
}

impl DecodedOrderedKey {
    /// Borrows the complete semantic due tuple, only for a due key.
    #[must_use]
    pub fn due(&self) -> Option<DueOrder<'_>> {
        if self.family != 3 {
            return None;
        }
        Some(DueOrder {
            priority: 2_u8.checked_sub(self.rank)?,
            due: TimestampMillis::new(u64::try_from(self.number).ok()?),
            id: self.part(0),
            partition_key: self.part(1),
            state: self.part(2),
            workflow_type: self.part(3),
        })
    }

    /// Borrows the exact lane fields, only for a FIFO key.
    #[must_use]
    pub fn lane(&self) -> Option<LaneKey<'_>> {
        (self.family == 4).then(|| LaneKey {
            workflow_type: self.part(0),
            state: self.part(1),
            partition_key: self.part(2),
        })
    }

    /// Borrows FIFO score, full sequence and original member ID.
    #[must_use]
    pub fn member(&self) -> Option<(i8, u128, &[u8])> {
        if self.family != 4 {
            return None;
        }
        let score = match self.rank {
            0 => -1,
            1 => 0,
            _ => return None,
        };
        Some((score, self.number, self.part(3)))
    }

    /// Borrows numeric ordering milliseconds and the unchanged original ID.
    #[must_use]
    pub fn history_position(&self) -> Option<(u64, &str)> {
        if self.family != 5 {
            return None;
        }
        Some((
            u64::try_from(self.number).ok()?,
            std::str::from_utf8(self.part(0)).ok()?,
        ))
    }

    /// Returns logical metadata128 plus the actual copied component bytes.
    #[must_use]
    pub fn logical_bytes(&self) -> usize {
        128 + self.bytes.len()
    }

    fn part(&self, index: usize) -> &[u8] {
        let (start, length) = self.ranges[index];
        &self.bytes[start..start + length]
    }
}

pub(super) fn key_size(fields: &[&[u8]], scalar_bytes: usize) -> Result<usize, StateValueError> {
    let mut length = 7;
    add(&mut length, scalar_bytes)?;
    for field in fields {
        for byte in *field {
            add(&mut length, if *byte == 0 { 2 } else { 1 })?;
        }
        add(&mut length, 2)?;
    }
    Ok(length)
}

pub(super) fn key_start(
    family: u8,
    length: usize,
    maximum: usize,
) -> Result<Vec<u8>, StateValueError> {
    if family > 5 {
        return Err(StateValueError::UnsupportedTag);
    }
    if length < 7 {
        return Err(StateValueError::InvalidValue);
    }
    let mut output = allocate(length, maximum, *b"FWPK")?;
    output.push(family);
    Ok(output)
}

pub(super) fn put_component(output: &mut Vec<u8>, bytes: &[u8]) {
    for byte in bytes {
        output.push(*byte);
        if *byte == 0 {
            output.push(255);
        }
    }
    output.extend_from_slice(&[0, 0]);
}

pub(super) fn encode_components(
    family: u8,
    fields: &[&[u8]],
    maximum: usize,
) -> Result<Vec<u8>, StateValueError> {
    if maximum == 0 {
        return Err(StateValueError::InvalidValue);
    }
    let mut output = key_start(family, key_size(fields, 0)?, maximum)?;
    for field in fields {
        put_component(&mut output, field);
    }
    Ok(output)
}

pub(super) fn header(
    bytes: &[u8],
    family: u8,
    wire_cap: usize,
    decoded_cap: usize,
) -> Result<usize, StateValueError> {
    if wire_cap == 0 || decoded_cap == 0 {
        return Err(StateValueError::InvalidValue);
    }
    let mut offset = start(bytes, *b"FWPK", wire_cap)?;
    if read_u8(bytes, &mut offset)? != family {
        return Err(StateValueError::UnsupportedTag);
    }
    Ok(offset)
}

fn scan_component(
    bytes: &[u8],
    offset: &mut usize,
) -> Result<((usize, usize), usize), StateValueError> {
    let start = *offset;
    let mut length = 0;
    loop {
        let end = *offset;
        if read_u8(bytes, offset)? == 0 {
            match read_u8(bytes, offset)? {
                0 => return Ok(((start, end), length)),
                255 => (),
                _ => return Err(StateValueError::UnsupportedTag),
            }
        }
        add(&mut length, 1)?;
    }
}

pub(super) fn components<const N: usize>(
    bytes: &[u8],
    offset: &mut usize,
) -> Result<([(usize, usize); N], usize), StateValueError> {
    let mut ranges = [(0, 0); N];
    let mut total = 0;
    for range in &mut ranges {
        let (found, length) = scan_component(bytes, offset)?;
        *range = found;
        add(&mut total, length)?;
    }
    Ok((ranges, total))
}

fn component_equals(bytes: &[u8], range: (usize, usize), expected: &[u8]) -> bool {
    let mut encoded = bytes[range.0..range.1].iter();
    let mut original = expected.iter();
    while let Some(byte) = encoded.next() {
        if *byte == 0 && encoded.next() != Some(&255) {
            return false;
        }
        if original.next() != Some(byte) {
            return false;
        }
    }
    original.next().is_none()
}

pub(super) fn expected(
    bytes: &[u8],
    ranges: &[(usize, usize)],
    fields: &[&[u8]],
) -> Result<(), StateValueError> {
    if ranges.len() != fields.len()
        || ranges
            .iter()
            .zip(fields)
            .any(|(range, field)| !component_equals(bytes, *range, field))
    {
        return Err(StateValueError::InvalidValue);
    }
    Ok(())
}

pub(super) fn own(
    family: u8,
    rank: u8,
    number: u128,
    bytes: &[u8],
    parts: &[(usize, usize)],
    length: usize,
    maximum: usize,
) -> Result<DecodedOrderedKey, StateValueError> {
    let logical = length
        .checked_add(128)
        .ok_or(StateValueError::LengthOverflow)?;
    if maximum == 0 || parts.len() > 4 {
        return Err(StateValueError::InvalidValue);
    }
    if logical > maximum {
        return Err(StateValueError::Limit {
            bytes: logical,
            maximum,
        });
    }
    let mut output = Vec::new();
    output
        .try_reserve_exact(length)
        .map_err(|_| StateValueError::Allocation)?;
    let mut ranges = [(0, 0); 4];
    for (index, part) in parts.iter().enumerate() {
        let start = output.len();
        let mut encoded = bytes[part.0..part.1].iter();
        while let Some(byte) = encoded.next() {
            if *byte == 0 && encoded.next() != Some(&255) {
                return Err(StateValueError::Malformed);
            }
            output.push(*byte);
        }
        ranges[index] = (start, output.len() - start);
    }
    if output.len() != length {
        return Err(StateValueError::Malformed);
    }
    Ok(DecodedOrderedKey {
        family,
        rank,
        number,
        bytes: output,
        ranges,
    })
}
