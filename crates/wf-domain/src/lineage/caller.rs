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

use crate::policy::complete::RawFlowPolicyValue as Raw;

/// Rejecting baseline for the source's shallow nonempty-list guard.
///
/// # Errors
/// Returns the source refusal until the separately tested body is implemented.
pub fn validate_children(_children: Raw<'_>) -> Result<(), &'static str> {
    Err("ERR flow children must be a non-empty list")
}

/// Rejecting baseline for binary parent IDs and captured child-map rows.
///
/// Map rows have unique actual keys; map-shape classification happens outside.
/// # Errors
/// Returns the source refusal until the separately tested body is implemented.
pub fn validate_no_parent_child_id<'v>(
    _parent_id: &[u8],
    _child_attrs: &[&[(Raw<'v>, Raw<'v>)]],
) -> Result<(), &'static str> {
    Err("ERR flow child id must differ from parent id")
}

/// Rejecting baseline for ordered, captured atom-key exhaustion options.
///
/// Returned states will borrow opaque binary inputs; this entry owns no map.
/// # Errors
/// Returns the source refusal until the separately tested body is implemented.
pub fn exhaust_to_opts<'v>(
    _opts: &[(&str, Raw<'v>)],
) -> Result<(&'v [u8], &'v [u8]), &'static str> {
    Err("ERR flow exhaust_to must include success and failure states")
}
