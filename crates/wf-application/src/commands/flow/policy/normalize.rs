//! `RetryPolicy.normalize_flow_policy/3` over a merged policy map: the same
//! checks in the same order, with the same `ERR` texts.

use std::collections::BTreeMap;

use wf_domain::policy::{Backoff, RetryOverride};

use super::super::FlowError;
use super::super::meta;
use super::super::term::Term;
use super::{FlowPolicy, Mode, Retention, StatePolicy, backoff_kind, check_size};

const MAX_RETRIES: i64 = 1_000;
const MAX_DELAY_MS: i64 = 2_592_000_000;
const MAX_RETENTION_TTL_MS: i64 = 31_536_000_000;
const MAX_ACTIVE_MS: i64 = 31_536_000_000;
const MAX_HISTORY_MAX_EVENTS: i64 = 1_000_000;
const MAX_INDEXED_ATTRIBUTES: usize = 3;

type Entries = [(Vec<u8>, Term)];

fn rejected(text: &str) -> FlowError {
    FlowError::rejected(text.to_owned())
}

fn get<'a>(entries: &'a Entries, key: &str) -> Option<&'a Term> {
    entries
        .iter()
        .find(|(name, _)| name.as_slice() == key.as_bytes())
        .map(|(_, value)| value)
}

/// The entries of a map, or of an empty list (an empty keyword list).
fn map_like(value: &Term) -> Option<&Entries> {
    match value {
        Term::Map(entries) => Some(entries),
        Term::List(items) if items.is_empty() => Some(&[]),
        _ => None,
    }
}

fn int_in(value: &Term, low: i64, high: i64) -> Option<u64> {
    match value {
        Term::Int(value) if (low..=high).contains(value) => u64::try_from(*value).ok(),
        _ => None,
    }
}

fn backoff(value: &Term) -> Result<Backoff, FlowError> {
    let entries = match value {
        Term::Nil => &[][..],
        other => map_like(other)
            .ok_or_else(|| rejected("ERR flow retry backoff must be a map or keyword list"))?,
    };
    let defaults = Backoff::default();
    let kind = match get(entries, "kind") {
        None => defaults.kind,
        Some(Term::Bin(text)) => backoff_kind(text).ok_or_else(kind_error)?,
        Some(_) => return Err(kind_error()),
    };
    let delay = |key: &str, default: u64| match get(entries, key) {
        None => Ok(default),
        Some(value) => int_in(value, 0, MAX_DELAY_MS).ok_or_else(|| {
            rejected(&format!(
                "ERR flow retry {key} must be between 0 and {MAX_DELAY_MS}"
            ))
        }),
    };
    let base_ms = delay("base_ms", defaults.base_ms)?;
    let max_ms = delay("max_ms", defaults.max_ms)?;
    let jitter_pct = match get(entries, "jitter_pct") {
        None => defaults.jitter_pct,
        Some(value) => int_in(value, 0, 100)
            .and_then(|value| u8::try_from(value).ok())
            .ok_or_else(|| rejected("ERR flow retry jitter_pct must be between 0 and 100"))?,
    };
    Ok(Backoff {
        kind,
        base_ms,
        max_ms,
        jitter_pct,
    })
}

fn kind_error() -> FlowError {
    rejected("ERR flow retry backoff kind must be none, fixed, linear, or exponential")
}

/// `normalize_override/1`: only the given fields; a given backoff is full.
///
/// # Errors
/// The Elixir server's refusal text for an invalid field.
pub(crate) fn retry_override(value: &Term) -> Result<Option<RetryOverride>, FlowError> {
    if matches!(value, Term::Nil) {
        return Ok(None);
    }
    let entries = map_like(value)
        .ok_or_else(|| rejected("ERR flow retry policy must be a map or keyword list"))?;
    if get(entries, "max_attempts").is_some() {
        return Err(rejected(
            "ERR flow retry max_attempts was renamed to max_retries",
        ));
    }
    let max_retries = get(entries, "max_retries")
        .map(|value| {
            int_in(value, 0, MAX_RETRIES)
                .and_then(|value| u16::try_from(value).ok())
                .ok_or_else(|| {
                    rejected(&format!(
                        "ERR flow retry max_retries must be between 0 and {MAX_RETRIES}"
                    ))
                })
        })
        .transpose()?;
    let backoff = get(entries, "backoff").map(backoff).transpose()?;
    let exhausted_to = match get(entries, "exhausted_to") {
        None => None,
        Some(Term::Bin(state)) if state.as_slice() == b"running" => {
            return Err(rejected("ERR flow retry exhausted_to cannot be running"));
        }
        Some(Term::Bin(state)) if !state.is_empty() => Some(state.clone()),
        Some(_) => {
            return Err(rejected(
                "ERR flow retry exhausted_to must be a non-empty string",
            ));
        }
    };
    Ok(Some(RetryOverride {
        max_retries,
        backoff,
        exhausted_to,
    }))
}

fn retention(value: &Term) -> Result<Option<Retention>, FlowError> {
    if matches!(value, Term::Nil) {
        return Ok(None);
    }
    let entries = map_like(value)
        .ok_or_else(|| rejected("ERR flow retention policy must be a map or keyword list"))?;
    let ttl_ms = get(entries, "ttl_ms")
        .map(|value| {
            int_in(value, 1, MAX_RETENTION_TTL_MS).ok_or_else(|| {
                rejected(&format!(
                    "ERR flow retention ttl_ms must be between 1 and {MAX_RETENTION_TTL_MS}"
                ))
            })
        })
        .transpose()?;
    if get(entries, "history_hot_max_events").is_some() {
        return Err(rejected(
            "ERR flow retention history_hot_max_events is internal",
        ));
    }
    let history_max_events = get(entries, "history_max_events")
        .map(|value| {
            int_in(value, 1, MAX_HISTORY_MAX_EVENTS).ok_or_else(|| {
                rejected(&format!(
                    "ERR flow retention history_max_events must be between 1 and {MAX_HISTORY_MAX_EVENTS}"
                ))
            })
        })
        .transpose()?;
    Ok(Some(Retention {
        ttl_ms,
        history_max_events,
    }))
}

fn max_active_ms(value: &Term) -> Result<Option<u64>, FlowError> {
    match value {
        Term::Nil => Ok(None),
        Term::Bin(text) if text.as_slice() == b"infinity" || text.as_slice() == b"INFINITY" => {
            Ok(None)
        }
        other => int_in(other, 1, MAX_ACTIVE_MS).map(Some).ok_or_else(|| {
            rejected(&format!(
                "ERR flow max_active_ms must be between 1 and {MAX_ACTIVE_MS} or infinity"
            ))
        }),
    }
}

/// `Attributes.normalize_indexed_names/1`.
fn indexed_attributes(value: &Term) -> Result<Vec<String>, FlowError> {
    let items = match value {
        Term::Nil => return Ok(Vec::new()),
        Term::List(items) => items,
        _ => return Err(rejected("ERR flow indexed_attributes must be a list")),
    };
    let mut names: Vec<String> = Vec::new();
    for item in items {
        let Term::Bin(raw) = item else {
            return Err(rejected("ERR flow attribute key must be a string"));
        };
        let name = meta::attribute_key(raw)?;
        if !names.contains(&name) {
            names.push(name);
        }
    }
    if names.len() > MAX_INDEXED_ATTRIBUTES {
        return Err(rejected(&format!(
            "ERR flow indexed_attributes supports at most {MAX_INDEXED_ATTRIBUTES} keys"
        )));
    }
    Ok(names)
}

/// `StateMeta.normalize_indexed_key/1`: one key at most.
fn indexed_state_meta(value: &Term) -> Result<Option<String>, FlowError> {
    let invalid = || rejected("ERR flow indexed_state_meta must be a string or one-item list");
    let key = |item: &Term| match item {
        Term::Nil => Ok(None),
        Term::Bin(raw) if raw.is_empty() => Ok(None),
        Term::Bin(raw) => meta::state_meta_key(raw).map(Some),
        Term::List(items) if items.is_empty() => Ok(None),
        _ => Err(invalid()),
    };
    let Term::List(items) = value else {
        return key(value);
    };
    let mut keys: Vec<String> = Vec::new();
    for item in items {
        if let Some(name) = key(item)?
            && !keys.contains(&name)
        {
            keys.push(name);
        }
    }
    if keys.len() > 1 {
        return Err(rejected(
            "ERR flow indexed_state_meta supports at most 1 key",
        ));
    }
    Ok(keys.into_iter().next())
}

fn state_policy(entries: &Entries) -> Result<StatePolicy, FlowError> {
    if get(entries, "max_active_ms").is_some() {
        return Err(rejected("ERR flow max_active_ms is type-level only"));
    }
    let mode = match get(entries, "mode") {
        None => None,
        Some(Term::Bin(text)) => match text.to_ascii_lowercase().as_slice() {
            b"parallel" => Some(Mode::Parallel),
            b"fifo" => Some(Mode::Fifo),
            _ => return Err(rejected("ERR flow state mode must be parallel or fifo")),
        },
        Some(_) => return Err(rejected("ERR flow state mode must be parallel or fifo")),
    };
    let retry = get(entries, "retry")
        .map(retry_override)
        .transpose()?
        .flatten();
    let retention = get(entries, "retention")
        .map(retention)
        .transpose()?
        .flatten();
    match get(entries, "governance") {
        None | Some(Term::Nil) => {}
        Some(_) => {
            return Err(rejected(
                "ERR flow policy governance is not supported by this server yet",
            ));
        }
    }
    Ok(StatePolicy {
        mode,
        retry,
        retention,
    })
}

fn states(value: Option<&Term>) -> Result<BTreeMap<Vec<u8>, StatePolicy>, FlowError> {
    let entries = match value {
        None => return Ok(BTreeMap::new()),
        Some(Term::Map(entries)) => entries.as_slice(),
        Some(Term::List(items)) if items.is_empty() => &[][..],
        Some(Term::List(_)) => {
            return Err(rejected(
                "ERR flow policy states must be a map, keyword list, or state-policy pair list",
            ));
        }
        Some(_) => {
            return Err(rejected(
                "ERR flow policy states must be a map or keyword list",
            ));
        }
    };
    // Elixir walks a map in key order; the first invalid state answers.
    let mut sorted: Vec<&(Vec<u8>, Term)> = entries.iter().collect();
    sorted.sort_by(|left, right| left.0.cmp(&right.0));
    let mut states = BTreeMap::new();
    for (state, policy) in sorted {
        if state.as_slice() == b"running" {
            return Err(rejected("ERR flow policy state cannot be running"));
        }
        if state.is_empty() {
            return Err(rejected("ERR flow policy state must be a non-empty string"));
        }
        let fields = map_like(policy)
            .ok_or_else(|| rejected("ERR flow state policy must be a map or keyword list"))?;
        states.insert(state.clone(), state_policy(fields)?);
    }
    Ok(states)
}

/// Normalizes a merged policy map.
///
/// # Errors
/// The Elixir server's refusal text for the first invalid field.
pub fn normalize(attrs: &Entries) -> Result<FlowPolicy, FlowError> {
    if get(attrs, "mode").is_some() {
        return Err(rejected("ERR flow state mode is state-level only"));
    }
    let max_active_ms = get(attrs, "max_active_ms")
        .map(max_active_ms)
        .transpose()?
        .flatten();
    let retry = get(attrs, "retry")
        .map(retry_override)
        .transpose()?
        .flatten();
    let retention = get(attrs, "retention")
        .map(retention)
        .transpose()?
        .flatten();
    let indexed_attributes = get(attrs, "indexed_attributes")
        .map(indexed_attributes)
        .transpose()?;
    let indexed_state_meta = get(attrs, "indexed_state_meta")
        .map(indexed_state_meta)
        .transpose()?
        .flatten();
    let states = states(get(attrs, "states"))?;
    let policy = FlowPolicy {
        max_active_ms,
        retention,
        retry,
        states,
        indexed_attributes,
        indexed_state_meta,
    };
    check_size(&policy.term())?;
    Ok(policy)
}
