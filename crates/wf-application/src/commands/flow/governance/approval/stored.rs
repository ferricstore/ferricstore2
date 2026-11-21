//! An approval's stored form: its reply map, encoded.

use super::super::super::term::Term;
use super::{Approval, Status, Version};

fn text(term: &Term, key: &str) -> Option<Vec<u8>> {
    match term.get(key.as_bytes()) {
        Some(Term::Bin(value)) => Some(value.clone()),
        _ => None,
    }
}

/// A stored field that is neither nil nor of its type.
struct Damaged;

fn optional_text(term: &Term, key: &str) -> Result<Option<Vec<u8>>, Damaged> {
    match term.get(key.as_bytes()) {
        None | Some(Term::Nil) => Ok(None),
        Some(Term::Bin(value)) => Ok(Some(value.clone())),
        Some(_) => Err(Damaged),
    }
}

fn optional_int(term: &Term, key: &str) -> Result<Option<u64>, Damaged> {
    match term.get(key.as_bytes()) {
        None | Some(Term::Nil) => Ok(None),
        Some(Term::Int(value)) => u64::try_from(*value).map(Some).map_err(|_| Damaged),
        Some(_) => Err(Damaged),
    }
}

impl Approval {
    /// The stored form.
    #[must_use]
    pub fn encode(&self) -> Option<Vec<u8>> {
        let mut out = Vec::new();
        self.view().encode(&mut out)?;
        Some(out)
    }

    /// Reads the stored form; `None` when it is not an approval.
    #[must_use]
    pub fn decode(bytes: &[u8]) -> Option<Self> {
        let Some((term, [])) = Term::decode(bytes) else {
            return None;
        };
        let policy_version = match term.get(b"policy_version") {
            None | Some(Term::Nil) => None,
            Some(Term::Bin(text)) => Some(Version::Text(text.clone())),
            Some(Term::Int(number)) => Some(Version::Number(u64::try_from(*number).ok()?)),
            Some(_) => return None,
        };
        let assignees = match term.get(b"assignees") {
            Some(Term::List(items)) => items
                .iter()
                .map(|item| match item {
                    Term::Bin(value) => Some(value.clone()),
                    _ => None,
                })
                .collect::<Option<Vec<_>>>()?,
            _ => return None,
        };
        let requested_at_ms = match term.get(b"requested_at_ms") {
            Some(Term::Int(value)) => u64::try_from(*value).ok()?,
            _ => return None,
        };
        Some(Self {
            id: text(&term, "id")?,
            flow_id: text(&term, "flow_id")?,
            scope: text(&term, "scope")?,
            reason: optional_text(&term, "reason").ok()?,
            requested_by: optional_text(&term, "requested_by").ok()?,
            requested_at_ms,
            assignees,
            policy_hash: optional_text(&term, "policy_hash").ok()?,
            policy_version,
            expires_at_ms: optional_int(&term, "expires_at_ms").ok()?,
            status: Status::parse(&text(&term, "status")?)?,
            decided_by: optional_text(&term, "decided_by").ok()?,
            decided_at_ms: optional_int(&term, "decided_at_ms").ok()?,
            decision_reason: optional_text(&term, "decision_reason").ok()?,
        })
    }
}
