//! Block framing for the group log.
//!
//! A block is a 24-byte header followed by its payload. The header is the
//! magic `FWGL`, format version `2`, three zero bytes, the payload length
//! `u32`, the record count `u32`, the CRC32 (IEEE) of those 16 header bytes
//! and the payload, and four zero bytes. The payload is each record as a
//! `u32` length and its bytes. Integers are big-endian.
//!
//! Version 1 sealed blocks with SHA-256; the checksum only has to catch torn
//! and damaged writes, which CRC32 does at a fraction of the cost (ADR-015).
//! A block header of another version is never this writer's torn write,
//! so recovery refuses it instead of cutting it.

use super::GroupLogError;

const MAGIC: [u8; 4] = *b"FWGL";
const VERSION: u8 = 2;
const FIELDS_BYTES: usize = 16;
const CHECKSUM_BYTES: usize = 8;
const HEADER_BYTES: usize = FIELDS_BYTES + CHECKSUM_BYTES;
const RECORD_PREFIX_BYTES: usize = 4;

/// Whether records of these lengths fit in one block of `max_block_bytes`;
/// `None` when the size overflows.
pub(super) fn batch_fits(lengths: &[usize], max_block_bytes: usize) -> Option<bool> {
    let payload = payload_bytes(lengths)?;
    Some(payload.checked_add(HEADER_BYTES)? <= max_block_bytes)
}

fn payload_bytes(lengths: &[usize]) -> Option<usize> {
    lengths.iter().try_fold(0_usize, |total, length| {
        total.checked_add(length.checked_add(RECORD_PREFIX_BYTES)?)
    })
}

/// Encodes queued batches as consecutive blocks, packing whole batches into
/// each block up to `max_block_bytes`. Records are copied once, straight
/// into the output.
pub(super) fn encode(
    batches: &[Vec<Vec<u8>>],
    max_block_bytes: usize,
) -> Result<Vec<u8>, GroupLogError> {
    let mut sizes = Vec::new();
    let mut block_bytes = HEADER_BYTES;
    let mut records = 0_usize;
    for batch in batches {
        let batch_bytes = payload_bytes(&batch.iter().map(Vec::len).collect::<Vec<_>>())
            .ok_or(GroupLogError::TooLarge)?;
        let joined = block_bytes
            .checked_add(batch_bytes)
            .ok_or(GroupLogError::TooLarge)?;
        if records > 0 && joined > max_block_bytes {
            sizes.push(records);
            records = 0;
            block_bytes = HEADER_BYTES;
        }
        block_bytes = block_bytes
            .checked_add(batch_bytes)
            .ok_or(GroupLogError::TooLarge)?;
        records = records
            .checked_add(batch.len())
            .ok_or(GroupLogError::TooLarge)?;
    }
    if records > 0 {
        sizes.push(records);
    }
    let mut out = Vec::with_capacity(
        batches
            .iter()
            .flatten()
            .map(|record| record.len().saturating_add(RECORD_PREFIX_BYTES))
            .fold(0_usize, usize::saturating_add)
            .saturating_add(sizes.len().saturating_mul(HEADER_BYTES)),
    );
    let mut all = batches.iter().flatten();
    for count in sizes {
        push_block(all.by_ref().take(count), count, &mut out)?;
    }
    Ok(out)
}

fn push_block<'a>(
    records: impl Iterator<Item = &'a Vec<u8>>,
    count: usize,
    out: &mut Vec<u8>,
) -> Result<(), GroupLogError> {
    let start = out.len();
    out.resize(
        start
            .checked_add(HEADER_BYTES)
            .ok_or(GroupLogError::TooLarge)?,
        0,
    );
    for record in records {
        let length = u32::try_from(record.len()).map_err(|_| GroupLogError::TooLarge)?;
        out.extend_from_slice(&length.to_be_bytes());
        out.extend_from_slice(record);
    }
    let payload_length = out
        .len()
        .checked_sub(start)
        .and_then(|block| block.checked_sub(HEADER_BYTES))
        .ok_or(GroupLogError::TooLarge)?;
    let payload_length = u32::try_from(payload_length).map_err(|_| GroupLogError::TooLarge)?;
    let count = u32::try_from(count).map_err(|_| GroupLogError::TooLarge)?;
    let header = MAGIC
        .into_iter()
        .chain([VERSION, 0, 0, 0])
        .chain(payload_length.to_be_bytes())
        .chain(count.to_be_bytes());
    let block = out.get_mut(start..).ok_or(GroupLogError::TooLarge)?;
    for (slot, byte) in block.iter_mut().zip(header) {
        *slot = byte;
    }
    let (fields, rest) = block.split_at_mut(FIELDS_BYTES);
    let (checksum, payload) = rest.split_at_mut(CHECKSUM_BYTES);
    for (slot, byte) in checksum.iter_mut().zip(crc(fields, payload).to_be_bytes()) {
        *slot = byte;
    }
    Ok(())
}

fn crc(fields: &[u8], payload: &[u8]) -> u32 {
    let mut hasher = crc32fast::Hasher::new();
    hasher.update(fields);
    hasher.update(payload);
    hasher.finalize()
}

/// Recovers every complete block from `bytes`. Returns the records and the
/// length of the valid prefix; bytes after it are a torn tail.
///
/// # Errors
/// The offset of a damaged block that precedes a valid one, or of a whole
/// block in another format version.
pub(super) fn scan(bytes: &[u8], max_block_bytes: usize) -> Result<(Vec<Vec<u8>>, usize), usize> {
    let mut records = Vec::new();
    let mut offset = 0_usize;
    while let Some(rest) = bytes.get(offset..).filter(|rest| !rest.is_empty()) {
        let (block_records, size) = match parse(rest, max_block_bytes) {
            Parsed::Block(block_records, size) => (block_records, size),
            Parsed::Foreign => return Err(offset),
            Parsed::Invalid => {
                if later_block(rest, max_block_bytes) {
                    return Err(offset);
                }
                return Ok((records, offset));
            }
        };
        records.extend(block_records);
        offset = offset.checked_add(size).ok_or(offset)?;
    }
    Ok((records, offset))
}

/// Whether a valid block starts anywhere after the first byte of `bytes`.
fn later_block(bytes: &[u8], max_block_bytes: usize) -> bool {
    bytes
        .windows(MAGIC.len())
        .enumerate()
        .skip(1)
        .filter(|(_, window)| *window == MAGIC)
        .any(|(start, _)| {
            bytes
                .get(start..)
                .is_some_and(|rest| matches!(parse(rest, max_block_bytes), Parsed::Block(..)))
        })
}

/// One parsed position of a segment.
enum Parsed {
    /// A whole valid block: its records and encoded size.
    Block(Vec<Vec<u8>>, usize),
    /// A block header of another format version.
    Foreign,
    /// Not a whole valid block: damage or a torn tail.
    Invalid,
}

/// Parses one block at the start of `bytes`.
fn parse(bytes: &[u8], max_block_bytes: usize) -> Parsed {
    match block(bytes, max_block_bytes) {
        Ok((records, size)) => Parsed::Block(records, size),
        Err(foreign) if foreign => Parsed::Foreign,
        Err(_) => Parsed::Invalid,
    }
}

/// A valid block's records and size; `Err(true)` for a header that names
/// another version, `Err(false)` otherwise.
fn block(bytes: &[u8], max_block_bytes: usize) -> Result<(Vec<Vec<u8>>, usize), bool> {
    let (fields, rest) = bytes.split_at_checked(FIELDS_BYTES).ok_or(false)?;
    let [
        m0,
        m1,
        m2,
        m3,
        version,
        r0,
        r1,
        r2,
        l0,
        l1,
        l2,
        l3,
        c0,
        c1,
        c2,
        c3,
    ] = <[u8; FIELDS_BYTES]>::try_from(fields).map_err(|_| false)?;
    if [m0, m1, m2, m3] != MAGIC || [r0, r1, r2] != [0, 0, 0] {
        return Err(false);
    }
    let payload_length =
        usize::try_from(u32::from_be_bytes([l0, l1, l2, l3])).map_err(|_| false)?;
    if version != VERSION {
        // This writer's torn tail carries its own version (the header goes
        // out before the payload), so another version is never a tail.
        return Err(true);
    }
    let (stored, rest) = rest.split_at_checked(CHECKSUM_BYTES).ok_or(false)?;
    let size = HEADER_BYTES.checked_add(payload_length).ok_or(false)?;
    if size > max_block_bytes {
        return Err(false);
    }
    let payload = rest.get(..payload_length).ok_or(false)?;
    let mut expected = [0_u8; CHECKSUM_BYTES];
    for (slot, byte) in expected.iter_mut().zip(crc(fields, payload).to_be_bytes()) {
        *slot = byte;
    }
    if expected != *stored {
        return Err(false);
    }
    let count = usize::try_from(u32::from_be_bytes([c0, c1, c2, c3])).map_err(|_| false)?;
    let records = records(payload, count).ok_or(false)?;
    Ok((records, size))
}

fn records(mut payload: &[u8], count: usize) -> Option<Vec<Vec<u8>>> {
    let mut records = Vec::with_capacity(count.min(payload.len()));
    for _ in 0..count {
        let (prefix, rest) = payload.split_at_checked(RECORD_PREFIX_BYTES)?;
        let length = usize::try_from(u32::from_be_bytes(prefix.try_into().ok()?)).ok()?;
        let (record, rest) = rest.split_at_checked(length)?;
        records.push(record.to_vec());
        payload = rest;
    }
    payload.is_empty().then_some(records)
}
