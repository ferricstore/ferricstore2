//! Explicit original-record metadata data capture, without routing or durability.
mod budget;
mod model;
mod schema;
use crate::value::{
    OriginalValueContext, OriginalValueEncodeLimits, OriginalValueInput, OriginalValueLimits,
};
pub use model::{
    OriginalRecordMetadata, OriginalRecordMetadataError, OriginalRecordMetadataField,
    PreparedOriginalRecordMetadata,
};

/// Prepares one versioned capture using the shared original stored-value codec.
/// Fields preserve missing/nil/empty; values are data, never source admission.
/// # Errors
/// Refuses malformed schemas, unsupported captures, resources and allocations.
pub fn prepare_original_record_metadata(
    input: OriginalValueInput<'_>,
    init_pid: Option<usize>,
    limits: OriginalValueEncodeLimits,
) -> Result<PreparedOriginalRecordMetadata, OriginalRecordMetadataError> {
    use crate::value::{prepare_original_flow_value, prepare_original_flow_value_with_init_pid};
    budget::encode_limits(input, limits)?;
    let mut remaining = limits.max_work_units;
    budget::spend(&mut remaining, budget::CONTROL)?;
    schema::validate(input, &mut remaining)?;
    let mut selected = limits;
    selected.max_work_units = budget::positive(remaining)?;
    selected.max_temporary_bytes = budget::positive(
        limits
            .max_temporary_bytes
            .checked_sub(budget::CONTROL)
            .ok_or(OriginalRecordMetadataError::Limit)?,
    )?;
    let value = match init_pid {
        Some(pid) => prepare_original_flow_value_with_init_pid(input, pid, selected),
        None => prepare_original_flow_value(input, selected),
    }
    .map_err(OriginalRecordMetadataError::Encoding)?;
    budget::spend(&mut remaining, value.work_units())?;
    let temporary = budget::CONTROL
        .checked_add(value.temporary_bytes())
        .ok_or(OriginalRecordMetadataError::Limit)?;
    Ok(PreparedOriginalRecordMetadata {
        value,
        work: limits.max_work_units - remaining,
        temporary,
    })
}

/// Moves one complete capture into an owned typed document using explicit context.
/// # Errors
/// Refuses schema/version, malformed stored data, missing context or resources.
pub fn decode_original_record_metadata(
    encoded: Vec<u8>,
    context: &OriginalValueContext<'_>,
    limits: OriginalValueLimits,
) -> Result<OriginalRecordMetadata, OriginalRecordMetadataError> {
    use crate::value::decode_original_flow_value_with_work;
    budget::decode_limits(limits)?;
    let mut remaining = limits.max_work_units;
    budget::spend(&mut remaining, budget::CONTROL)?;
    let mut selected = limits;
    selected.max_work_units = budget::positive(remaining)?;
    let (document, used) = decode_original_flow_value_with_work(encoded, context, selected)
        .map_err(OriginalRecordMetadataError::Decoding)?;
    budget::spend(&mut remaining, used)?;
    let fields = schema::validate(document.as_input(), &mut remaining)?;
    Ok(OriginalRecordMetadata {
        document,
        fields,
        work: limits.max_work_units - remaining,
    })
}

/// Decode with an additional prospective auxiliary-buffer ceiling.
/// The moved input backing is external; the returned peak includes fixed control
/// and conservatively accumulated requested buffers plus relocation, not RSS.
/// # Errors
/// Preserve original schema/context/codec failures and independent resource refusal.
pub fn decode_original_record_metadata_bounded(
    encoded: Vec<u8>,
    context: &OriginalValueContext<'_>,
    limits: OriginalValueLimits,
    maximum_temporary_bytes: usize,
) -> Result<(OriginalRecordMetadata, usize), OriginalRecordMetadataError> {
    budget::decode_limits(limits)?;
    if maximum_temporary_bytes == 0 {
        return Err(OriginalRecordMetadataError::InvalidLimits);
    }
    let auxiliary = maximum_temporary_bytes
        .checked_sub(budget::CONTROL)
        .filter(|n| *n > 0)
        .ok_or(OriginalRecordMetadataError::Limit)?;
    let mut remaining = limits.max_work_units;
    budget::spend(&mut remaining, budget::CONTROL)?;
    let mut selected = limits;
    selected.max_work_units = budget::positive(remaining)?;
    let (document, used, temporary) =
        crate::value::decode_original_flow_value_bounded(encoded, context, selected, auxiliary)
            .map_err(OriginalRecordMetadataError::Decoding)?;
    budget::spend(&mut remaining, used)?;
    let fields = schema::validate(document.as_input(), &mut remaining)?;
    let temporary = budget::CONTROL
        .checked_add(temporary)
        .ok_or(OriginalRecordMetadataError::Limit)?;
    Ok((
        OriginalRecordMetadata {
            document,
            fields,
            work: limits.max_work_units - remaining,
        },
        temporary,
    ))
}
