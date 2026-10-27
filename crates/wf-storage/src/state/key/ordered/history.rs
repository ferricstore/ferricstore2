//! Exact original-ID history keys, preserving physical namespace and ordering.

use wf_domain::history::HistoryEntry;
use wf_domain::workflow::request::RequestScope;
use wf_types::identity::{PartitionId, RecordId};

use super::{
    DecodedOrderedKey, StateValueError, components, encode_components, expected, header, key_size,
    key_start, own, put_component, scope_fields,
};
use crate::state::value::{canonical_history_position, finish, read_u64};

/// Emits an exclusive seek position without fabricating a history observation.
/// # Errors
/// Rejects noncanonical IDs, zero/exceeded caps, size overflow or allocation.
pub fn encode_history_cursor_key(
    scope: &RequestScope,
    partition: &PartitionId,
    id: &RecordId,
    event_id: &str,
    maximum_bytes: usize,
) -> Result<Vec<u8>, StateValueError> {
    if maximum_bytes == 0 {
        return Err(StateValueError::InvalidValue);
    }
    let (milliseconds, _) = canonical_history_position(event_id.as_bytes())?;
    let scope = scope_fields(scope);
    let fields = [
        scope[0],
        scope[1],
        scope[2],
        partition.as_bytes(),
        id.as_bytes(),
        event_id.as_bytes(),
    ];
    let mut output = key_start(5, key_size(&fields, 8)?, maximum_bytes)?;
    for field in &fields[..5] {
        put_component(&mut output, field);
    }
    output.extend_from_slice(&milliseconds.to_be_bytes());
    put_component(&mut output, event_id.as_bytes());
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
