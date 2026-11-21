//! `FLOW.GOVERNANCE.OVERVIEW`, as `Governance.Admin.overview/2`: the
//! approval, budget, limit and circuit listings under the same options,
//! with counts. Each listing reads the options it knows and refuses its
//! own invalid ones, in that order; the circuit listing (no wire command
//! of its own, and no status filter on the wire) orders open circuits
//! first, then half-open, then closed.

use super::super::FlowError;
use super::super::opts::Opts;
use super::super::term::{Term, map};
use super::circuit::Circuit;
use super::{Kind, Listed, approval, budget, limit};
use crate::routing::Partition;

const MAX_LISTED: usize = 1_000;

/// The circuit listing an overview carries.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CircuitList {
    /// Most circuits listed.
    pub limit: usize,
    /// Only these scopes.
    pub scopes: Option<Vec<Vec<u8>>>,
}

/// An overview's four listings.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Overview {
    /// Approvals.
    pub approvals: approval::List,
    /// Budgets.
    pub budgets: budget::List,
    /// Limits.
    pub limits: limit::List,
    /// Circuits.
    pub circuits: CircuitList,
}

fn rejected(text: &str) -> FlowError {
    FlowError::rejected(text.to_owned())
}

/// The request's options a listing knows, as its own request.
fn only(args: &Term, keys: &[&str]) -> Term {
    match args {
        Term::Map(entries) => Term::Map(
            entries
                .iter()
                .filter(|(key, _)| keys.iter().any(|known| known.as_bytes() == key.as_slice()))
                .cloned()
                .collect(),
        ),
        other => other.clone(),
    }
}

fn circuit_list(args: &Term) -> Result<CircuitList, FlowError> {
    let opts = Opts::new(
        "FLOW.GOVERNANCE.OVERVIEW",
        args,
        &["limit", "scope", "partition_key"],
    )?;
    let limit = match opts.get("limit") {
        None => 100,
        Some(Term::Int(limit)) if *limit > 0 => usize::try_from(*limit)
            .unwrap_or(usize::MAX)
            .min(MAX_LISTED),
        Some(_) => {
            return Err(rejected(
                "ERR flow circuit limit must be a positive integer",
            ));
        }
    };
    let scopes = match (opts.get("scope"), opts.get("partition_key")) {
        (Some(Term::Bin(scope)), _) if !scope.is_empty() => Some(vec![scope.clone()]),
        (None | Some(Term::Nil), Some(Term::Bin(partition))) if !partition.is_empty() => {
            let mut prefixed = b"partition:".to_vec();
            prefixed.extend_from_slice(partition);
            Some(vec![partition.clone(), prefixed])
        }
        (None | Some(Term::Nil), None | Some(Term::Nil)) => None,
        _ => {
            return Err(rejected(
                "ERR flow circuit scope must be a non-empty string",
            ));
        }
    };
    for scope in scopes.iter().flatten() {
        let prefix = super::super::keys::governance_prefix(&Partition::parse(scope), "c").len();
        super::key_size(prefix.saturating_add(scope.len()))?;
    }
    Ok(CircuitList { limit, scopes })
}

/// Reads `FLOW.GOVERNANCE.OVERVIEW`.
///
/// # Errors
/// An unknown option, or the first listing's text for its invalid option.
pub fn parse(args: &Term) -> Result<Overview, FlowError> {
    Opts::new(
        "FLOW.GOVERNANCE.OVERVIEW",
        args,
        &[
            "limit",
            "status",
            "scope",
            "partition_key",
            "flow_id",
            "now_ms",
            "deadline_ms",
        ],
    )?;
    Ok(Overview {
        approvals: approval::parse_list(&only(
            args,
            &["limit", "status", "scope", "partition_key", "flow_id"],
        ))?,
        budgets: budget::parse_list(&only(args, &["limit", "scope", "partition_key"]))?,
        limits: limit::parse_list(&only(args, &["limit", "scope", "partition_key", "now_ms"]))?,
        circuits: circuit_list(&only(args, &["limit", "scope", "partition_key"]))?,
    })
}

const fn rank(status: &[u8]) -> u8 {
    match status {
        b"open" => 0,
        b"half_open" => 1,
        b"closed" => 2,
        _ => 3,
    }
}

impl CircuitList {
    fn select<'a>(&self, records: impl Iterator<Item = &'a [u8]>) -> Listed {
        records
            .filter_map(Circuit::decode)
            .filter(|circuit| {
                self.scopes
                    .as_ref()
                    .is_none_or(|scopes| scopes.contains(&circuit.scope))
            })
            .map(|circuit| {
                let mut key = vec![rank(&circuit.status)];
                key.extend_from_slice(&circuit.scope);
                (key, circuit.view())
            })
            .collect()
    }

    fn join(&self, mut parts: Listed) -> Term {
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

impl Overview {
    /// One shard's records of every kind, keyed by kind, then each
    /// listing's own order.
    #[must_use]
    pub fn select(&self, records: &[(Kind, &[u8])], wall: u64) -> Listed {
        let tagged = |kind: Kind, listed: Listed| {
            listed.into_iter().map(move |(key, view)| {
                let mut tagged = vec![kind.tag()];
                tagged.extend(key);
                (tagged, view)
            })
        };
        let pick = |kind: Kind| {
            records
                .iter()
                .filter(move |(held, _)| *held == kind)
                .map(|(_, value)| *value)
        };
        let mut listed: Listed = Vec::new();
        listed.extend(tagged(
            Kind::Approval,
            self.approvals.select(pick(Kind::Approval)),
        ));
        listed.extend(tagged(
            Kind::Budget,
            self.budgets.select(pick(Kind::Budget)),
        ));
        listed.extend(tagged(
            Kind::Limit,
            self.limits.select(pick(Kind::Limit), wall),
        ));
        listed.extend(tagged(
            Kind::Circuit,
            self.circuits.select(pick(Kind::Circuit)),
        ));
        listed
    }

    /// The overview: each listing joined, and the counts.
    #[must_use]
    pub fn join(&self, parts: &[(Vec<u8>, Term)]) -> Term {
        let part = |kind: Kind| -> Listed {
            parts
                .iter()
                .filter(|(key, _)| key.first() == Some(&kind.tag()))
                .map(|(key, view)| (key.get(1..).unwrap_or_default().to_vec(), view.clone()))
                .collect()
        };
        let approvals = self.approvals.join(part(Kind::Approval));
        let budgets = self.budgets.join(part(Kind::Budget));
        let limits = self.limits.join(part(Kind::Limit));
        let circuits = self.circuits.join(part(Kind::Circuit));
        let count = |list: &Term, key: &[u8], value: Option<&[u8]>| -> Term {
            let Term::List(items) = list else {
                return Term::int(0);
            };
            let matching = items
                .iter()
                .filter(|item| {
                    value.is_none_or(|value| item.get(key) == Some(&Term::Bin(value.to_vec())))
                })
                .count();
            Term::int(u64::try_from(matching).unwrap_or(u64::MAX))
        };
        let counts = map(vec![
            ("approvals", count(&approvals, b"status", None)),
            (
                "pending_approvals",
                count(&approvals, b"status", Some(b"pending")),
            ),
            ("budgets", count(&budgets, b"scope", None)),
            ("limits", count(&limits, b"scope", None)),
            ("circuits", count(&circuits, b"status", None)),
            ("open_circuits", count(&circuits, b"status", Some(b"open"))),
            (
                "half_open_circuits",
                count(&circuits, b"status", Some(b"half_open")),
            ),
        ]);
        map(vec![
            ("approvals", approvals),
            ("budgets", budgets),
            ("limits", limits),
            ("circuits", circuits),
            ("counts", counts),
        ])
    }
}
