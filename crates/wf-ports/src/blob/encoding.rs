//! Borrowed logical-binary framing, independent of typed ETF decoding.

use crate::blob::OriginalValueBuildError;

/// Measure exact stored bytes, escaping a leading source FSV2 marker once.
///
/// # Errors
///
/// Returns an operational limit on overflow or a caller cap breach.
pub fn original_binary_stored_len(
    input: &[u8],
    maximum: usize,
) -> Result<usize, OriginalValueBuildError> {
    let length = if input.starts_with(b"FSV2") {
        input
            .len()
            .checked_add(5)
            .ok_or(OriginalValueBuildError::Limit)?
    } else {
        input.len()
    };
    if length > maximum {
        return Err(OriginalValueBuildError::Limit);
    }
    Ok(length)
}

/// Borrow a static escape prefix and the entire unchanged logical input.
///
/// The slices retain their input backing; this is not an input-owner memory cap.
///
/// # Errors
///
/// Returns an operational limit on overflow or a caller cap breach.
pub fn original_binary_stored_parts(
    input: &[u8],
    maximum: usize,
) -> Result<[&[u8]; 2], OriginalValueBuildError> {
    original_binary_stored_len(input, maximum)?;
    let prefix = if input.starts_with(b"FSV2") {
        b"FSV2\x01".as_slice()
    } else {
        b"".as_slice()
    };
    Ok([prefix, input])
}
