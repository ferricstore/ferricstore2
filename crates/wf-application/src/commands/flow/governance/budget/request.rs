//! Reading budget requests in the order `BudgetStore` checks them, a
//! commit's usage as `Budget.valid_usage?/1` bounds it, and listings.

use std::collections::BTreeMap;

use super::super::super::opts::{MAX_EXACT, Opts};
use super::super::super::term::Term;
use super::super::super::{FlowError, keys};
use super::super::{Listed, key_size, native};
use super::{Budget, Command};
use crate::routing::Partition;

const MAX_RESERVATION_ID_BYTES: usize = 256;
const MAX_USAGE_BYTES: usize = 262_144;
const MAX_USAGE_DEPTH: usize = 64;
const MAX_USAGE_NODES: usize = 4_096;
const MAX_LISTED: usize = 1_000;

fn rejected(text: String) -> FlowError {
    FlowError::rejected(text)
}

fn scope_key(scope: &[u8]) -> Result<(), FlowError> {
    let prefix = keys::governance_prefix(&Partition::parse(scope), "b").len();
    key_size(prefix.saturating_add(scope.len()))
}

/// `Args.error/2`: the first argument that is not a non-empty string.
fn non_empty(command: &str, args: &[(&str, &[u8])]) -> Result<(), FlowError> {
    match args.iter().find(|(_, value)| value.is_empty()) {
        Some((name, _)) => Err(rejected(format!(
            "ERR flow {command} {name} must be a non-empty string"
        ))),
        None => Ok(()),
    }
}

fn now_ms(opts: &Opts<'_>) -> Result<Option<u64>, FlowError> {
    match opts.get("now_ms") {
        None => Ok(None),
        Some(Term::Int(now)) if u64::try_from(*now).is_ok_and(|now| now <= MAX_EXACT) => {
            Ok(u64::try_from(*now).ok())
        }
        Some(_) => Err(rejected(
            "ERR flow budget now_ms must be a non-negative integer".to_owned(),
        )),
    }
}

fn reservation_size(id: &[u8]) -> Result<(), FlowError> {
    if id.len() > MAX_RESERVATION_ID_BYTES {
        return Err(rejected(
            "ERR flow budget reservation_id must be at most 256 bytes".to_owned(),
        ));
    }
    Ok(())
}

fn native_integer(args: &Term, key: &str, minimum: i64) -> Result<u64, FlowError> {
    match args.get(key.as_bytes()) {
        Some(Term::Int(value)) if *value >= minimum => {
            Ok(u64::try_from(*value).unwrap_or(u64::MAX))
        }
        _ => Err(FlowError::BadRequest(format!(
            "ERR native field {key} must be a {}",
            if minimum > 0 {
                "positive integer"
            } else {
                "non-negative integer"
            }
        ))),
    }
}

fn reserve(args: &Term) -> Result<Command, FlowError> {
    let scope = native(args, "scope")?;
    let amount = native_integer(args, "amount", 1)?;
    let opts = Opts::new(
        "FLOW.BUDGET.RESERVE",
        args,
        &[
            "scope",
            "amount",
            "now_ms",
            "reservation_id",
            "limit",
            "window_ms",
            "deadline_ms",
        ],
    )?;
    non_empty("budget", &[("scope", &scope)])?;
    scope_key(&scope)?;
    if amount > MAX_EXACT {
        return Err(rejected(
            "ERR flow budget amount must be a positive integer".to_owned(),
        ));
    }
    let now_ms = now_ms(&opts)?;
    let reservation_id = match opts.get("reservation_id") {
        None | Some(Term::Nil) => None,
        Some(Term::Bin(id)) if !id.is_empty() => {
            reservation_size(id)?;
            Some(id.clone())
        }
        Some(_) => {
            return Err(rejected(
                "ERR flow budget reservation_id must be a non-empty string".to_owned(),
            ));
        }
    };
    Ok(Command::Reserve {
        shape: (opts.get("limit").cloned(), opts.get("window_ms").cloned()),
        scope,
        amount,
        now_ms,
        reservation_id,
    })
}

fn settle(name: &str, args: &Term, commit: bool) -> Result<Command, FlowError> {
    let scope = native(args, "scope")?;
    let reservation_id = native(args, "reservation_id")?;
    let actual = if commit {
        Some(native_integer(args, "actual_amount", 0)?)
    } else {
        None
    };
    let allowed: &[&str] = if commit {
        &[
            "scope",
            "reservation_id",
            "actual_amount",
            "usage",
            "now_ms",
            "deadline_ms",
        ]
    } else {
        &["scope", "reservation_id", "now_ms", "deadline_ms"]
    };
    let opts = Opts::new(name, args, allowed)?;
    let command = if commit {
        "budget commit"
    } else {
        "budget release"
    };
    non_empty(
        command,
        &[("scope", &scope), ("reservation_id", &reservation_id)],
    )?;
    scope_key(&scope)?;
    if actual.is_some_and(|actual| actual > MAX_EXACT) {
        return Err(rejected(
            "ERR flow budget actual_amount must be a non-negative integer".to_owned(),
        ));
    }
    reservation_size(&reservation_id)?;
    Ok(Command::Settle {
        now_ms: now_ms(&opts)?,
        usage: opts
            .get("usage")
            .filter(|usage| **usage != Term::Nil)
            .cloned(),
        scope,
        reservation_id,
        actual_amount: actual,
    })
}

/// Reads `FLOW.BUDGET.RESERVE`, `COMMIT`, `RELEASE` or `GET`.
///
/// # Errors
/// Elixir's text for an invalid option, or an unknown command.
pub fn parse(name: &str, args: &Term) -> Result<Command, FlowError> {
    match name {
        "FLOW.BUDGET.RESERVE" => reserve(args),
        "FLOW.BUDGET.COMMIT" => settle(name, args, true),
        "FLOW.BUDGET.RELEASE" => settle(name, args, false),
        "FLOW.BUDGET.GET" => {
            let scope = native(args, "scope")?;
            Opts::new(name, args, &["scope", "deadline_ms"])?;
            non_empty("budget", &[("scope", &scope)])?;
            scope_key(&scope)?;
            Ok(Command::Get(scope))
        }
        _ => Err(rejected(format!(
            "ERR {name} is not supported by this server"
        ))),
    }
}

fn positive(value: Option<&Term>, key: &str) -> Result<u64, FlowError> {
    match value {
        Some(Term::Int(value))
            if *value > 0 && u64::try_from(*value).is_ok_and(|value| value <= MAX_EXACT) =>
        {
            Ok(u64::try_from(*value).unwrap_or(0))
        }
        _ => Err(rejected(format!(
            "ERR flow budget {key} must be a positive integer"
        ))),
    }
}

/// `new_budget/3`: a first reservation's budget, from its limit and window.
pub(super) fn new_budget(
    scope: &[u8],
    shape: &(Option<Term>, Option<Term>),
    now: u64,
) -> Result<Budget, FlowError> {
    let limit = positive(shape.0.as_ref(), "limit")?;
    let window_ms = positive(shape.1.as_ref(), "window_ms")?;
    Ok(Budget {
        scope: scope.to_vec(),
        limit,
        window_ms,
        window_start_ms: now,
        used: 0,
        reservations: BTreeMap::new(),
        settled: Vec::new(),
        generated: 0,
    })
}

/// `validate_usage_term/3`: each value spends a node; a list element sits
/// one level below the element before it, as Elixir walks the list.
fn nodes(term: &Term, depth: usize, remaining: usize) -> Option<usize> {
    if depth > MAX_USAGE_DEPTH || remaining == 0 {
        return None;
    }
    match term {
        Term::List(items) => {
            let mut left = remaining;
            for (index, item) in items.iter().enumerate() {
                let level = depth.saturating_add(index);
                if level > MAX_USAGE_DEPTH || left == 0 {
                    return None;
                }
                left = nodes(item, level.saturating_add(1), left.saturating_sub(1))?;
            }
            let tail = depth.saturating_add(items.len());
            (tail <= MAX_USAGE_DEPTH && left > 0).then(|| left.saturating_sub(1))
        }
        Term::Map(entries) => {
            let mut left = remaining.saturating_sub(1);
            for (_, value) in entries {
                left = left.checked_sub(1).filter(|_| depth < MAX_USAGE_DEPTH)?;
                left = nodes(value, depth.saturating_add(1), left)?;
            }
            Some(left)
        }
        _ => Some(remaining.saturating_sub(1)),
    }
}

/// Whether a commit's usage is a bounded portable term.
pub(super) fn portable(usage: &Term) -> bool {
    let mut encoded = Vec::new();
    nodes(usage, 0, MAX_USAGE_NODES).is_some()
        && usage.encode(&mut encoded).is_some()
        && encoded.len() <= MAX_USAGE_BYTES
}

/// `FLOW.BUDGET.LIST`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct List {
    /// Most budgets listed.
    pub limit: usize,
    /// Only these scopes.
    pub scopes: Option<Vec<Vec<u8>>>,
}

/// Reads `FLOW.BUDGET.LIST`.
///
/// # Errors
/// Elixir's text for an invalid option.
pub fn parse_list(args: &Term) -> Result<List, FlowError> {
    let opts = Opts::new(
        "FLOW.BUDGET.LIST",
        args,
        &["limit", "scope", "partition_key", "deadline_ms"],
    )?;
    let limit = match opts.get("limit") {
        None => 100,
        Some(Term::Int(limit)) if *limit > 0 => usize::try_from(*limit)
            .unwrap_or(usize::MAX)
            .min(MAX_LISTED),
        Some(_) => {
            return Err(rejected(
                "ERR flow budget limit must be a positive integer".to_owned(),
            ));
        }
    };
    let scopes = match (opts.get("scope"), opts.get("partition_key")) {
        (Some(Term::Bin(scope)), _) if !scope.is_empty() => {
            scope_key(scope)?;
            Some(vec![scope.clone()])
        }
        (None | Some(Term::Nil), Some(Term::Bin(partition))) if !partition.is_empty() => {
            let mut prefixed = b"partition:".to_vec();
            prefixed.extend_from_slice(partition);
            let scopes = vec![partition.clone(), prefixed];
            for scope in &scopes {
                scope_key(scope)?;
            }
            Some(scopes)
        }
        (None | Some(Term::Nil), None | Some(Term::Nil)) => None,
        _ => {
            return Err(rejected(
                "ERR flow budget scope must be a non-empty string".to_owned(),
            ));
        }
    };
    Ok(List { limit, scopes })
}

impl List {
    /// One shard's matching budgets, keyed by scope.
    #[must_use]
    pub fn select<'a>(&self, records: impl Iterator<Item = &'a [u8]>) -> Listed {
        records
            .filter_map(Budget::decode)
            .filter(|budget| {
                self.scopes
                    .as_ref()
                    .is_none_or(|scopes| scopes.contains(&budget.scope))
            })
            .map(|budget| (budget.scope.clone(), budget.view()))
            .collect()
    }

    /// The budgets by scope, cut to the limit.
    #[must_use]
    pub fn join(&self, mut parts: Listed) -> Term {
        parts.sort_by(|left, right| left.0.cmp(&right.0));
        Term::List(
            parts
                .into_iter()
                .take(self.limit)
                .map(|(_, view)| view)
                .collect(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{Term, portable};

    #[test]
    fn usage_is_bounded_as_elixir_walks_it() {
        // A list element sits one level below the one before it: 64
        // elements fit, 65 do not.
        let list = |count: i64| Term::List((0..count).map(Term::Int).collect());
        assert!(portable(&list(64)));
        assert!(!portable(&list(65)));
        // The innermost value of 64 nested maps sits at depth 64; one more
        // map puts its key past the bound.
        let nest = |count| {
            (0..count).fold(Term::Int(1), |inner, _| {
                Term::Map(vec![(b"k".to_vec(), inner)])
            })
        };
        assert!(portable(&nest(64)));
        assert!(!portable(&nest(65)));
        let wide = Term::Map(
            (0..2_100)
                .map(|index| (format!("k{index}").into_bytes(), Term::Int(index)))
                .collect(),
        );
        assert!(!portable(&wide));
    }
}
