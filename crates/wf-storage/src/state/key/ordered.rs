//! Total due/FIFO ordering and original-ID history order; no backend guards.

use wf_domain::claim::{DueOrder, LaneKey, LaneOrder};
use wf_domain::history::HistoryEntry;
use wf_domain::workflow::request::RequestScope;
use wf_types::identity::{PartitionId, RecordId};

use super::{
    DecodedOrderedKey, StateValueError, components, encode_components, expected, header, key_size,
    key_start, own, put_component,
};
use crate::state::value::{add, canonical_history_position, finish, read_u8, read_u64, read_u128};

fn scope_fields(scope: &RequestScope) -> [&[u8]; 3] {
    [
        scope.group().as_bytes(),
        scope.recovery().as_bytes(),
        scope.tenant().as_bytes(),
    ]
}

/// Emits the complete tenant due prefix, without asserting range completeness.
/// # Errors
/// Rejects byte caps, checked size overflow or allocation failure.
pub fn encode_due_prefix(
    scope: &RequestScope,
    maximum_bytes: usize,
) -> Result<Vec<u8>, StateValueError> {
    encode_components(3, &scope_fields(scope), maximum_bytes)
}

/// Emits priority descending, then due and exact binary tie fields ascending.
/// # Errors
/// Rejects priority outside0..2, byte caps, size overflow or allocation failure.
pub fn encode_due_key(
    scope: &RequestScope,
    key: DueOrder<'_>,
    maximum_bytes: usize,
) -> Result<Vec<u8>, StateValueError> {
    if maximum_bytes == 0 || key.priority > 2 {
        return Err(StateValueError::InvalidValue);
    }
    let scope = scope_fields(scope);
    let fields = [
        scope[0],
        scope[1],
        scope[2],
        key.id,
        key.partition_key,
        key.state,
        key.workflow_type,
    ];
    let mut output = key_start(3, key_size(&fields, 9)?, maximum_bytes)?;
    for field in &scope {
        put_component(&mut output, field);
    }
    output.push(2 - key.priority);
    output.extend_from_slice(&key.due.get().to_be_bytes());
    for field in &fields[3..] {
        put_component(&mut output, field);
    }
    Ok(output)
}

/// Emits a lane prefix with exact type/state/partition ordering.
/// # Errors
/// Rejects byte caps, size overflow or allocation failure.
pub fn encode_fifo_prefix(
    scope: &RequestScope,
    lane: LaneKey<'_>,
    maximum_bytes: usize,
) -> Result<Vec<u8>, StateValueError> {
    let scope = scope_fields(scope);
    encode_components(
        4,
        &[
            scope[0],
            scope[1],
            scope[2],
            lane.workflow_type,
            lane.state,
            lane.partition_key,
        ],
        maximum_bytes,
    )
}

/// Emits running score, full u128 sequence and opaque member ID after the lane.
/// # Errors
/// Rejects unsupported score, byte caps, size overflow or allocation failure.
pub fn encode_fifo_key(
    scope: &RequestScope,
    lane: LaneKey<'_>,
    member: LaneOrder<'_>,
    maximum_bytes: usize,
) -> Result<Vec<u8>, StateValueError> {
    if maximum_bytes == 0 {
        return Err(StateValueError::InvalidValue);
    }
    let rank = match member.score() {
        -1 => 0,
        0 => 1,
        _ => return Err(StateValueError::InvalidValue),
    };
    let scope = scope_fields(scope);
    let fields = [
        scope[0],
        scope[1],
        scope[2],
        lane.workflow_type,
        lane.state,
        lane.partition_key,
        member.id(),
    ];
    let mut output = key_start(4, key_size(&fields, 17)?, maximum_bytes)?;
    for field in &fields[..6] {
        put_component(&mut output, field);
    }
    output.push(rank);
    output.extend_from_slice(&member.sequence().to_be_bytes());
    put_component(&mut output, member.id());
    Ok(output)
}

/// Emits one exact record's history prefix, without a query/authority claim.
/// # Errors
/// Rejects byte caps, size overflow or allocation failure.
pub fn encode_history_prefix(
    scope: &RequestScope,
    partition: &PartitionId,
    id: &RecordId,
    maximum_bytes: usize,
) -> Result<Vec<u8>, StateValueError> {
    let scope = scope_fields(scope);
    encode_components(
        5,
        &[
            scope[0],
            scope[1],
            scope[2],
            partition.as_bytes(),
            id.as_bytes(),
        ],
        maximum_bytes,
    )
}

/// Orders by numeric ID milliseconds then original ID, independently of time/version.
/// # Errors
/// Rejects a noncanonical/mismatched ID, caps, size overflow or allocation failure.
pub fn encode_history_key(
    scope: &RequestScope,
    partition: &PartitionId,
    id: &RecordId,
    event: &HistoryEntry,
    maximum_bytes: usize,
) -> Result<Vec<u8>, StateValueError> {
    if maximum_bytes == 0 {
        return Err(StateValueError::InvalidValue);
    }
    let (milliseconds, version) = canonical_history_position(event.event_id.as_bytes())?;
    if version != event.version {
        return Err(StateValueError::InvalidValue);
    }
    let scope = scope_fields(scope);
    let fields = [
        scope[0],
        scope[1],
        scope[2],
        partition.as_bytes(),
        id.as_bytes(),
        event.event_id.as_bytes(),
    ];
    let mut output = key_start(5, key_size(&fields, 8)?, maximum_bytes)?;
    for field in &fields[..5] {
        put_component(&mut output, field);
    }
    output.extend_from_slice(&milliseconds.to_be_bytes());
    put_component(&mut output, event.event_id.as_bytes());
    Ok(output)
}

/// Validates complete due framing/namespace before admitting four copied fields.
/// # Errors
/// Rejects malformed/versioned data, foreign scope, scalar/cap or allocation failure.
pub fn decode_due_key(
    bytes: &[u8],
    scope: &RequestScope,
    maximum_bytes: usize,
    maximum_decoded_bytes: usize,
) -> Result<DecodedOrderedKey, StateValueError> {
    let mut offset = header(bytes, 3, maximum_bytes, maximum_decoded_bytes)?;
    let (namespace, _) = components::<3>(bytes, &mut offset)?;
    let rank = read_u8(bytes, &mut offset)?;
    let due = read_u64(bytes, &mut offset)?;
    let (parts, length) = components::<4>(bytes, &mut offset)?;
    finish(bytes, offset)?;
    expected(bytes, &namespace, &scope_fields(scope))?;
    if rank > 2 {
        return Err(StateValueError::InvalidValue);
    }
    own(
        3,
        rank,
        u128::from(due),
        bytes,
        &parts,
        length,
        maximum_decoded_bytes,
    )
}

/// Validates complete FIFO framing and expected lane before four copied fields.
/// # Errors
/// Rejects malformed/versioned data, foreign scope/lane, caps or allocation failure.
pub fn decode_fifo_key(
    bytes: &[u8],
    scope: &RequestScope,
    lane: LaneKey<'_>,
    maximum_bytes: usize,
    maximum_decoded_bytes: usize,
) -> Result<DecodedOrderedKey, StateValueError> {
    let mut offset = header(bytes, 4, maximum_bytes, maximum_decoded_bytes)?;
    let (namespace, _) = components::<3>(bytes, &mut offset)?;
    let (lane_parts, mut length) = components::<3>(bytes, &mut offset)?;
    let rank = read_u8(bytes, &mut offset)?;
    let sequence = read_u128(bytes, &mut offset)?;
    let (id, id_length) = components::<1>(bytes, &mut offset)?;
    finish(bytes, offset)?;
    expected(bytes, &namespace, &scope_fields(scope))?;
    expected(
        bytes,
        &lane_parts,
        &[lane.workflow_type, lane.state, lane.partition_key],
    )?;
    if rank > 1 {
        return Err(StateValueError::UnsupportedTag);
    }
    add(&mut length, id_length)?;
    own(
        4,
        rank,
        sequence,
        bytes,
        &[lane_parts[0], lane_parts[1], lane_parts[2], id[0]],
        length,
        maximum_decoded_bytes,
    )
}

/// Validates full history identity/score before copying only the original ID.
/// # Errors
/// Rejects malformed/versioned data, foreign identity, invalid ID, caps or allocation.
pub fn decode_history_key(
    bytes: &[u8],
    scope: &RequestScope,
    partition: &PartitionId,
    id: &RecordId,
    maximum_bytes: usize,
    maximum_decoded_bytes: usize,
) -> Result<DecodedOrderedKey, StateValueError> {
    let mut offset = header(bytes, 5, maximum_bytes, maximum_decoded_bytes)?;
    let (namespace, _) = components::<5>(bytes, &mut offset)?;
    let milliseconds = read_u64(bytes, &mut offset)?;
    let (parts, length) = components::<1>(bytes, &mut offset)?;
    finish(bytes, offset)?;
    let scope = scope_fields(scope);
    expected(
        bytes,
        &namespace,
        &[
            scope[0],
            scope[1],
            scope[2],
            partition.as_bytes(),
            id.as_bytes(),
        ],
    )?;
    let (found, _) = canonical_history_position(&bytes[parts[0].0..parts[0].1])?;
    if found != milliseconds {
        return Err(StateValueError::InvalidValue);
    }
    own(
        5,
        0,
        u128::from(milliseconds),
        bytes,
        &parts,
        length,
        maximum_decoded_bytes,
    )
}
