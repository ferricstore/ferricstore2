//! Borrowed source option validation, distinct from mutation admission.
//!
//! Rows are already captured proper atom-key options. Their order and duplicate
//! keys are preserved. These helpers confer no parsing or admission authority.

use std::fmt;

use crate::policy::complete::RawFlowPolicyValue as Raw;

mod put;

pub use put::{maybe_put_attr, maybe_put_default_attr, maybe_put_keyword};

const MAX_REF_SIZE: usize = 4_096;
const MAX_EXACT_INTEGER: u64 = 9_007_199_254_740_991;

/// Source diagnosis borrowing only the captured option name.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OptionsError<'key> {
    key: &'key str,
    kind: OptionsErrorKind,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum OptionsErrorKind {
    Required,
    NonEmpty,
    String,
    NonNeg,
    Ceiling,
    Boolean,
    RefTooLarge,
}

impl fmt::Display for OptionsError<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let key = self.key;
        match self.kind {
            OptionsErrorKind::Required => write!(formatter, "ERR flow {key} is required"),
            OptionsErrorKind::NonEmpty => {
                write!(formatter, "ERR flow {key} must be a non-empty string")
            }
            OptionsErrorKind::String => write!(formatter, "ERR flow {key} must be a string"),
            OptionsErrorKind::NonNeg => {
                write!(formatter, "ERR flow {key} must be a non-negative integer")
            }
            OptionsErrorKind::Ceiling => {
                write!(
                    formatter,
                    "ERR flow {key} exceeds maximum {MAX_EXACT_INTEGER}"
                )
            }
            OptionsErrorKind::Boolean => write!(formatter, "ERR flow {key} must be a boolean"),
            OptionsErrorKind::RefTooLarge => {
                write!(
                    formatter,
                    "ERR flow {key} too large (max {MAX_REF_SIZE} bytes)"
                )
            }
        }
    }
}

impl std::error::Error for OptionsError<'_> {}

/// Rejecting baseline for required nonempty opaque binary options.
///
/// # Errors
/// Returns the source required-field diagnosis until the tested body is added.
pub fn required_binary<'key, 'value>(
    _opts: &[(&str, Raw<'value>)],
    key: &'key str,
) -> Result<&'value [u8], OptionsError<'key>> {
    Err(OptionsError {
        key,
        kind: OptionsErrorKind::Required,
    })
}

/// Rejecting baseline for an optional nonempty binary and its raw fallback.
///
/// # Errors
/// Returns the source nonempty-string diagnosis before implementation.
pub fn optional_binary<'key, 'value>(
    _opts: &[(&str, Raw<'value>)],
    key: &'key str,
    _default: Raw<'value>,
) -> Result<&'value [u8], OptionsError<'key>> {
    Err(OptionsError {
        key,
        kind: OptionsErrorKind::NonEmpty,
    })
}

/// Rejecting baseline for optional binary-or-nil selection.
///
/// # Errors
/// Returns the source string diagnosis before implementation.
pub fn optional_binary_or_nil<'key, 'value>(
    _opts: &[(&str, Raw<'value>)],
    key: &'key str,
    _default: Raw<'value>,
) -> Result<Option<&'value [u8]>, OptionsError<'key>> {
    Err(OptionsError {
        key,
        kind: OptionsErrorKind::String,
    })
}

/// Rejecting baseline for a required nonnegative exact source integer.
///
/// # Errors
/// Returns the source integer diagnosis before implementation.
pub fn required_non_neg_integer<'key>(
    _opts: &[(&str, Raw<'_>)],
    key: &'key str,
) -> Result<u64, OptionsError<'key>> {
    Err(OptionsError {
        key,
        kind: OptionsErrorKind::NonNeg,
    })
}

/// Rejecting baseline for integer fallback selection and absent nil.
///
/// # Errors
/// Returns the source ceiling diagnosis before implementation.
pub fn optional_non_neg_integer<'key>(
    _opts: &[(&str, Raw<'_>)],
    key: &'key str,
    _default: Raw<'_>,
) -> Result<Option<u64>, OptionsError<'key>> {
    Err(OptionsError {
        key,
        kind: OptionsErrorKind::Ceiling,
    })
}

/// Rejecting baseline for captured boolean atoms and a raw fallback.
///
/// # Errors
/// Returns the source boolean diagnosis before implementation.
pub fn optional_boolean<'key>(
    _opts: &[(&str, Raw<'_>)],
    key: &'key str,
    _default: Raw<'_>,
) -> Result<bool, OptionsError<'key>> {
    Err(OptionsError {
        key,
        kind: OptionsErrorKind::Boolean,
    })
}

/// Rejecting baseline for already classified nil-or-binary references.
///
/// # Errors
/// Returns the source reference-size diagnosis before implementation.
pub fn validate_ref_size<'key>(
    key: &'key str,
    _value: Option<&[u8]>,
) -> Result<(), OptionsError<'key>> {
    Err(OptionsError {
        key,
        kind: OptionsErrorKind::RefTooLarge,
    })
}
