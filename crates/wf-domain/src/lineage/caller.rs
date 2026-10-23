//! Borrowed child-failure options, separate from child-group application.

use std::fmt;

use super::ChildFailure;

/// One captured value in the finite child-failure option vocabulary.
#[derive(Clone, Copy)]
pub enum ChildFailureValue<'a> {
    /// An already captured source atom, without binary alias normalization.
    Atom(ChildFailure),
    /// A borrowed valid UTF-8 binary policy name.
    Binary(&'a str),
    /// A supplied value outside the supported atom and binary vocabulary.
    Other,
}

/// A selected value is unsupported or absent from the allowed policy set.
#[derive(Debug)]
pub struct ChildPolicyError<'a> {
    key: &'a str,
}

impl fmt::Display for ChildPolicyError<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let key = self.key;
        write!(formatter, "ERR flow {key} has unsupported value")
    }
}

impl std::error::Error for ChildPolicyError<'_> {}

/// Selects the first exact option key, normalizes it, then checks membership.
///
/// Only an absent key uses `default`; a supplied unsupported value refuses.
/// The option order and caller bytes remain unchanged, and nothing is copied.
/// # Errors
/// Returns the source key diagnostic for an unsupported or disallowed value.
pub fn optional_child_failure<'key, 'value>(
    opts: &[(&str, ChildFailureValue<'value>)],
    key: &'key str,
    default: ChildFailureValue<'value>,
    allowed: &[ChildFailure],
) -> Result<ChildFailure, ChildPolicyError<'key>> {
    let value = opts
        .iter()
        .find_map(|(name, value)| (*name == key).then_some(*value))
        .unwrap_or(default);
    normalize_child_failure_value(value)
        .filter(|value| allowed.contains(value))
        .ok_or(ChildPolicyError { key })
}

fn normalize_child_failure_value(value: ChildFailureValue<'_>) -> Option<ChildFailure> {
    match value {
        ChildFailureValue::Atom(value) => Some(value),
        ChildFailureValue::Binary(value) if value.eq_ignore_ascii_case("ignore") => {
            Some(ChildFailure::Ignore)
        }
        ChildFailureValue::Binary(value)
            if value.eq_ignore_ascii_case("fail_parent")
                || value.eq_ignore_ascii_case("fail-parent") =>
        {
            Some(ChildFailure::FailParent)
        }
        ChildFailureValue::Binary(_) | ChildFailureValue::Other => None,
    }
}
