//! Original state-key identity, reusing the existing partition tag codec.
use super::{MAX_KEY_BYTES, put, reserve_key, split_tag, tag};
use crate::blob::{OriginalValueBuildError as B, OriginalValuePartition as P};
use wf_domain::metadata::OriginalMetadataInput as I;
use wf_domain::metadata::scope::{
    OriginalScopeError as E, OriginalScopeLimits as L, OriginalScopeResult as R,
    original_record_physical_partition_observation,
};

/// Extract the source workflow ID after a canonical state-key tag.
#[must_use]
pub fn parse_original_state_key(input: &[u8]) -> Option<&[u8]> {
    let (_, id) = split_tag(input, *b"}:s:")?;
    if id.is_empty() { None } else { Some(id) }
}

/// Render a source state key with one bounded final allocation.
/// # Errors
/// Refuses independent output ceilings and failed reservations.
pub fn render_original_state_key(
    id: &[u8],
    partition: P<'_>,
    maximum: usize,
) -> Result<Vec<u8>, B> {
    let maximum = maximum.min(MAX_KEY_BYTES);
    let fixed = id.len().checked_add(3).ok_or(B::Limit)?;
    let remaining = maximum.checked_sub(fixed).ok_or(B::Limit)?;
    let mut tag_bytes = [0; 49];
    let size = tag::render_tag(partition, id, remaining, &mut tag_bytes)?;
    let length = fixed.checked_add(size).ok_or(B::Limit)?;
    let mut bytes = reserve_key(length)?;
    let mut cursor = 0;
    put(
        &mut bytes,
        &mut cursor,
        tag_bytes.get(..size).ok_or(B::Limit)?,
    )?;
    put(&mut bytes, &mut cursor, b":s:")?;
    put(&mut bytes, &mut cursor, id)?;
    Ok(bytes)
}

/// Test exact original state-key ownership from captured record fields.
/// # Errors
/// Source invalidity returns false; operational refusals remain errors.
pub fn owns_original_state_key(
    id: Option<I<'_, '_>>,
    partition: Option<I<'_, '_>>,
    metadata: Option<I<'_, '_>>,
    state_key: I<'_, '_>,
    limits: L,
) -> Result<R<bool>, E> {
    validate_limits(limits)?;
    let mut result = R {
        value: false,
        work_units: 49,
        temporary_bytes: 49,
    };
    charge(&mut result, 0, limits)?;
    let (Some(I::Binary(id)), I::Binary(state_key)) = (id, state_key) else {
        return Ok(result);
    };
    if id.is_empty()
        || id.len() > MAX_KEY_BYTES
        || state_key.is_empty()
        || state_key.len() > MAX_KEY_BYTES
    {
        return Ok(result);
    }
    charge(
        &mut result,
        id.len().checked_add(state_key.len()).ok_or(E::Limit)?,
        limits,
    )?;
    if parse_original_state_key(state_key) != Some(id) {
        return Ok(result);
    }
    let mut nested = limits;
    nested.maximum_work_units = limits
        .maximum_work_units
        .checked_sub(result.work_units)
        .filter(|n| *n > 0)
        .ok_or(E::Limit)?;
    nested.maximum_temporary_bytes = limits
        .maximum_temporary_bytes
        .checked_sub(49)
        .filter(|n| *n > 0)
        .ok_or(E::Limit)?;
    // This is an intermediate source partition, not the boolean result's output.
    nested.maximum_output_bytes = usize::MAX;
    let physical = original_record_physical_partition_observation(partition, metadata, nested)?;
    charge(&mut result, physical.work_units, limits)?;
    result.temporary_bytes = physical.temporary_bytes.checked_add(49).ok_or(E::Limit)?;
    let Ok(physical_value) = physical.value else {
        return Ok(result);
    };
    let partition = match physical_value.as_deref() {
        None => P::Automatic,
        Some(value) if value.len() <= MAX_KEY_BYTES => P::Explicit(value),
        Some(_) => return Ok(result),
    };
    let scan = physical_value.as_deref().map_or(id.len(), <[u8]>::len);
    charge(&mut result, scan.checked_add(49).ok_or(E::Limit)?, limits)?;
    let mut rendered = [0; 49];
    let length = tag::render_tag(partition, id, 49, &mut rendered).map_err(|_| E::Limit)?;
    let (actual_tag, _) = split_tag(state_key, *b"}:s:").ok_or(E::InvalidSystemMetadata)?;
    result.value = actual_tag == rendered.get(..length).ok_or(E::Limit)?;
    Ok(result)
}

fn validate_limits(limits: L) -> Result<(), E> {
    if [
        limits.maximum_output_bytes,
        limits.maximum_work_units,
        limits.maximum_temporary_bytes,
    ]
    .contains(&0)
    {
        return Err(E::InvalidLimits);
    }
    Ok(())
}

fn charge(result: &mut R<bool>, units: usize, limits: L) -> Result<(), E> {
    result.work_units = result.work_units.checked_add(units).ok_or(E::Limit)?;
    if result.work_units > limits.maximum_work_units
        || result.temporary_bytes > limits.maximum_temporary_bytes
    {
        return Err(E::Limit);
    }
    Ok(())
}
