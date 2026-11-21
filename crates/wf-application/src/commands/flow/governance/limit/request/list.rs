//! Reading `FLOW.LIMIT.LIST`, and selecting and joining its parts.

use super::super::super::super::FlowError;
use super::super::super::super::opts::Opts;
use super::super::super::super::term::Term;
use super::super::super::Listed;
use super::super::Owner;
use super::{now_ms, rejected, scope_key};

const MAX_LISTED: usize = 1_000;

/// `FLOW.LIMIT.LIST`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct List {
    /// Most limits listed.
    pub limit: usize,
    /// Only these scopes.
    pub scopes: Option<Vec<Vec<u8>>>,
    /// The client's clock, for reclaiming expired leases in the view.
    pub now_ms: Option<u64>,
}

/// Reads `FLOW.LIMIT.LIST`.
///
/// # Errors
/// Elixir's text for an invalid option.
pub fn parse_list(args: &Term) -> Result<List, FlowError> {
    let opts = Opts::new(
        "FLOW.LIMIT.LIST",
        args,
        &["limit", "scope", "partition_key", "now_ms", "deadline_ms"],
    )?;
    let limit = match opts.get("limit") {
        None => 100,
        Some(Term::Int(limit)) if *limit > 0 => usize::try_from(*limit)
            .unwrap_or(usize::MAX)
            .min(MAX_LISTED),
        Some(_) => return Err(rejected("ERR flow limit limit must be a positive integer")),
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
        _ => return Err(rejected("ERR flow limit scope must be a non-empty string")),
    };
    Ok(List {
        limit,
        scopes,
        now_ms: now_ms(&opts)?,
    })
}

impl List {
    /// One shard's matching limits, keyed by scope, expired leases
    /// reclaimed in the view at the listing's time (`wall` when none).
    #[must_use]
    pub fn select<'a>(&self, records: impl Iterator<Item = &'a [u8]>, wall: u64) -> Listed {
        records
            .filter_map(Owner::decode)
            .filter(|owner| {
                self.scopes
                    .as_ref()
                    .is_none_or(|scopes| scopes.contains(&owner.scope))
            })
            .map(|mut owner| {
                owner.reclaim(self.now_ms.unwrap_or(wall));
                (owner.scope.clone(), owner.view())
            })
            .collect()
    }

    /// The limits by scope, cut to the limit.
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
