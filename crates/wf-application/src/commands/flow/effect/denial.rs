//! Governance refusals as `Ferricstore.Flow.Governance.Decision` builds
//! them, their decision ids, and the hash of the policy effects are
//! decided under.
//!
//! A refusal is an error whose reply is a map: the decision's fields with
//! its `GOVERNANCE_*` code, and `retryable` and `safe_to_retry` false.
//! Elixir also sends the transport's `code: "error"` under the same key,
//! which clients refuse as a duplicate; this node sends the code once.

use std::sync::LazyLock;

use sha2::{Digest, Sha256};

use super::super::term::{Term, map};
use crate::routing::base64url_nopad;

/// A refusal field's value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Field {
    /// Nil.
    Nil,
    /// A binary.
    Bin(Vec<u8>),
    /// A count or time.
    Int(u64),
}

impl From<Option<Vec<u8>>> for Field {
    fn from(value: Option<Vec<u8>>) -> Self {
        value.map_or(Self::Nil, Self::Bin)
    }
}

/// A structured governance refusal.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Denial {
    /// Its fields in reply order.
    pub fields: Vec<(&'static str, Field)>,
}

/// `decision_id/2`: the URL-safe base64 SHA-256 of `reason:subject`.
#[must_use]
pub fn decision_id(reason: &str, subject: &[u8]) -> Vec<u8> {
    let mut hasher = Sha256::new();
    hasher.update(reason.as_bytes());
    hasher.update(b":");
    hasher.update(subject);
    base64url_nopad(&hasher.finalize()).into_bytes()
}

/// `Decision.error/2`'s reason for a code.
const fn reason(code: &str) -> &'static str {
    match code.as_bytes() {
        b"GOVERNANCE_EFFECT_DENIED" => "effect_denied",
        b"GOVERNANCE_CONFLICT" => "governance_conflict",
        b"GOVERNANCE_BUDGET_EXHAUSTED" => "budget_exhausted",
        b"GOVERNANCE_LIMIT_EXCEEDED" => "limit_exhausted",
        _ => "governance_denied",
    }
}

impl Denial {
    /// A decision with `code` and `message`, its reason, then `rest`.
    #[must_use]
    pub fn new(
        code: &'static str,
        message: Vec<u8>,
        rest: Vec<(&'static str, Option<Vec<u8>>)>,
    ) -> Self {
        let mut fields = vec![
            ("code", Field::Bin(code.as_bytes().to_vec())),
            ("message", Field::Bin(message)),
            ("reason", Field::Bin(reason(code).as_bytes().to_vec())),
        ];
        fields.extend(
            rest.into_iter()
                .map(|(key, value)| (key, Field::from(value))),
        );
        Self { fields }
    }

    /// The decision with `key` set to `value` as well.
    #[must_use]
    pub fn with(mut self, key: &'static str, value: Field) -> Self {
        self.fields.push((key, value));
        self
    }

    fn field(&self, name: &str) -> &[u8] {
        self.fields
            .iter()
            .find_map(|(key, value)| match value {
                Field::Bin(bytes) if *key == name => Some(bytes.as_slice()),
                _ => None,
            })
            .unwrap_or_default()
    }

    /// Its code.
    #[must_use]
    pub fn code(&self) -> &[u8] {
        self.field("code")
    }

    /// Its message.
    #[must_use]
    pub fn message(&self) -> &[u8] {
        self.field("message")
    }

    /// Its policy name.
    #[must_use]
    pub fn policy(&self) -> &[u8] {
        self.field("policy")
    }

    /// The reply map.
    #[must_use]
    pub fn payload(&self) -> Term {
        let mut entries: Vec<(&str, Term)> = self
            .fields
            .iter()
            .map(|(key, value)| {
                let value = match value {
                    Field::Nil => Term::Nil,
                    Field::Bin(bytes) => Term::Bin(bytes.clone()),
                    Field::Int(number) => Term::int(*number),
                };
                (*key, value)
            })
            .collect();
        entries.extend([
            ("retryable", Term::Bool(false)),
            ("safe_to_retry", Term::Bool(false)),
        ]);
        map(entries)
    }
}

/// Elixir's default governance policy (`Governance.Policy.default/0`), the
/// one every effect is decided under, in canonical key order.
#[must_use]
pub fn default_policy() -> Term {
    let empty = || Term::List(Vec::new());
    map(vec![
        (
            "approvals",
            map(vec![
                ("assignees", empty()),
                ("effects", empty()),
                ("states", empty()),
            ]),
        ),
        (
            "audit",
            map(vec![
                ("denials", Term::text("sampled")),
                ("effect_events", Term::Bool(true)),
            ]),
        ),
        ("budgets", map(Vec::new())),
        ("circuits", map(Vec::new())),
        (
            "effects",
            map(vec![
                ("allowed", empty()),
                ("denied", empty()),
                ("require_idempotency_key", Term::Bool(true)),
            ]),
        ),
        ("limits", map(Vec::new())),
        ("mode", Term::text("minimal")),
    ])
}

/// The default policy's hash, computed once.
static DEFAULT_HASH: LazyLock<Vec<u8>> = LazyLock::new(|| digest(&default_policy()));

/// The hash effects of a type without a policy are decided under: the
/// URL-safe base64 SHA-256 of the default policy's encoding.
#[must_use]
pub fn default_policy_hash() -> &'static [u8] {
    &DEFAULT_HASH
}

/// The hash effects of a type with `policy` stored are decided under: the
/// default policy and `policy` in its normalized form, which orders state
/// and rule names and builds every other map in a fixed order, so equal
/// policies share a hash however their maps were given. A shard computes
/// it once, when it installs the policy.
///
/// Elixir hashes its own term encoding of the type's whole stored record,
/// its name included, so the value differs; clients compare it, not
/// decode it.
#[must_use]
pub fn policy_hash(policy: &Term) -> Vec<u8> {
    digest(&Term::List(vec![default_policy(), policy.clone()]))
}

fn digest(term: &Term) -> Vec<u8> {
    let mut encoded = Vec::new();
    // A policy that reaches here was stored, so it encodes.
    let _encoded = term.encode(&mut encoded);
    base64url_nopad(&Sha256::digest(&encoded)).into_bytes()
}
