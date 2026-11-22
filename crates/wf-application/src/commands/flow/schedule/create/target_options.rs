//! A schedule's target options: the known keys, `type` required and
//! `state` defaulting to `queued`, and the id rules of each kind.

use super::super::super::FlowError;
use super::super::super::term::{Term, map};
use super::super::FLOW_ID_PREFIX;
use super::super::definition::Kind;
use super::{non_neg, rejected};

const TARGET_KEYS: [&str; 14] = [
    "correlation_id",
    "id",
    "id_prefix",
    "parent_flow_id",
    "partition_key",
    "payload",
    "payload_ref",
    "priority",
    "root_flow_id",
    "run_at_ms",
    "state",
    "type",
    "value_refs",
    "values",
];

/// Elixir's default bound on a target's inline payload and values.
const INLINE_MAX_BYTES: usize = 8 * 1024;

/// The normalized target: a map (an empty list is an empty one), known
/// keys only, `type` required, `state` defaulting to `queued`.
pub(super) fn normalize_target(options: &Term) -> Result<Term, FlowError> {
    let entries = match options.get(b"target") {
        Some(Term::Map(entries)) => entries.as_slice(),
        Some(Term::List(items)) if items.is_empty() => &[],
        Some(Term::List(_)) => {
            return Err(rejected(
                "ERR flow schedule target must be a keyword list or map",
            ));
        }
        _ => return Err(rejected("ERR flow schedule target is required")),
    };
    if let Some((key, _)) = entries.iter().find(|(key, _)| {
        !TARGET_KEYS
            .iter()
            .any(|name| name.as_bytes() == key.as_slice())
    }) {
        return Err(rejected(format!(
            "ERR unsupported flow schedule target field {}",
            String::from_utf8_lossy(key)
        )));
    }
    let text = |key: &str, required: bool| -> Result<Option<Term>, FlowError> {
        match entries
            .iter()
            .find(|(name, _)| name.as_slice() == key.as_bytes())
        {
            Some((_, Term::Bin(value))) if !value.is_empty() => Ok(Some(Term::Bin(value.clone()))),
            None | Some((_, Term::Nil)) if !required => Ok(None),
            _ => Err(rejected(format!(
                "ERR flow schedule {key} must be a non-empty string"
            ))),
        }
    };
    let mut target: Vec<(&str, Term)> = vec![
        ("type", text("type", true)?.unwrap_or(Term::Nil)),
        (
            "state",
            text("state", false)?.unwrap_or_else(|| Term::text("queued")),
        ),
    ];
    for key in ["id", "id_prefix", "partition_key"] {
        target.extend(text(key, false)?.map(|value| (key, value)));
    }
    let run_at = entries
        .iter()
        .find(|(name, _)| name.as_slice() == b"run_at_ms")
        .map(|(_, value)| value);
    target.extend(non_neg(run_at, "run_at_ms")?.map(|at| ("run_at_ms", Term::int(at))));
    for key in [
        "priority",
        "correlation_id",
        "parent_flow_id",
        "root_flow_id",
        "payload",
        "payload_ref",
        "values",
        "value_refs",
    ] {
        if let Some((_, value)) = entries
            .iter()
            .find(|(name, _)| name.as_slice() == key.as_bytes())
        {
            target.push((key, value.clone()));
        }
    }
    target.sort_by_key(|(key, _)| *key);
    Ok(map(target))
}

/// A target's inline payload and values stay within
/// [`INLINE_MAX_BYTES`] encoded; larger ones go by reference.
fn inline_values(target: &Term) -> Result<(), FlowError> {
    for key in ["payload", "values"] {
        if let Some(value) = target.get(key.as_bytes()) {
            let mut encoded = Vec::new();
            let fits = value.encode(&mut encoded).is_some() && encoded.len() <= INLINE_MAX_BYTES;
            if !fits {
                return Err(rejected(format!(
                    "ERR flow schedule {key} too large; use {key}_ref/value_refs"
                )));
            }
        }
    }
    Ok(())
}

pub(super) fn target_rules(kind: Kind, target: &Term) -> Result<(), FlowError> {
    let has = |key: &[u8]| target.get(key).is_some();
    if has(b"id") && has(b"id_prefix") {
        return Err(rejected(
            "ERR flow schedule target cannot set both id and id_prefix",
        ));
    }
    if kind.recurring() && has(b"id") {
        return Err(rejected(
            "ERR recurring schedule target must use id_prefix, not id",
        ));
    }
    for (key, label) in [("id", "id"), ("id_prefix", "id_prefix")] {
        if let Some(Term::Bin(value)) = target.get(key.as_bytes())
            && value.starts_with(FLOW_ID_PREFIX.as_bytes())
        {
            return Err(rejected(format!(
                "ERR scheduled target {label} is reserved for internal use"
            )));
        }
    }
    inline_values(target)
}
