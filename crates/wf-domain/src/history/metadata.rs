//! Source history metadata normalization from explicit borrowed captures.

mod budget;
mod model;
mod numeric;
mod scalar;
mod text;

pub use model::{
    OriginalHistoryMetadataError, OriginalHistoryMetadataField, OriginalHistoryMetadataLimits,
    PreparedOriginalHistoryMetadata,
};

use crate::value::{OriginalValueInput, OriginalValueNode};

use budget::Budget;

/// Normalize captured map rows in their original enumeration order.
///
/// This acquires no native terms, record data or history-read authority.
/// Binary and UTF8 atom output may borrow the input byte pool.
/// Only root rows and selected scalar spans are interpreted; ignored compound
/// bodies are not traversed, and this does not validate the entire arena.
///
/// # Errors
/// Rejects invalid positive limits, selected capture spans/scalars, operational
/// resources/overflow and actual fallible reservations. No partial result escapes.
pub fn prepare_original_history_metadata(
    input: OriginalValueInput<'_>,
    limits: OriginalHistoryMetadataLimits,
) -> Result<PreparedOriginalHistoryMetadata<'_>, OriginalHistoryMetadataError> {
    let mut charge = Budget::new(input, limits)?;
    let root = input
        .nodes
        .get(input.root)
        .ok_or(OriginalHistoryMetadataError::InvalidInput)?;
    let mut fields = Vec::new();
    if let OriginalValueNode::Map { entries } = root {
        let rows = input
            .edges
            .get(entries.clone())
            .ok_or(OriginalHistoryMetadataError::InvalidInput)?;
        if !rows.len().is_multiple_of(2) {
            return Err(OriginalHistoryMetadataError::InvalidInput);
        }
        if rows.len() / 2 > limits.max_pairs {
            return Err(OriginalHistoryMetadataError::Limit);
        }
        fields = charge.reserve(rows.len() / 2)?;
        for [key, value] in rows.as_chunks::<2>().0 {
            let key = input
                .nodes
                .get(*key)
                .ok_or(OriginalHistoryMetadataError::InvalidInput)?;
            if !scalar::supported_key(key) {
                continue;
            }
            let value = input
                .nodes
                .get(*value)
                .ok_or(OriginalHistoryMetadataError::InvalidInput)?;
            if !scalar::supported_value(value) {
                continue;
            }
            fields.push(OriginalHistoryMetadataField {
                key: scalar::text(input, key, false, &mut charge)?,
                value: scalar::text(input, value, true, &mut charge)?,
            });
        }
    }
    Ok(PreparedOriginalHistoryMetadata {
        fields,
        work: charge.work,
        temporary: charge.peak,
    })
}
