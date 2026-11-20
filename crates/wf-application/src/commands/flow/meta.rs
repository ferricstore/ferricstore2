//! Flow attributes and state metadata, normalized as the Elixir server does
//! (`Ferricstore.Flow.Attributes` and `Ferricstore.Flow.StateMeta`).
//!
//! Keys are trimmed UTF-8 text; sorted maps reproduce the key order of the
//! small Erlang maps the Elixir server replies with.

use std::collections::BTreeMap;

use super::FlowError;
use super::term::Term;

/// Attribute name to value: nil, a binary, an integer, a float, a boolean or
/// a list of binaries.
pub type Attributes = BTreeMap<String, Term>;
/// State name to its metadata entries; values are scalars.
pub type StateMeta = BTreeMap<String, BTreeMap<String, Term>>;

const MAX_ATTRIBUTES: usize = 16;
const MAX_KEY_BYTES: usize = 64;
const MAX_VALUE_BYTES: usize = 256;
const MAX_ATTRIBUTE_BYTES: usize = 2_048;
const MAX_LIST_VALUES: usize = 16;
const MAX_STATES: usize = 64;
const MAX_ENTRIES_PER_STATE: usize = 16;
const MAX_STATE_BYTES: usize = 64;
const MAX_STATE_META_BYTES: usize = 16_384;

fn rejected(text: &str) -> FlowError {
    FlowError::rejected(text.to_owned())
}

/// A trimmed, non-empty, unreserved key; `kind` is `attribute` or
/// `state_meta`.
fn key(kind: &str, raw: &[u8]) -> Result<String, FlowError> {
    let text = std::str::from_utf8(raw)
        .map_err(|_| rejected(&format!("ERR flow {kind} key must be valid UTF-8")))?;
    let text = text.trim();
    if text.is_empty() {
        Err(rejected(&format!("ERR flow {kind} key must not be empty")))
    } else if text.len() > MAX_KEY_BYTES {
        Err(rejected(&format!("ERR flow {kind} key too large")))
    } else if text.starts_with("__") {
        Err(rejected(&format!("ERR flow {kind} key is reserved")))
    } else {
        Ok(text.to_owned())
    }
}

/// An attribute name to delete, normalized as a key.
///
/// # Errors
/// The Elixir server's refusal text for an invalid key.
pub fn attribute_key(raw: &[u8]) -> Result<String, FlowError> {
    key("attribute", raw)
}

/// A `state_meta` key, normalized as `StateMeta.normalize_name/1` does.
///
/// # Errors
/// The Elixir server's refusal text for an invalid key.
pub fn state_meta_key(raw: &[u8]) -> Result<String, FlowError> {
    key("state_meta", raw)
}

/// Applies a merge and a delete to attributes, checking the totals again.
///
/// # Errors
/// The Elixir server's refusal text when the result is too large.
pub fn update_attributes(
    current: &Attributes,
    merge: &Attributes,
    delete: &[String],
) -> Result<Attributes, FlowError> {
    let mut updated = current.clone();
    updated.extend(
        merge
            .iter()
            .map(|(name, value)| (name.clone(), value.clone())),
    );
    for name in delete {
        updated.remove(name);
    }
    let as_term = attributes_term(&updated);
    attributes(Some(&as_term))
}

fn scalar(kind: &str, value: &Term) -> Result<Term, FlowError> {
    match value {
        Term::Bin(bytes) if bytes.len() > MAX_VALUE_BYTES => {
            Err(rejected(&format!("ERR flow {kind} value too large")))
        }
        Term::Float(number) if !number.is_finite() => {
            Err(rejected(&format!("ERR flow {kind} float must be finite")))
        }
        Term::Nil | Term::Bool(_) | Term::Int(_) | Term::Float(_) | Term::Bin(_) => {
            Ok(value.clone())
        }
        Term::List(_) | Term::Map(_) => Err(rejected(&format!(
            "ERR flow {kind} value must be {}",
            if kind == "attribute" {
                "scalar or string list"
            } else {
                "scalar"
            }
        ))),
    }
}

fn attribute_value(value: &Term) -> Result<Term, FlowError> {
    let Term::List(items) = value else {
        return scalar("attribute", value);
    };
    if items.is_empty() {
        return Err(rejected("ERR flow attribute list must not be empty"));
    }
    if items.len() > MAX_LIST_VALUES {
        return Err(rejected("ERR flow attribute list too large"));
    }
    let mut unique: Vec<Term> = Vec::new();
    for item in items {
        let Term::Bin(_) = item else {
            return Err(rejected("ERR flow attribute list values must be strings"));
        };
        let item = scalar("attribute", item)?;
        if !unique.contains(&item) {
            unique.push(item);
        }
    }
    Ok(Term::List(unique))
}

fn value_size(value: &Term) -> usize {
    match value {
        Term::Bin(bytes) => bytes.len(),
        Term::Int(_) | Term::Float(_) => 8,
        Term::Bool(_) => 1,
        Term::List(items) => items.iter().map(value_size).sum(),
        Term::Nil | Term::Map(_) => 0,
    }
}

fn entries<'a>(value: Option<&'a Term>, error: &str) -> Result<&'a [(Vec<u8>, Term)], FlowError> {
    match value {
        None | Some(Term::Nil) => Ok(&[]),
        Some(Term::Map(entries)) => Ok(entries),
        Some(_) => Err(rejected(error)),
    }
}

/// `Attributes.normalize/1` of the `attributes` option.
///
/// # Errors
/// The Elixir server's refusal text for an invalid value.
pub fn attributes(value: Option<&Term>) -> Result<Attributes, FlowError> {
    let entries = entries(value, "ERR flow attributes must be a map")?;
    if entries.len() > MAX_ATTRIBUTES {
        return Err(rejected("ERR too many flow attributes"));
    }
    let mut normalized = Attributes::new();
    for (name, value) in entries {
        normalized.insert(key("attribute", name)?, attribute_value(value)?);
    }
    let total: usize = normalized
        .iter()
        .map(|(name, value)| name.len().saturating_add(value_size(value)))
        .sum();
    if total > MAX_ATTRIBUTE_BYTES {
        return Err(rejected("ERR flow attributes too large"));
    }
    Ok(normalized)
}

/// The entries of one state's metadata, as sent in `state_meta`.
///
/// # Errors
/// The Elixir server's refusal text for an invalid value.
pub fn state_meta_update(value: Option<&Term>) -> Result<BTreeMap<String, Term>, FlowError> {
    let entries = entries(value, "ERR flow state_meta must be a map")?;
    if entries.len() > MAX_ENTRIES_PER_STATE {
        return Err(rejected("ERR too many flow state_meta entries"));
    }
    let mut normalized = BTreeMap::new();
    for (name, value) in entries {
        normalized.insert(key("state_meta", name)?, scalar("state_meta", value)?);
    }
    Ok(normalized)
}

/// Merges `update` into the metadata of `state`, checking the totals.
///
/// # Errors
/// The Elixir server's refusal text for an invalid value.
pub fn merge_state_meta(
    current: &StateMeta,
    state: &[u8],
    update: &BTreeMap<String, Term>,
) -> Result<StateMeta, FlowError> {
    let mut merged = current.clone();
    if update.is_empty() {
        return Ok(merged);
    }
    let state = std::str::from_utf8(state)
        .map_err(|_| rejected("ERR flow state_meta state must be valid UTF-8"))?
        .trim();
    if state.is_empty() {
        return Err(rejected("ERR flow state_meta state must not be empty"));
    }
    if state.len() > MAX_STATE_BYTES {
        return Err(rejected("ERR flow state_meta state too large"));
    }
    merged.entry(state.to_owned()).or_default().extend(
        update
            .iter()
            .map(|(name, value)| (name.clone(), value.clone())),
    );
    if merged.len() > MAX_STATES {
        return Err(rejected("ERR too many flow state_meta states"));
    }
    if merged
        .values()
        .any(|meta| meta.len() > MAX_ENTRIES_PER_STATE)
    {
        return Err(rejected("ERR too many flow state_meta entries"));
    }
    let total: usize = merged
        .iter()
        .map(|(state, meta)| {
            meta.iter()
                .map(|(name, value)| name.len().saturating_add(value_size(value)))
                .sum::<usize>()
                .saturating_add(state.len())
        })
        .sum();
    if total > MAX_STATE_META_BYTES {
        return Err(rejected("ERR flow state_meta too large"));
    }
    Ok(merged)
}

/// Attributes as a reply map.
#[must_use]
pub fn attributes_term(attributes: &Attributes) -> Term {
    Term::Map(
        attributes
            .iter()
            .map(|(name, value)| (name.as_bytes().to_vec(), value.clone()))
            .collect(),
    )
}

/// State metadata as a reply map.
#[must_use]
pub fn state_meta_term(state_meta: &StateMeta) -> Term {
    Term::Map(
        state_meta
            .iter()
            .map(|(state, meta)| {
                let entries = meta
                    .iter()
                    .map(|(name, value)| (name.as_bytes().to_vec(), value.clone()))
                    .collect();
                (state.as_bytes().to_vec(), Term::Map(entries))
            })
            .collect(),
    )
}
