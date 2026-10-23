//! Original attribute predicates and normalization at the captured boundary.

use super::{OriginalAttributes, OriginalMetadataError, OriginalMetadataInput};

/// Rejection-only baseline for canonical original attributes.
#[must_use]
pub fn valid_original_attributes(_value: OriginalMetadataInput<'_, '_>) -> bool {
    false
}

/// Rejection-only baseline for an original canonical scalar.
#[must_use]
pub fn valid_original_attribute_scalar(_value: OriginalMetadataInput<'_, '_>) -> bool {
    false
}

/// Rejection-only baseline for an original canonical binary list.
#[must_use]
pub fn valid_original_attribute_list(_value: OriginalMetadataInput<'_, '_>) -> bool {
    false
}

/// Rejection-only baseline for complete original attribute normalization.
///
/// # Errors
/// Returns the original root-map diagnosis for every input.
pub fn normalize_original_attributes<'leaf>(
    _value: OriginalMetadataInput<'_, 'leaf>,
) -> Result<OriginalAttributes<'leaf>, OriginalMetadataError> {
    Err(OriginalMetadataError::SourceReturned(
        "ERR flow attributes must be a map",
    ))
}
