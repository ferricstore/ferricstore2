//! A state's governance policy, as `Governance.Policy.normalize/1` checks
//! it: the mode, effect rules, audit settings, named limit, budget and
//! circuit rules, and approval rules, each optional and kept sparse in
//! its normalized form (names as text, lists without repeats).

use super::super::FlowError;
use super::super::MAX_EXACT;
use super::super::term::Term;

mod rules;

use rules::{budget_rule, circuit_rule, limit_rule, rules};

const MAX_LIST_VALUES: usize = 1_000;
pub(super) const MAX_STRING_BYTES: usize = 65_535;
const MAX_POLICY_BYTES: usize = 1_048_576;

pub(super) type Entries = Vec<(Vec<u8>, Term)>;
/// A map's entries as given.
pub(super) type Pairs = [(Vec<u8>, Term)];
/// A named rule's normalizer.
pub(super) type Normalizer = fn(&Pairs) -> Result<Entries, FlowError>;

pub(super) fn rejected(text: impl Into<String>) -> FlowError {
    FlowError::rejected(text.into())
}

/// A map, or the empty keyword list Elixir also accepts.
pub(super) fn as_map(value: &Term) -> Option<&Pairs> {
    match value {
        Term::Map(entries) => Some(entries),
        Term::List(items) if items.is_empty() => Some(&[]),
        _ => None,
    }
}

pub(super) fn get<'a>(entries: &'a Pairs, key: &str) -> Option<&'a Term> {
    entries
        .iter()
        .find(|(name, _)| name.as_slice() == key.as_bytes())
        .map(|(_, value)| value)
        .filter(|value| **value != Term::Nil)
}

pub(super) fn put(entries: &mut Entries, key: &str, value: Option<Term>) {
    if let Some(value) = value {
        entries.push((key.as_bytes().to_vec(), value));
    }
}

pub(super) fn choice(
    value: Option<&Term>,
    choices: &[&str],
    error: &str,
) -> Result<Option<Term>, FlowError> {
    match value {
        None => Ok(None),
        Some(Term::Bin(text)) => {
            let lower = text.to_ascii_lowercase();
            choices
                .iter()
                .find(|choice| choice.as_bytes() == lower.as_slice())
                .map(|choice| Some(Term::text(choice)))
                .ok_or_else(|| rejected(error))
        }
        Some(_) => Err(rejected(error)),
    }
}

fn boolean(value: Option<&Term>, name: &str) -> Result<Option<Term>, FlowError> {
    match value {
        None => Ok(None),
        Some(Term::Bool(flag)) => Ok(Some(Term::Bool(*flag))),
        Some(_) => Err(rejected(format!(
            "ERR flow governance {name} must be boolean"
        ))),
    }
}

/// `optional_string_list/4`: repeats dropped, at most 1000 values.
pub(super) fn strings(
    value: Option<&Term>,
    name: &str,
    max_bytes: usize,
) -> Result<Option<Term>, FlowError> {
    let items = match value {
        None => return Ok(None),
        Some(Term::List(items)) => items,
        Some(_) => {
            return Err(rejected(format!(
                "ERR flow governance {name} must be a list of strings"
            )));
        }
    };
    let mut kept: Vec<Term> = Vec::new();
    for (count, item) in items.iter().enumerate() {
        if count >= MAX_LIST_VALUES {
            return Err(rejected(
                "ERR flow governance list must contain at most 1000 values",
            ));
        }
        match item {
            Term::Bin(text) if !text.is_empty() && text.len() <= max_bytes => {
                if !kept.contains(item) {
                    kept.push(item.clone());
                }
            }
            Term::Bin(text) if !text.is_empty() => {
                return Err(rejected(format!(
                    "ERR flow governance {name} values must be at most {max_bytes} bytes"
                )));
            }
            _ => {
                return Err(rejected(
                    "ERR flow governance list values must be non-empty strings",
                ));
            }
        }
    }
    Ok(Some(Term::List(kept)))
}

pub(super) fn integer(
    value: Option<&Term>,
    name: &str,
    minimum: i64,
    required: bool,
) -> Result<Option<Term>, FlowError> {
    let text = if minimum > 0 {
        "a positive integer"
    } else {
        "a non-negative integer"
    };
    match value {
        None if !required => Ok(None),
        Some(Term::Int(number))
            if *number >= minimum
                && u64::try_from(*number).is_ok_and(|number| number <= MAX_EXACT) =>
        {
            Ok(Some(Term::Int(*number)))
        }
        _ => Err(rejected(format!(
            "ERR flow governance {name} must be {text}"
        ))),
    }
}

pub(super) fn section<'a>(
    value: Option<&'a Term>,
    name: &str,
) -> Result<Option<&'a Pairs>, FlowError> {
    match value {
        None => Ok(None),
        Some(value) => as_map(value).map(Some).ok_or_else(|| {
            rejected(format!(
                "ERR flow governance {name} policy must be a map or keyword list"
            ))
        }),
    }
}

fn effects(value: Option<&Term>) -> Result<Option<Term>, FlowError> {
    let Some(entries) = section(value, "effects")? else {
        return Ok(None);
    };
    let mut out = Vec::new();
    put(
        &mut out,
        "allowed",
        strings(get(entries, "allowed"), "allowed", MAX_STRING_BYTES)?,
    );
    put(
        &mut out,
        "denied",
        strings(get(entries, "denied"), "denied", MAX_STRING_BYTES)?,
    );
    put(
        &mut out,
        "require_idempotency_key",
        boolean(
            get(entries, "require_idempotency_key"),
            "require_idempotency_key",
        )?,
    );
    Ok(Some(Term::Map(out)))
}

fn audit(value: Option<&Term>) -> Result<Option<Term>, FlowError> {
    let Some(entries) = section(value, "audit")? else {
        return Ok(None);
    };
    let mut out = Vec::new();
    put(
        &mut out,
        "effect_events",
        boolean(get(entries, "effect_events"), "effect_events")?,
    );
    put(
        &mut out,
        "denials",
        choice(
            get(entries, "denials"),
            &["off", "sampled", "all"],
            "ERR invalid flow governance denials audit mode",
        )?,
    );
    Ok(Some(Term::Map(out)))
}

fn approvals(value: Option<&Term>) -> Result<Option<Term>, FlowError> {
    let Some(entries) = section(value, "approvals")? else {
        return Ok(None);
    };
    let mut out = Vec::new();
    put(
        &mut out,
        "effects",
        strings(get(entries, "effects"), "effects", MAX_STRING_BYTES)?,
    );
    put(
        &mut out,
        "states",
        strings(get(entries, "states"), "states", MAX_STRING_BYTES)?,
    );
    put(
        &mut out,
        "assignees",
        strings(get(entries, "assignees"), "assignees", MAX_STRING_BYTES)?,
    );
    put(
        &mut out,
        "timeout_ms",
        integer(get(entries, "timeout_ms"), "timeout_ms", 1, false)?,
    );
    Ok(Some(Term::Map(out)))
}

/// Normalizes a state's `governance`; `None` for nil.
///
/// # Errors
/// Elixir's text for the first invalid part.
pub fn normalize(policy: &Term) -> Result<Option<Term>, FlowError> {
    if *policy == Term::Nil {
        return Ok(None);
    }
    let entries = as_map(policy)
        .ok_or_else(|| rejected("ERR flow governance policy must be a map or keyword list"))?;
    let mut out = Vec::new();
    put(
        &mut out,
        "mode",
        choice(
            get(entries, "mode"),
            &["minimal", "ledger", "full"],
            "ERR invalid flow governance mode",
        )?,
    );
    put(&mut out, "effects", effects(get(entries, "effects"))?);
    put(&mut out, "audit", audit(get(entries, "audit"))?);
    put(
        &mut out,
        "limits",
        rules(get(entries, "limits"), "limits", "limit", limit_rule)?,
    );
    put(
        &mut out,
        "budgets",
        rules(get(entries, "budgets"), "budgets", "budget", budget_rule)?,
    );
    put(
        &mut out,
        "circuits",
        rules(
            get(entries, "circuits"),
            "circuits",
            "circuit",
            circuit_rule,
        )?,
    );
    put(&mut out, "approvals", approvals(get(entries, "approvals"))?);
    let normalized = Term::Map(out);
    let mut encoded = Vec::new();
    if normalized.encode(&mut encoded).is_none() || encoded.len() > MAX_POLICY_BYTES {
        return Err(rejected(format!(
            "ERR flow governance policy exceeds {MAX_POLICY_BYTES}-byte durable limit"
        )));
    }
    Ok(Some(normalized))
}

/// A state's running-Flow limit rule: credits, whether strictly global,
/// and the lease size.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RunningLimit {
    /// Flows that may run at once.
    pub limit: u64,
    /// `strict_global` enforcement (the default).
    pub strict: bool,
    /// Credits a lease asks for.
    pub lease_size: Option<u64>,
}

/// The `limits.running` rule of a normalized governance policy.
#[must_use]
pub fn running_limit(governance: &Term) -> Option<RunningLimit> {
    let rule = governance.get(b"limits")?.get(b"running")?;
    let limit = match rule.get(b"limit") {
        Some(Term::Int(limit)) => u64::try_from(*limit).ok()?,
        _ => return None,
    };
    let lease_size = match rule.get(b"lease_size") {
        Some(Term::Int(size)) => u64::try_from(*size).ok(),
        _ => None,
    };
    Some(RunningLimit {
        limit,
        strict: rule.get(b"enforcement") != Some(&Term::text("approximate_global")),
        lease_size,
    })
}

/// The limit scope a type's running-Flow rules spend from, as Elixir names
/// it: `flow-running:` and the URL-safe base64 SHA-256 of the type.
#[must_use]
pub fn running_scope(flow_type: &[u8]) -> Vec<u8> {
    use sha2::{Digest, Sha256};
    let mut scope = b"flow-running:".to_vec();
    scope.extend_from_slice(crate::routing::base64url_nopad(&Sha256::digest(flow_type)).as_bytes());
    scope
}
