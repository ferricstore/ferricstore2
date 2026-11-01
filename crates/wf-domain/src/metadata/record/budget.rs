//! Schema work and separately held fixed control remain in the caller account.
use super::OriginalRecordMetadataError as E;
use crate::value::{OriginalValueEncodeLimits, OriginalValueInput, OriginalValueLimits};
pub(super) const CONTROL: usize = 4096;
pub(super) fn spend(remaining: &mut usize, amount: usize) -> Result<(), E> {
    *remaining = remaining.checked_sub(amount).ok_or(E::Limit)?;
    Ok(())
}
pub(super) fn positive(value: usize) -> Result<usize, E> {
    if value == 0 { Err(E::Limit) } else { Ok(value) }
}
pub(super) fn encode_limits(
    input: OriginalValueInput<'_>,
    limits: OriginalValueEncodeLimits,
) -> Result<(), E> {
    if [
        limits.max_input_bytes,
        limits.max_nodes,
        limits.max_edges,
        limits.max_depth,
        limits.max_encoded_bytes,
        limits.max_work_units,
        limits.max_temporary_bytes,
    ]
    .contains(&0)
    {
        return Err(E::InvalidLimits);
    }
    if input.bytes.len() > limits.max_input_bytes
        || input.nodes.len() > limits.max_nodes
        || input.edges.len() > limits.max_edges
    {
        return Err(E::Limit);
    }
    if limits.max_temporary_bytes <= CONTROL {
        return Err(E::Limit);
    }
    Ok(())
}
pub(super) fn decode_limits(limits: OriginalValueLimits) -> Result<(), E> {
    if [
        limits.max_encoded_bytes,
        limits.max_nodes,
        limits.max_edges,
        limits.max_depth,
        limits.max_work_units,
    ]
    .contains(&0)
    {
        return Err(E::InvalidLimits);
    }
    Ok(())
}
