//! Total due/FIFO ordering and original-ID history order; no backend guards.

use wf_domain::claim::{DueOrder, LaneKey, LaneOrder};
use wf_domain::workflow::request::RequestScope;

use super::{
    DecodedOrderedKey, StateValueError, components, encode_components, expected, header, key_size,
    key_start, own, put_component,
};
use crate::state::value::{add, finish, read_u8, read_u64, read_u128};

mod history;
pub use history::{
    decode_history_key, encode_history_cursor_key, encode_history_key, encode_history_prefix,
};

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
/// Emits the exact requested type's FIFO prefix for distinct-lane discovery.
/// # Errors
/// Rejects byte caps, checked length overflow or allocation failure.
pub fn encode_fifo_type_prefix(
    scope: &RequestScope,
    workflow_type: &[u8],
    maximum_bytes: usize,
) -> Result<Vec<u8>, StateValueError> {
    let fields = scope_fields(scope);
    encode_components(
        4,
        &[fields[0], fields[1], fields[2], workflow_type],
        maximum_bytes,
    )
}
/// Returns the lexical successor of the COMPLETE escaped lane prefix.
/// # Errors
/// Rejects caps, framing, arithmetic or allocation failure.
pub fn encode_fifo_lane_upper_bound(
    scope: &RequestScope,
    lane: LaneKey<'_>,
    maximum_bytes: usize,
) -> Result<Vec<u8>, StateValueError> {
    let mut upper = encode_fifo_prefix(scope, lane, maximum_bytes)?;
    let last = upper.last_mut().ok_or(StateValueError::Malformed)?;
    *last = last.checked_add(1).ok_or(StateValueError::LengthOverflow)?;
    Ok(upper)
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

/// Validates a complete FIFO member without presupposing its discovered lane.
/// # Errors
/// Rejects malformed/foreign keys and admits all four fields before ownership.
pub fn decode_fifo_observation_key(
    bytes: &[u8],
    scope: &RequestScope,
    maximum_bytes: usize,
    maximum_decoded_bytes: usize,
) -> Result<DecodedOrderedKey, StateValueError> {
    let mut offset = header(bytes, 4, maximum_bytes, maximum_decoded_bytes)?;
    let (namespace, _) = components::<3>(bytes, &mut offset)?;
    let (lane, mut length) = components::<3>(bytes, &mut offset)?;
    let rank = read_u8(bytes, &mut offset)?;
    let sequence = read_u128(bytes, &mut offset)?;
    let (id, n) = components::<1>(bytes, &mut offset)?;
    finish(bytes, offset)?;
    expected(bytes, &namespace, &scope_fields(scope))?;
    if rank > 1 {
        return Err(StateValueError::UnsupportedTag);
    }
    add(&mut length, n)?;
    own(
        4,
        rank,
        sequence,
        bytes,
        &[lane[0], lane[1], lane[2], id[0]],
        length,
        maximum_decoded_bytes,
    )
}

/// Measures the exact decoded due-key owner without allocating any component.
/// # Errors
/// Validates full namespace/rank/framing before returning checked128+four leaves.
pub fn due_key_owned_bytes(
    bytes: &[u8],
    scope: &RequestScope,
    maximum_wire_bytes: usize,
) -> Result<usize, StateValueError> {
    let mut offset = header(bytes, 3, maximum_wire_bytes, usize::MAX)?;
    let (namespace, _) = components::<3>(bytes, &mut offset)?;
    let rank = read_u8(bytes, &mut offset)?;
    let _due = read_u64(bytes, &mut offset)?;
    let (_, length) = components::<4>(bytes, &mut offset)?;
    finish(bytes, offset)?;
    expected(bytes, &namespace, &scope_fields(scope))?;
    if rank > 2 {
        return Err(StateValueError::InvalidValue);
    }
    length
        .checked_add(128)
        .ok_or(StateValueError::LengthOverflow)
}

/// Measures the exact decoded discovery key before copying any lane/member bytes.
/// # Errors
/// Validates namespace/rank/scalars/EOF and checked owner arithmetic.
pub fn fifo_observation_key_owned_bytes(
    bytes: &[u8],
    scope: &RequestScope,
    maximum_wire_bytes: usize,
) -> Result<usize, StateValueError> {
    let mut offset = header(bytes, 4, maximum_wire_bytes, usize::MAX)?;
    let (namespace, _) = components::<3>(bytes, &mut offset)?;
    let (_, mut length) = components::<3>(bytes, &mut offset)?;
    let rank = read_u8(bytes, &mut offset)?;
    let _sequence = read_u128(bytes, &mut offset)?;
    let (_, id) = components::<1>(bytes, &mut offset)?;
    finish(bytes, offset)?;
    expected(bytes, &namespace, &scope_fields(scope))?;
    if rank > 1 {
        return Err(StateValueError::UnsupportedTag);
    }
    add(&mut length, id)?;
    length
        .checked_add(128)
        .ok_or(StateValueError::LengthOverflow)
}
