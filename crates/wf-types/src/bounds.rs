//! Distinct storage identity and composed API-key byte bounds.

use crate::error::ValueError;

/// Maximum record-ID bytes accepted by the source storage identity guard.
///
/// This does not guarantee that the identifier fits inside an encoded API key.
pub const MAX_RECORD_ID_BYTES: usize = 65_535;

/// Maximum total bytes in a source API key, including routing and separators.
pub const MAX_API_KEY_BYTES: usize = 65_535;

/// Check an already encoded API key without imposing text or nonempty rules.
///
/// # Errors
/// Returns [`ValueError::ApiKeyTooLong`] when the full key exceeds 65,535 bytes.
pub fn validate_api_key_size(key: &[u8]) -> Result<(), ValueError> {
    validate_api_key_length(key.len())
}

/// Check the total length of all already encoded API-key parts.
///
/// Callers must include every routing prefix, encoded component, and separator.
/// This helper does not encode a state key or validate its ownership.
///
/// # Errors
/// Returns [`ValueError::LengthOverflow`] if the sum cannot fit in `usize`, or
/// [`ValueError::ApiKeyTooLong`] if the representable total exceeds 65,535 bytes.
pub fn checked_api_key_size(part_lengths: &[usize]) -> Result<usize, ValueError> {
    let bytes = part_lengths.iter().try_fold(0_usize, |total, part| {
        total.checked_add(*part).ok_or(ValueError::LengthOverflow)
    })?;
    validate_api_key_length(bytes)?;
    Ok(bytes)
}

fn validate_api_key_length(bytes: usize) -> Result<(), ValueError> {
    if bytes > MAX_API_KEY_BYTES {
        Err(ValueError::ApiKeyTooLong { bytes })
    } else {
        Ok(())
    }
}
