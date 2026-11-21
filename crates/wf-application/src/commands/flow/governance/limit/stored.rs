//! A limit's stored form: its reply map with the policy version's
//! fingerprint, the spend count, and each lease's reservation ids in use
//! and recorded count.

use std::collections::{BTreeMap, BTreeSet};

use super::super::super::term::{Term, map};
use super::{Lease, Owner};

fn int(term: &Term, key: &str) -> Option<u64> {
    match term.get(key.as_bytes()) {
        Some(Term::Int(value)) => u64::try_from(*value).ok(),
        _ => None,
    }
}

fn lease_term(lease: &Lease) -> Term {
    let Term::Map(mut entries) = lease.view() else {
        return Term::Nil;
    };
    entries.push((
        b"active".to_vec(),
        Term::List(lease.active.iter().cloned().map(Term::Bin).collect()),
    ));
    entries.push((b"recorded".to_vec(), Term::int(lease.recorded)));
    Term::Map(entries)
}

fn lease(term: &Term) -> Option<Lease> {
    let active = match term.get(b"active") {
        Some(Term::List(items)) => items
            .iter()
            .map(|item| match item {
                Term::Bin(id) => Some(id.clone()),
                _ => None,
            })
            .collect::<Option<BTreeSet<_>>>()?,
        _ => return None,
    };
    let last_spend_at_ms = match term.get(b"last_spend_at_ms") {
        None | Some(Term::Nil) => None,
        Some(Term::Int(at)) => Some(u64::try_from(*at).ok()?),
        Some(_) => return None,
    };
    Some(Lease {
        shard_id: int(term, "shard_id")?,
        epoch: int(term, "epoch")?,
        expires_at_ms: int(term, "expires_at_ms")?,
        available: int(term, "available")?,
        in_use: int(term, "in_use")?,
        pending_reclaim: int(term, "pending_reclaim")?,
        last_spend_at_ms,
        active,
        recorded: int(term, "recorded")?,
    })
}

impl Owner {
    /// The stored form.
    #[must_use]
    pub fn encode(&self) -> Option<Vec<u8>> {
        let term = map(vec![
            ("scope", Term::Bin(self.scope.clone())),
            ("limit", Term::int(self.limit)),
            ("free", Term::int(self.free)),
            ("config_version", Term::int(self.config_version)),
            ("epoch", Term::int(self.epoch)),
            (
                "policy_version",
                self.policy_version
                    .map_or(Term::Nil, |digest| Term::Bin(digest.to_vec())),
            ),
            (
                "leases",
                Term::List(self.leases.values().map(lease_term).collect()),
            ),
            ("batches", Term::int(self.batches)),
        ]);
        let mut out = Vec::new();
        term.encode(&mut out)?;
        Some(out)
    }

    /// Reads the stored form; `None` when it is not a limit.
    #[must_use]
    pub fn decode(bytes: &[u8]) -> Option<Self> {
        let Some((term, [])) = Term::decode(bytes) else {
            return None;
        };
        let policy_version = match term.get(b"policy_version") {
            None | Some(Term::Nil) => None,
            Some(Term::Bin(digest)) => Some(<[u8; 32]>::try_from(digest.as_slice()).ok()?),
            Some(_) => return None,
        };
        let leases = match term.get(b"leases") {
            Some(Term::List(items)) => items
                .iter()
                .map(|item| lease(item).map(|lease| (lease.shard_id, lease)))
                .collect::<Option<BTreeMap<_, _>>>()?,
            _ => return None,
        };
        let scope = match term.get(b"scope") {
            Some(Term::Bin(scope)) => scope.clone(),
            _ => return None,
        };
        Some(Self {
            scope,
            limit: int(&term, "limit")?,
            free: int(&term, "free")?,
            config_version: int(&term, "config_version")?,
            epoch: int(&term, "epoch")?,
            policy_version,
            leases,
            batches: int(&term, "batches")?,
        })
    }
}
