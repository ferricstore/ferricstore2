//! Immutable captured-row updates; source map iteration order is not represented.

use std::borrow::Cow;
use std::collections::TryReserveError;

/// No-op baseline for nil-skipping keyword replacement.
///
/// Some values represent non-nil captured values; nil is the outer None.
/// # Errors
/// The implemented body may return an output-row allocation error.
pub fn maybe_put_keyword<'a, T: Copy>(
    rows: &'a [(&'a str, T)],
    _key: &'a str,
    _value: Option<T>,
) -> Result<Cow<'a, [(&'a str, T)]>, TryReserveError> {
    Ok(Cow::Borrowed(rows))
}

/// No-op baseline for nil-skipping updates of unique-key map row views.
///
/// Row ordering is a Rust representation choice, not captured map traversal.
/// # Errors
/// The implemented body may return an output-row allocation error.
pub fn maybe_put_attr<'a, T: Copy>(
    rows: &'a [(&'a str, T)],
    _key: &'a str,
    _value: Option<T>,
) -> Result<Cow<'a, [(&'a str, T)]>, TryReserveError> {
    Ok(Cow::Borrowed(rows))
}

/// No-op baseline for strict-default omission on unique-key map row views.
///
/// Eq must be pure, deterministic and source-strict for the captured T. It may
/// allocate; Copy and Eq alone imply no total allocation or inline-size bound.
/// T can itself be optional: a nil unequal to the default must be inserted.
/// # Errors
/// The implemented body may return an output-row allocation error.
pub fn maybe_put_default_attr<'a, T: Copy + Eq>(
    rows: &'a [(&'a str, T)],
    _key: &'a str,
    _value: T,
    _default: T,
) -> Result<Cow<'a, [(&'a str, T)]>, TryReserveError> {
    Ok(Cow::Borrowed(rows))
}
