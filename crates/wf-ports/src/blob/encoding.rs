//! Logical-binary rejection baselines, independent of typed ETF decoding.

use crate::blob::OriginalValueBuildError;

/// Callable rejection baseline for a logical binary's exact stored length.
///
/// # Errors
///
/// Always returns [`OriginalValueBuildError::InvalidInput`] in this R0 stage.
pub fn original_binary_stored_len(
    _input: &[u8],
    _maximum: usize,
) -> Result<usize, OriginalValueBuildError> {
    Err(OriginalValueBuildError::InvalidInput)
}

/// Callable rejection baseline for borrowed prefix and unchanged binary parts.
///
/// # Errors
///
/// Always returns [`OriginalValueBuildError::InvalidInput`] in this R0 stage.
pub fn original_binary_stored_parts(
    _input: &[u8],
    _maximum: usize,
) -> Result<[&[u8]; 2], OriginalValueBuildError> {
    Err(OriginalValueBuildError::InvalidInput)
}
