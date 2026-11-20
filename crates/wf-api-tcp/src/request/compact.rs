//! Compact request bodies: `FLOW.CLAIM_DUE` (tag `0x91`) and
//! `FLOW.COMPLETE_MANY` (tags `0x92`, and `0x93` for ok-on-success).

use crate::codec::Reader;
use crate::frame::FLAG_CUSTOM_PAYLOAD;

use super::{ClaimDue, ClaimShape, CompleteItem, CompleteMany, Partitions, RequestError};

const CLAIM_DUE_TAG: u8 = 0x91;
const COMPLETE_MANY_TAG: u8 = 0x92;
const COMPLETE_MANY_OK_TAG: u8 = 0x93;
/// Smallest completion item: id, partition and lease lengths plus fencing.
const MIN_COMPLETE_ITEM_BYTES: usize = 20;
/// Smallest partition name: its length.
const MIN_PARTITION_BYTES: usize = 4;

/// Returns the payload tag and a reader after it when the frame carries a
/// compact payload.
fn compact(flags: u8, body: &[u8]) -> Option<(u8, Reader<'_>)> {
    if flags & FLAG_CUSTOM_PAYLOAD == 0 {
        return None;
    }
    let mut reader = Reader::new(body);
    let tag = reader.u8()?;
    Some((tag, reader))
}

fn owned(bytes: Option<&[u8]>) -> Option<Vec<u8>> {
    bytes.map(<[u8]>::to_vec)
}

/// Parses a compact `FLOW.CLAIM_DUE` body.
///
/// # Errors
/// Refuses a typed body, another payload tag, a truncated or over-long body,
/// and unknown reply-shape, reclaim or partition-mode bytes.
pub fn claim_due(flags: u8, body: &[u8]) -> Result<ClaimDue, RequestError> {
    let invalid = || RequestError::new("ERR native compact FLOW.CLAIM_DUE payload is invalid");
    let Some((CLAIM_DUE_TAG, mut reader)) = compact(flags, body) else {
        return Err(RequestError::new(
            "ERR FLOW.CLAIM_DUE requires the compact request payload",
        ));
    };
    let flow_type = owned(reader.bin()).ok_or_else(invalid)?;
    let state = reader
        .optional_bin()
        .map_err(|_| invalid())?
        .map(<[u8]>::to_vec);
    let worker = owned(reader.bin()).ok_or_else(invalid)?;
    let lease_ms = reader.i64().ok_or_else(invalid)?;
    let limit = reader.i64().ok_or_else(invalid)?;
    let block_ms = reader.i64().ok_or_else(invalid)?;
    let reclaim_expired = match reader.u8().ok_or_else(invalid)? {
        0 => false,
        1 => true,
        _ => return Err(invalid()),
    };
    let reclaim_ratio = reader.i64().ok_or_else(invalid)?;
    let priority = reader.i64().ok_or_else(invalid)?;
    let shape = match reader.u8().ok_or_else(invalid)? {
        1 => ClaimShape::Jobs,
        2 => ClaimShape::JobsState,
        3 => ClaimShape::JobsAttributes,
        4 => ClaimShape::JobsStateAttributes,
        _ => {
            return Err(RequestError::new(
                "ERR FLOW.CLAIM_DUE return mode is not supported by this server",
            ));
        }
    };
    let partitions = partitions(&mut reader).ok_or_else(invalid)?;
    if !reader.is_empty() {
        return Err(invalid());
    }
    Ok(ClaimDue {
        flow_type,
        state,
        worker,
        lease_ms,
        limit,
        block_ms: (block_ms >= 0).then_some(block_ms),
        reclaim_expired,
        reclaim_ratio,
        priority: (priority != i64::MIN).then_some(priority),
        shape,
        partitions,
    })
}

fn partitions(reader: &mut Reader<'_>) -> Option<Partitions> {
    match reader.u8()? {
        0 => Some(Partitions::Any),
        1 => owned(reader.bin()).map(Partitions::One),
        2 => {
            let count = reader.count(MIN_PARTITION_BYTES)?;
            let mut names = Vec::with_capacity(count);
            for _ in 0..count {
                names.push(owned(reader.bin())?);
            }
            Some(Partitions::Many(names))
        }
        _ => None,
    }
}

/// Parses a compact `FLOW.COMPLETE_MANY` body.
///
/// # Errors
/// Refuses a typed body, another payload tag, a truncated or over-long body
/// and an unknown independence byte.
pub fn complete_many(flags: u8, body: &[u8]) -> Result<CompleteMany, RequestError> {
    let invalid = || RequestError::new("ERR native compact FLOW.COMPLETE_MANY payload is invalid");
    let (ok_on_success, mut reader) = match compact(flags, body) {
        Some((COMPLETE_MANY_TAG, reader)) => (false, reader),
        Some((COMPLETE_MANY_OK_TAG, reader)) => (true, reader),
        _ => {
            return Err(RequestError::new(
                "ERR FLOW.COMPLETE_MANY requires the compact request payload",
            ));
        }
    };
    let partition_key = reader
        .optional_bin()
        .map_err(|_| invalid())?
        .map(<[u8]>::to_vec);
    let now_ms = reader.i64().ok_or_else(invalid)?;
    // 0 default, 1 false, 2 true, 3 true and terminal-local-only, 4 false and
    // terminal-local-only.
    let (independent, terminal_local_only) = match reader.u8().ok_or_else(invalid)? {
        0 | 1 => (false, false),
        2 => (true, false),
        3 => (true, true),
        4 => (false, true),
        _ => return Err(invalid()),
    };
    let count = reader.count(MIN_COMPLETE_ITEM_BYTES).ok_or_else(invalid)?;
    let mut items = Vec::with_capacity(count);
    for _ in 0..count {
        items.push(complete_item(&mut reader).ok_or_else(invalid)?);
    }
    if !reader.is_empty() {
        return Err(invalid());
    }
    Ok(CompleteMany {
        partition_key,
        now_ms,
        independent,
        terminal_local_only,
        ok_on_success,
        items,
    })
}

fn complete_item(reader: &mut Reader<'_>) -> Option<CompleteItem> {
    Some(CompleteItem {
        id: owned(reader.bin())?,
        partition_key: reader.optional_bin().ok()?.map(<[u8]>::to_vec),
        lease_token: owned(reader.bin())?,
        fencing_token: reader.i64()?,
    })
}
