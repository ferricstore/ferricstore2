//! Borrowed source option validation, distinct from mutation admission.
//!
//! Rows are already captured proper atom-key options. Their order and duplicate
//! keys are preserved. These helpers confer no parsing or admission authority.

use std::fmt;

use crate::policy::complete::RawFlowPolicyValue as Raw;

mod put;
mod score_bound;

pub use put::{maybe_put_attr, maybe_put_default_attr, maybe_put_keyword};
pub use score_bound::{OriginalScoreBound, OriginalScoreBoundError, parse_original_score_bound};

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

/// Selects the first exact key as a required nonempty opaque binary.
///
/// # Errors
/// Returns the source required-field or nonempty-string diagnosis.
pub fn required_binary<'key, 'value>(
    opts: &[(&str, Raw<'value>)],
    key: &'key str,
) -> Result<&'value [u8], OptionsError<'key>> {
    match first_option(opts, key) {
        Some(Raw::Binary(value)) if !value.is_empty() => Ok(value),
        None => Err(OptionsError {
            key,
            kind: OptionsErrorKind::Required,
        }),
        _ => Err(OptionsError {
            key,
            kind: OptionsErrorKind::NonEmpty,
        }),
    }
}

/// Selects a nonempty opaque binary, using the fallback only on absence.
///
/// # Errors
/// Returns the source nonempty-string diagnosis for an invalid selected value.
pub fn optional_binary<'key, 'value>(
    opts: &[(&str, Raw<'value>)],
    key: &'key str,
    default: Raw<'value>,
) -> Result<&'value [u8], OptionsError<'key>> {
    match first_option(opts, key).unwrap_or(default) {
        Raw::Binary(value) if !value.is_empty() => Ok(value),
        _ => Err(OptionsError {
            key,
            kind: OptionsErrorKind::NonEmpty,
        }),
    }
}

/// Selects a binary or nil; a present empty binary stays present.
///
/// # Errors
/// Returns the source string diagnosis for a nonbinary, nonnil selected value.
pub fn optional_binary_or_nil<'key, 'value>(
    opts: &[(&str, Raw<'value>)],
    key: &'key str,
    default: Raw<'value>,
) -> Result<Option<&'value [u8]>, OptionsError<'key>> {
    match first_option(opts, key).unwrap_or(default) {
        Raw::Nil | Raw::Atom("nil") => Ok(None),
        Raw::Binary(value) => Ok(Some(value)),
        _ => Err(OptionsError {
            key,
            kind: OptionsErrorKind::String,
        }),
    }
}

/// Selects a required integer in the source's nonnegative exact range.
///
/// # Errors
/// Returns the source missing, nonnegative-integer or ceiling diagnosis.
pub fn required_non_neg_integer<'key>(
    opts: &[(&str, Raw<'_>)],
    key: &'key str,
) -> Result<u64, OptionsError<'key>> {
    let value = first_option(opts, key).ok_or(OptionsError {
        key,
        kind: OptionsErrorKind::Required,
    })?;
    integer_value(value).map_err(|kind| OptionsError { key, kind })
}

/// Selects an exact integer; only an absent key may use a nil fallback.
///
/// # Errors
/// Returns the source nonnegative-integer or ceiling diagnosis.
pub fn optional_non_neg_integer<'key>(
    opts: &[(&str, Raw<'_>)],
    key: &'key str,
    default: Raw<'_>,
) -> Result<Option<u64>, OptionsError<'key>> {
    let value = match first_option(opts, key) {
        Some(value) => value,
        None if matches!(default, Raw::Nil | Raw::Atom("nil")) => return Ok(None),
        None => default,
    };
    integer_value(value)
        .map(Some)
        .map_err(|kind| OptionsError { key, kind })
}

/// Selects the source's boolean atoms, validating the fallback only on absence.
///
/// # Errors
/// Returns the source boolean diagnosis for a nonboolean selected value.
pub fn optional_boolean<'key>(
    opts: &[(&str, Raw<'_>)],
    key: &'key str,
    default: Raw<'_>,
) -> Result<bool, OptionsError<'key>> {
    match first_option(opts, key).unwrap_or(default) {
        Raw::Atom("true") => Ok(true),
        Raw::Atom("false") => Ok(false),
        _ => Err(OptionsError {
            key,
            kind: OptionsErrorKind::Boolean,
        }),
    }
}

/// Checks the inclusive source bound for already classified nil-or-binary refs.
///
/// # Errors
/// Returns the source reference-size diagnosis above 4,096 bytes.
pub fn validate_ref_size<'key>(
    key: &'key str,
    value: Option<&[u8]>,
) -> Result<(), OptionsError<'key>> {
    if value.is_some_and(|bytes| bytes.len() > MAX_REF_SIZE) {
        Err(OptionsError {
            key,
            kind: OptionsErrorKind::RefTooLarge,
        })
    } else {
        Ok(())
    }
}

fn first_option<'value>(opts: &[(&str, Raw<'value>)], key: &str) -> Option<Raw<'value>> {
    opts.iter()
        .find_map(|(name, value)| (*name == key).then_some(*value))
}

fn integer_value(value: Raw<'_>) -> Result<u64, OptionsErrorKind> {
    match value {
        Raw::Integer(value) if value < 0 => Err(OptionsErrorKind::NonNeg),
        Raw::Integer(value) if value > i128::from(MAX_EXACT_INTEGER) => {
            Err(OptionsErrorKind::Ceiling)
        }
        Raw::Integer(value) => u64::try_from(value).map_err(|_| OptionsErrorKind::NonNeg),
        Raw::BigInteger {
            negative,
            little_endian,
        } => {
            if negative && little_endian.iter().any(|byte| *byte != 0) {
                return Err(OptionsErrorKind::NonNeg);
            }
            let mut magnitude = 0_u64;
            for (index, byte) in little_endian.iter().copied().enumerate() {
                if index < 7 {
                    magnitude |= u64::from(byte) << (8 * index);
                } else if byte != 0 {
                    return Err(OptionsErrorKind::Ceiling);
                }
            }
            if magnitude > MAX_EXACT_INTEGER {
                Err(OptionsErrorKind::Ceiling)
            } else {
                Ok(magnitude)
            }
        }
        _ => Err(OptionsErrorKind::NonNeg),
    }
}
