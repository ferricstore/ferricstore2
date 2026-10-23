//! Original state-metadata validation and normalization.

use super::{OriginalMetadataError, OriginalMetadataInput, OriginalStateMeta};

/// Rejection-only baseline for original canonical scalar-only state metadata.
#[must_use]
pub fn valid_original_state_meta(_value: OriginalMetadataInput<'_, '_>) -> bool {
    false
}

/// Rejection-only baseline for complete original state-metadata normalization.
///
/// # Errors
/// Returns the original root-map diagnosis for every input.
pub fn normalize_original_state_meta<'leaf>(
    _value: OriginalMetadataInput<'_, 'leaf>,
) -> Result<OriginalStateMeta<'leaf>, OriginalMetadataError> {
    Err(OriginalMetadataError::SourceReturned(
        "ERR flow state_meta must be a map",
    ))
}
