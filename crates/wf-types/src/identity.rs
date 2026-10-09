//! Opaque binary record identity and source-compatible partition encoding.

use crate::{bounds::MAX_RECORD_ID_BYTES, error::ValueError};

/// A nonempty binary identifier within the source storage identity bound.
///
/// Bytes are opaque: invalid UTF-8, nulls, and delimiters are permitted.
/// API admission must separately check the full encoded state-key length.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct RecordId(Box<[u8]>);

impl RecordId {
    /// Construct a binary identifier without text normalization.
    ///
    /// # Errors
    /// Returns [`ValueError::EmptyRecordId`] for no bytes, or
    /// [`ValueError::RecordIdTooLong`] above 65,535 bytes.
    pub fn new(bytes: Vec<u8>) -> Result<Self, ValueError> {
        if bytes.is_empty() {
            Err(ValueError::EmptyRecordId)
        } else if bytes.len() > MAX_RECORD_ID_BYTES {
            Err(ValueError::RecordIdTooLong { bytes: bytes.len() })
        } else {
            Ok(Self(bytes.into_boxed_slice()))
        }
    }

    /// Borrow the original validated bytes.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

/// A partition key encoded from at least one arbitrary binary component.
///
/// Matches the source `fpk:` format: each component contributes its decimal
/// byte length, a colon, and its unmodified bytes. Empty components are valid.
/// Encoding has no source storage-size cap; adapters enforce composed-key bounds.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PartitionKey(Vec<u8>);

impl PartitionKey {
    /// Encode a nonempty component list using source-compatible byte lengths.
    ///
    /// # Errors
    /// Returns [`ValueError::EmptyPartitionComponents`] for an empty list,
    /// [`ValueError::LengthOverflow`] for an unrepresentable encoded length,
    /// or [`ValueError::AllocationFailed`] if the output cannot be reserved.
    pub fn encode_components(components: &[&[u8]]) -> Result<Self, ValueError> {
        const PREFIX: &[u8] = b"fpk:";

        if components.is_empty() {
            return Err(ValueError::EmptyPartitionComponents);
        }

        let encoded_length = components
            .iter()
            .try_fold(PREFIX.len(), |total, component| {
                total
                    .checked_add(decimal_digits(component.len()))
                    .and_then(|length| length.checked_add(1))
                    .and_then(|length| length.checked_add(component.len()))
                    .ok_or(ValueError::LengthOverflow)
            })?;
        let mut encoded = Vec::new();
        encoded
            .try_reserve_exact(encoded_length)
            .map_err(|_| ValueError::AllocationFailed)?;
        encoded.extend_from_slice(PREFIX);
        for component in components {
            append_decimal(component.len(), &mut encoded);
            encoded.push(b':');
            encoded.extend_from_slice(component);
        }
        Ok(Self(encoded))
    }

    /// Borrow the complete `fpk:` encoding.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

fn decimal_digits(mut value: usize) -> usize {
    let mut digits = 1;
    while value >= 10 {
        value /= 10;
        digits += 1;
    }
    digits
}

fn append_decimal(mut value: usize, encoded: &mut Vec<u8>) {
    // Three decimal digits per byte cover every value of usize, including zero.
    let mut digits = [0_u8; std::mem::size_of::<usize>() * 3];
    let mut start = digits.len();
    loop {
        start -= 1;
        digits[start] = b"0123456789"[value % 10];
        value /= 10;
        if value == 0 {
            break;
        }
    }
    encoded.extend_from_slice(&digits[start..]);
}
