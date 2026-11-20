//! `FLOW.POLICY.SET` options as text, as `parse_flow_policy_set_options/6`
//! reads them: type-level options, retry and backoff fields, retention
//! fields, and `STATE <name>` sections that run to the next `STATE`.

use super::super::FlowError;
use super::super::term::Term;
use super::options::{Options, boolean, int, not_integer, syntax, upper_is, word};

/// Where one option goes.
enum Scope {
    Policy,
    Retry,
    Backoff,
    Retention,
}

fn positive(value: &Term) -> Result<Term, FlowError> {
    int(value)
        .filter(|int| *int > 0)
        .map(Term::Int)
        .ok_or_else(not_integer)
}

fn non_negative(value: &Term) -> Result<Term, FlowError> {
    int(value)
        .filter(|int| *int >= 0)
        .map(Term::Int)
        .ok_or_else(not_integer)
}

/// A flat JSON array of strings; `None` when `text` is not exactly one.
fn json_strings(text: &str) -> Option<Vec<Term>> {
    let inner = text.strip_prefix('[')?.strip_suffix(']')?.trim();
    if inner.is_empty() {
        return Some(Vec::new());
    }
    inner
        .split(',')
        .map(|item| {
            let item = item.trim().strip_prefix('"')?.strip_suffix('"')?;
            (!item.contains(['"', '\\'])).then(|| Term::text(item))
        })
        .collect()
}

/// `flow_policy_indexed_attributes/1`.
fn indexed_attributes(value: &Term) -> Term {
    let Some(raw) = word(value) else {
        return match value {
            Term::List(_) => value.clone(),
            other => Term::List(vec![other.clone()]),
        };
    };
    let text = String::from_utf8_lossy(raw);
    let trimmed = text.trim();
    if trimmed.starts_with('[') {
        return Term::List(json_strings(trimmed).unwrap_or_else(|| vec![value.clone()]));
    }
    if text.contains(',') {
        return Term::List(
            text.split(',')
                .map(str::trim)
                .filter(|item| !item.is_empty())
                .map(Term::text)
                .collect(),
        );
    }
    Term::List(vec![value.clone()])
}

/// `parse_flow_policy_option/2`.
fn option(name: &Term, value: &Term) -> Result<(Scope, &'static str, Term), FlowError> {
    let token = word(name)
        .map(|bytes| String::from_utf8_lossy(bytes).to_ascii_uppercase())
        .ok_or_else(syntax)?;
    let lower = |kind: &str| Term::text(&kind.to_ascii_lowercase());
    Ok(match token.as_str() {
        "RETENTION_TTL" | "RETENTION_TTL_MS" => (Scope::Retention, "ttl_ms", positive(value)?),
        "HISTORY_MAX_EVENTS" => (Scope::Retention, "history_max_events", positive(value)?),
        "HISTORY_HOT_MAX_EVENTS" => {
            return Err(FlowError::rejected(
                "ERR flow retention history_hot_max_events is internal".to_owned(),
            ));
        }
        "MAX_ACTIVE_MS" if upper_is(value, "INFINITY") => {
            (Scope::Policy, "max_active_ms", Term::text("infinity"))
        }
        "MAX_ACTIVE_MS" => (Scope::Policy, "max_active_ms", positive(value)?),
        "EXPECTED_GENERATION" => (Scope::Policy, "expected_generation", non_negative(value)?),
        "REPLACE" => (
            Scope::Policy,
            "replace",
            Term::Bool(boolean(value).ok_or_else(|| {
                FlowError::rejected("ERR flow replace must be boolean".to_owned())
            })?),
        ),
        "INDEXED_STATE_META" => (Scope::Policy, "indexed_state_meta", value.clone()),
        "INDEXED_ATTRIBUTES" => (
            Scope::Policy,
            "indexed_attributes",
            indexed_attributes(value),
        ),
        "MODE" if upper_is(value, "PARALLEL") || upper_is(value, "FIFO") => {
            let mode = word(value).map(String::from_utf8_lossy).unwrap_or_default();
            (Scope::Policy, "mode", lower(&mode))
        }
        "MODE" => {
            return Err(FlowError::rejected(
                "ERR flow state mode must be parallel or fifo".to_owned(),
            ));
        }
        "MAX_RETRIES" => (Scope::Retry, "max_retries", non_negative(value)?),
        "EXHAUSTED_TO" => (Scope::Retry, "exhausted_to", value.clone()),
        "BACKOFF" => {
            let kind = word(value)
                .map(|bytes| String::from_utf8_lossy(bytes).to_ascii_uppercase())
                .filter(|kind| ["NONE", "FIXED", "LINEAR", "EXPONENTIAL"].contains(&kind.as_str()))
                .ok_or_else(|| {
                    FlowError::rejected(
                        "ERR flow retry backoff kind must be none, fixed, linear, or exponential"
                            .to_owned(),
                    )
                })?;
            (Scope::Backoff, "kind", lower(&kind))
        }
        "BASE_MS" => (Scope::Backoff, "base_ms", non_negative(value)?),
        "MAX_MS" => (Scope::Backoff, "max_ms", non_negative(value)?),
        "JITTER_PCT" => (Scope::Backoff, "jitter_pct", non_negative(value)?),
        _ => return Err(syntax()),
    })
}

/// Options that only a type has, refused inside a `STATE` section.
const TYPE_ONLY: [&str; 5] = [
    "indexed_state_meta",
    "indexed_attributes",
    "max_active_ms",
    "expected_generation",
    "replace",
];

/// Reads policy options into typed-map entries.
pub(super) fn options(args: &[Term]) -> Result<Options, FlowError> {
    let mut policy = Options::default();
    let (mut retry, mut backoff, mut retention) =
        (Options::default(), Options::default(), Options::default());
    let mut states = Options::default();
    let mut rest = args;
    while let Some((name, tail)) = rest.split_first() {
        let Some((value, after)) = tail.split_first() else {
            return Err(syntax());
        };
        if upper_is(name, "STATE") {
            let end = after
                .iter()
                .position(|arg| upper_is(arg, "STATE"))
                .unwrap_or(after.len());
            let (section, next) = after.split_at(end);
            let state = options(section)?;
            if let Some(key) = TYPE_ONLY.iter().find(|key| state.get(key).is_some()) {
                return Err(FlowError::rejected(format!(
                    "ERR flow {key} is type-level only"
                )));
            }
            states.merge("states", value, state.term())?;
            rest = next;
            continue;
        }
        let (scope, key, coerced) = option(name, value)?;
        match scope {
            Scope::Policy => policy.push(key, coerced),
            Scope::Retry => retry.push(key, coerced),
            Scope::Backoff => backoff.push(key, coerced),
            Scope::Retention => retention.push(key, coerced),
        }
        rest = after;
    }
    if !backoff.0.is_empty() {
        retry.push("backoff", backoff.term());
    }
    if !retry.0.is_empty() {
        policy.push("retry", retry.term());
    }
    if !retention.0.is_empty() {
        policy.push("retention", retention.term());
    }
    policy.0.extend(states.0);
    Ok(policy)
}
