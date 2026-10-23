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

/// Checks only that the captured list has a nonempty cons prefix.
///
/// # Errors
/// Returns the source refusal for an empty list or any non-list value.
pub fn validate_children(children: Raw<'_>) -> Result<(), &'static str> {
    match children {
        Raw::List(prefix) | Raw::ImproperList { prefix, .. } if !prefix.is_empty() => Ok(()),
        _ => Err("ERR flow children must be a non-empty list"),
    }
}

/// Rejects a child whose atom-keyed ID equals the binary parent ID.
///
/// Map rows have unique actual keys; map-shape classification happens outside.
/// # Errors
/// Returns the source refusal if any child has the parent ID.
pub fn validate_no_parent_child_id<'v>(
    parent_id: &[u8],
    child_attrs: &[&[(Raw<'v>, Raw<'v>)]],
) -> Result<(), &'static str> {
    if child_attrs.iter().any(|attrs| {
        matches!(child_map_field(attrs, "id", true), Some(Raw::Binary(id)) if id == parent_id)
    }) {
        Err("ERR flow child id must differ from parent id")
    } else {
        Ok(())
    }
}

/// Selects nonempty binary exhaustion states from captured atom-key options.
///
/// Options use first-key precedence. Explicit maps prefer a complete atom-key
/// pair, then a complete binary-key pair; only nil uses top-level states.
/// Returned states borrow opaque binary inputs; this entry owns no map.
/// # Errors
/// Returns the source refusal for missing, empty or nonbinary states.
pub fn exhaust_to_opts<'v>(opts: &[(&str, Raw<'v>)]) -> Result<(&'v [u8], &'v [u8]), &'static str> {
    match child_option(opts, "exhaust_to") {
        Raw::Nil | Raw::Atom("nil") => {
            exhaust_to_states(child_option(opts, "success"), child_option(opts, "failure"))
        }
        Raw::Map(rows) => {
            if let (Some(success), Some(failure)) = (
                child_map_field(rows, "success", true),
                child_map_field(rows, "failure", true),
            ) {
                return exhaust_to_states(success, failure);
            }
            match (
                child_map_field(rows, "success", false),
                child_map_field(rows, "failure", false),
            ) {
                (Some(success), Some(failure)) => exhaust_to_states(success, failure),
                _ => Err("ERR flow exhaust_to must include success and failure states"),
            }
        }
        _ => Err("ERR flow exhaust_to must include success and failure states"),
    }
}

fn child_option<'v>(opts: &[(&str, Raw<'v>)], key: &str) -> Raw<'v> {
    opts.iter()
        .find_map(|(name, value)| (*name == key).then_some(*value))
        .unwrap_or(Raw::Nil)
}

fn child_map_field<'v>(rows: &[(Raw<'v>, Raw<'v>)], key: &str, atom_key: bool) -> Option<Raw<'v>> {
    rows.iter().find_map(|(name, value)| {
        let matches = match name {
            Raw::Atom(name) if atom_key => *name == key,
            Raw::Binary(name) if !atom_key => *name == key.as_bytes(),
            _ => false,
        };
        matches.then_some(*value)
    })
}

fn exhaust_to_states<'v>(
    success: Raw<'v>,
    failure: Raw<'v>,
) -> Result<(&'v [u8], &'v [u8]), &'static str> {
    match (success, failure) {
        (Raw::Binary(success), Raw::Binary(failure))
            if !success.is_empty() && !failure.is_empty() =>
        {
            Ok((success, failure))
        }
        _ => Err("ERR flow exhaust_to must include success and failure states"),
    }
}
