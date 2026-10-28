//! Source history metadata normalization from explicit borrowed captures.

mod model;

pub use model::{
    OriginalHistoryMetadataError, OriginalHistoryMetadataField, OriginalHistoryMetadataLimits,
    PreparedOriginalHistoryMetadata,
};

use crate::value::OriginalValueInput;

/// Normalize captured map rows in their original enumeration order.
///
/// This acquires no native terms, record data or history-read authority.
/// Binary and UTF8 atom output may borrow the input byte pool.
///
/// # Errors
/// This rejecting TDD facade has no normalization implementation yet.
pub fn prepare_original_history_metadata(
    _input: OriginalValueInput<'_>,
    _limits: OriginalHistoryMetadataLimits,
) -> Result<PreparedOriginalHistoryMetadata<'_>, OriginalHistoryMetadataError> {
    Err(OriginalHistoryMetadataError::InvalidInput)
}
