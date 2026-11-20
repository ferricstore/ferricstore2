//! Block framing for the group log.
//!
//! A block is a 48-byte header followed by its payload. The header is the
//! magic `FWGL`, version `1`, three zero bytes, the payload length `u32`, the
//! record count `u32`, and the SHA-256 of `wf-group-log-v1`, those 16 header
//! bytes and the payload. The payload is each record as a `u32` length and its
//! bytes. Integers are big-endian.

use sha2::{Digest, Sha256};

use super::GroupLogError;

const MAGIC: [u8; 4] = *b"FWGL";
const VERSION: u8 = 1;
const DOMAIN: &[u8] = b"wf-group-log-v1";
const FIELDS_BYTES: usize = 16;
const DIGEST_BYTES: usize = 32;
const HEADER_BYTES: usize = FIELDS_BYTES + DIGEST_BYTES;
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
/// each block up to `max_block_bytes`.
pub(super) fn encode(
    batches: &[Vec<Vec<u8>>],
    max_block_bytes: usize,
) -> Result<Vec<u8>, GroupLogError> {
    let mut out = Vec::new();
    let mut block: Vec<&[u8]> = Vec::new();
    let mut block_bytes = HEADER_BYTES;
    for batch in batches {
        let lengths: Vec<usize> = batch.iter().map(Vec::len).collect();
        let batch_bytes = payload_bytes(&lengths).ok_or(GroupLogError::TooLarge)?;
        let joined = block_bytes
            .checked_add(batch_bytes)
            .ok_or(GroupLogError::TooLarge)?;
        if !block.is_empty() && joined > max_block_bytes {
            push_block(&block, &mut out)?;
            block.clear();
            block_bytes = HEADER_BYTES;
        }
        block_bytes = block_bytes
            .checked_add(batch_bytes)
            .ok_or(GroupLogError::TooLarge)?;
        block.extend(batch.iter().map(Vec::as_slice));
    }
    if !block.is_empty() {
        push_block(&block, &mut out)?;
    }
    Ok(out)
}

fn push_block(records: &[&[u8]], out: &mut Vec<u8>) -> Result<(), GroupLogError> {
    let mut payload = Vec::new();
    for record in records {
        let length = u32::try_from(record.len()).map_err(|_| GroupLogError::TooLarge)?;
        payload.extend_from_slice(&length.to_be_bytes());
        payload.extend_from_slice(record);
    }
    let payload_length = u32::try_from(payload.len()).map_err(|_| GroupLogError::TooLarge)?;
    let count = u32::try_from(records.len()).map_err(|_| GroupLogError::TooLarge)?;
    let mut fields = [0_u8; FIELDS_BYTES];
    let header = MAGIC
        .into_iter()
        .chain([VERSION, 0, 0, 0])
        .chain(payload_length.to_be_bytes())
        .chain(count.to_be_bytes());
    for (slot, byte) in fields.iter_mut().zip(header) {
        *slot = byte;
    }
    out.extend_from_slice(&fields);
    out.extend_from_slice(&digest(&fields, &payload));
    out.extend_from_slice(&payload);
    Ok(())
}

fn digest(fields: &[u8], payload: &[u8]) -> [u8; DIGEST_BYTES] {
    let mut hasher = Sha256::new();
    hasher.update(DOMAIN);
    hasher.update(fields);
    hasher.update(payload);
    hasher.finalize().into()
}

/// Recovers every complete block from `bytes`. Returns the records and the
/// length of the valid prefix; bytes after it are a torn tail.
///
/// # Errors
/// [`GroupLogError::Corrupt`] when a damaged block precedes a valid one.
pub(super) fn scan(
    bytes: &[u8],
    max_block_bytes: usize,
) -> Result<(Vec<Vec<u8>>, usize), GroupLogError> {
    let mut records = Vec::new();
    let mut offset = 0;
    while let Some(rest) = bytes.get(offset..).filter(|rest| !rest.is_empty()) {
        let Some((block_records, size)) = parse(rest, max_block_bytes) else {
            if later_block(rest, max_block_bytes) {
                let offset = u64::try_from(offset).map_err(|_| GroupLogError::Unavailable)?;
                return Err(GroupLogError::Corrupt { offset });
            }
            return Ok((records, offset));
        };
        records.extend(block_records);
        offset = offset.checked_add(size).ok_or(GroupLogError::Unavailable)?;
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
                .is_some_and(|rest| parse(rest, max_block_bytes).is_some())
        })
}

/// Parses one block at the start of `bytes`: its records and encoded size.
fn parse(bytes: &[u8], max_block_bytes: usize) -> Option<(Vec<Vec<u8>>, usize)> {
    let (fields, rest) = bytes.split_at_checked(FIELDS_BYTES)?;
    let (stored, rest) = rest.split_at_checked(DIGEST_BYTES)?;
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
    ] = <[u8; FIELDS_BYTES]>::try_from(fields).ok()?;
    if [m0, m1, m2, m3] != MAGIC || version != VERSION || [r0, r1, r2] != [0, 0, 0] {
        return None;
    }
    let payload_length = usize::try_from(u32::from_be_bytes([l0, l1, l2, l3])).ok()?;
    let size = HEADER_BYTES.checked_add(payload_length)?;
    if size > max_block_bytes {
        return None;
    }
    let payload = rest.get(..payload_length)?;
    if digest(fields, payload) != *stored {
        return None;
    }
    let count = usize::try_from(u32::from_be_bytes([c0, c1, c2, c3])).ok()?;
    let records = records(payload, count)?;
    Some((records, size))
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
