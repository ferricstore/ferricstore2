//! Immutable captured-row updates; source map iteration order is not represented.

use std::borrow::Cow;
use std::collections::TryReserveError;

/// Borrows nil no-ops; otherwise removes matching keys and prepends the new pair.
///
/// Some values represent non-nil captured values; nil is the outer None.
/// # Errors
/// Returns an output-row capacity or allocation error.
pub fn maybe_put_keyword<'a, T: Copy>(
    rows: &'a [(&'a str, T)],
    key: &'a str,
    value: Option<T>,
) -> Result<Cow<'a, [(&'a str, T)]>, TryReserveError> {
    match value {
        None => Ok(Cow::Borrowed(rows)),
        Some(value) => put_rows(rows, key, value, true),
    }
}

/// Borrows nil no-ops; otherwise replaces or appends in a unique-key map view.
///
/// Row ordering is a Rust representation choice, not captured map traversal.
/// # Errors
/// Returns an output-row capacity or allocation error.
pub fn maybe_put_attr<'a, T: Copy>(
    rows: &'a [(&'a str, T)],
    key: &'a str,
    value: Option<T>,
) -> Result<Cow<'a, [(&'a str, T)]>, TryReserveError> {
    match value {
        None => Ok(Cow::Borrowed(rows)),
        Some(value) => put_rows(rows, key, value, false),
    }
}

/// Borrows strict-default no-ops; otherwise writes even a nil captured value.
///
/// Eq must be pure, deterministic and source-strict for the captured T. It may
/// allocate; Copy and Eq alone imply no total allocation or inline-size bound.
/// T can itself be optional: a nil unequal to the default must be inserted.
/// # Errors
/// Returns an output-row capacity or allocation error.
pub fn maybe_put_default_attr<'a, T: Copy + Eq>(
    rows: &'a [(&'a str, T)],
    key: &'a str,
    value: T,
    default: T,
) -> Result<Cow<'a, [(&'a str, T)]>, TryReserveError> {
    if value == default {
        Ok(Cow::Borrowed(rows))
    } else {
        put_rows(rows, key, value, false)
    }
}

fn put_rows<'a, T: Copy>(
    rows: &'a [(&'a str, T)],
    key: &'a str,
    value: T,
    keyword: bool,
) -> Result<Cow<'a, [(&'a str, T)]>, TryReserveError> {
    let kept = rows.iter().filter(|(name, _)| *name != key).count();
    // Nonzero-size row slices fit isize::MAX bytes, leaving room for one index.
    let output_count = if keyword {
        kept + 1
    } else {
        rows.len() + usize::from(kept == rows.len())
    };
    let mut output = Vec::new();
    output.try_reserve_exact(output_count)?;
    if keyword {
        output.push((key, value));
        for &(name, prior) in rows {
            if name != key {
                output.push((name, prior));
            }
        }
    } else {
        for &(name, prior) in rows {
            output.push((name, if name == key { value } else { prior }));
        }
        if kept == rows.len() {
            output.push((key, value));
        }
    }
    Ok(Cow::Owned(output))
}
