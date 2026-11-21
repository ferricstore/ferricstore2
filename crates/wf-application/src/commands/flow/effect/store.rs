//! A Flow's effects and governance ledger as stored with the Flow, and the
//! reply maps Elixir builds from them (`public_effect/2`, ledger events).

use std::collections::BTreeMap;

use super::super::term::{Term, map};
use super::{LEDGER_MAX_BYTES, LEDGER_MAX_EVENTS, Status};

/// One effect of a Flow.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Effect {
    /// What kind of effect it is.
    pub effect_type: Vec<u8>,
    /// The Flow's type when it was reserved.
    pub flow_type: Vec<u8>,
    /// The Flow's state when it was reserved.
    pub state: Vec<u8>,
    /// Where it is in its lifecycle.
    pub status: Status,
    /// What it does, for replay and conflict checks.
    pub operation_digest: Vec<u8>,
    /// The key the reservation carried.
    pub idempotency_key: Option<Vec<u8>>,
    /// The hash of the policy it was reserved under.
    pub policy_hash: Vec<u8>,
    /// When it was reserved.
    pub created_at_ms: u64,
    /// When it last moved.
    pub updated_at_ms: u64,
    /// The provider's id for it.
    pub external_id: Option<Vec<u8>>,
    /// What went wrong.
    pub error: Option<Vec<u8>>,
    /// Why.
    pub reason: Option<Vec<u8>>,
    /// How long it took.
    pub latency_ms: Option<u64>,
}

/// What a ledger event records.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// An effect reached a status.
    Effect(Status),
    /// A reservation the policy refused.
    EffectDenied,
}

impl Kind {
    fn name(self) -> String {
        match self {
            Self::Effect(status) => format!("effect_{}", status.name()),
            Self::EffectDenied => "effect_denied".to_owned(),
        }
    }

    fn parse(name: &[u8]) -> Option<Self> {
        if name == b"effect_denied" {
            return Some(Self::EffectDenied);
        }
        name.strip_prefix(b"effect_")
            .and_then(Status::parse)
            .map(Self::Effect)
    }
}

/// One governance ledger event.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Event {
    /// Its id: deterministic for effect moves, numbered for refusals.
    pub id: Vec<u8>,
    /// What it records.
    pub kind: Kind,
    /// When.
    pub at_ms: u64,
    /// The effect's key.
    pub effect_key: Vec<u8>,
    /// The effect's type.
    pub effect_type: Vec<u8>,
    /// The policy hash in force.
    pub policy_hash: Vec<u8>,
    /// A refusal's code, message and policy.
    pub denial: Option<[Vec<u8>; 3]>,
}

/// A Flow's effects by key, its ledger in time order, and how many
/// refusals it has recorded (numbering their event ids).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Governance {
    /// Effects by key.
    pub effects: BTreeMap<Vec<u8>, Effect>,
    /// Ledger events, oldest first.
    pub ledger: Vec<Event>,
    /// Refusals recorded so far.
    pub denials: u64,
}

/// The Flow fields every view carries.
#[derive(Clone, Copy, Debug)]
pub struct Owner<'a> {
    /// The Flow id.
    pub id: &'a [u8],
    /// Its partition's name.
    pub partition_key: &'a [u8],
}

fn bin(value: &[u8]) -> Term {
    Term::Bin(value.to_vec())
}

fn optional(entries: &mut Vec<(&str, Term)>, key: &'static str, value: Option<&Vec<u8>>) {
    if let Some(value) = value {
        entries.push((key, bin(value)));
    }
}

impl Effect {
    /// The effect as Elixir replies with it, `decision` added to a
    /// command's reply.
    #[must_use]
    pub fn view(&self, owner: Owner<'_>, effect_key: &[u8], decision: Option<&str>) -> Term {
        let mut entries = vec![
            ("flow_id", bin(owner.id)),
            ("partition_key", bin(owner.partition_key)),
            ("type", bin(&self.flow_type)),
            ("state", bin(&self.state)),
            ("effect_key", bin(effect_key)),
            ("effect_type", bin(&self.effect_type)),
            ("status", Term::text(self.status.name())),
            ("operation_digest", bin(&self.operation_digest)),
            (
                "idempotency_key",
                Term::optional_bin(self.idempotency_key.as_deref()),
            ),
            ("policy_hash", bin(&self.policy_hash)),
            ("policy_version", Term::Nil),
            ("created_at_ms", Term::int(self.created_at_ms)),
            ("updated_at_ms", Term::int(self.updated_at_ms)),
        ];
        optional(&mut entries, "external_id", self.external_id.as_ref());
        optional(&mut entries, "error", self.error.as_ref());
        optional(&mut entries, "reason", self.reason.as_ref());
        if let Some(latency) = self.latency_ms {
            entries.push(("latency_ms", Term::int(latency)));
        }
        if let Some(decision) = decision {
            entries.push(("decision", Term::text(decision)));
        }
        map(entries)
    }
}

impl Event {
    /// The event as `FLOW.GOVERNANCE.LEDGER` lists it.
    #[must_use]
    pub fn view(&self, owner: Owner<'_>) -> Term {
        let status = match self.kind {
            Kind::Effect(status) => status.name(),
            Kind::EffectDenied => "denied",
        };
        let mut entries = vec![
            ("id", bin(&self.id)),
            ("flow_id", bin(owner.id)),
            ("partition_key", bin(owner.partition_key)),
            ("kind", Term::text(&self.kind.name())),
            ("at_ms", Term::int(self.at_ms)),
            ("effect_key", bin(&self.effect_key)),
            ("effect_type", bin(&self.effect_type)),
            ("status", Term::text(status)),
            ("policy_hash", bin(&self.policy_hash)),
            ("policy_version", Term::Nil),
        ];
        if let Some([code, message, policy]) = &self.denial {
            entries.extend([
                ("code", bin(code)),
                ("message", bin(message)),
                ("policy", bin(policy)),
            ]);
        }
        map(entries)
    }

    fn term(&self) -> Term {
        let mut entries = vec![
            ("id", bin(&self.id)),
            ("kind", Term::text(&self.kind.name())),
            ("at_ms", Term::int(self.at_ms)),
            ("effect_key", bin(&self.effect_key)),
            ("effect_type", bin(&self.effect_type)),
            ("policy_hash", bin(&self.policy_hash)),
        ];
        if let Some(denial) = &self.denial {
            entries.push((
                "denial",
                Term::List(denial.iter().map(|part| bin(part)).collect()),
            ));
        }
        map(entries)
    }

    fn from_term(term: &Term) -> Option<Self> {
        let denial = match term.get(b"denial") {
            None => None,
            Some(Term::List(parts)) => match parts.as_slice() {
                [Term::Bin(code), Term::Bin(message), Term::Bin(policy)] => {
                    Some([code.clone(), message.clone(), policy.clone()])
                }
                _ => return None,
            },
            Some(_) => return None,
        };
        Some(Self {
            id: text(term, "id")?,
            kind: Kind::parse(&text(term, "kind")?)?,
            at_ms: integer(term, "at_ms")?,
            effect_key: text(term, "effect_key")?,
            effect_type: text(term, "effect_type")?,
            policy_hash: text(term, "policy_hash")?,
            denial,
        })
    }
}

fn text(term: &Term, key: &str) -> Option<Vec<u8>> {
    match term.get(key.as_bytes()) {
        Some(Term::Bin(value)) => Some(value.clone()),
        _ => None,
    }
}

/// A stored field that is neither absent nor of its type.
struct Damaged;

fn optional_text(term: &Term, key: &str) -> Result<Option<Vec<u8>>, Damaged> {
    match term.get(key.as_bytes()) {
        None | Some(Term::Nil) => Ok(None),
        Some(Term::Bin(value)) => Ok(Some(value.clone())),
        Some(_) => Err(Damaged),
    }
}

fn integer(term: &Term, key: &str) -> Option<u64> {
    match term.get(key.as_bytes()) {
        Some(Term::Int(value)) => u64::try_from(*value).ok(),
        _ => None,
    }
}

impl Effect {
    fn term(&self, effect_key: &[u8]) -> Term {
        let owner = Owner {
            id: b"",
            partition_key: b"",
        };
        self.view(owner, effect_key, None)
    }

    fn from_term(term: &Term) -> Option<(Vec<u8>, Self)> {
        let latency_ms = match term.get(b"latency_ms") {
            None => None,
            Some(_) => Some(integer(term, "latency_ms")?),
        };
        let effect = Self {
            effect_type: text(term, "effect_type")?,
            flow_type: text(term, "type")?,
            state: text(term, "state")?,
            status: Status::parse(&text(term, "status")?)?,
            operation_digest: text(term, "operation_digest")?,
            idempotency_key: optional_text(term, "idempotency_key").ok()?,
            policy_hash: text(term, "policy_hash")?,
            created_at_ms: integer(term, "created_at_ms")?,
            updated_at_ms: integer(term, "updated_at_ms")?,
            external_id: optional_text(term, "external_id").ok()?,
            error: optional_text(term, "error").ok()?,
            reason: optional_text(term, "reason").ok()?,
            latency_ms,
        };
        Some((text(term, "effect_key")?, effect))
    }
}

impl Governance {
    /// Whether there is nothing to keep.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.effects.is_empty() && self.ledger.is_empty() && self.denials == 0
    }

    /// The stored form, an encoded term.
    #[must_use]
    pub fn encode(&self) -> Option<Vec<u8>> {
        let term = Term::List(vec![
            Term::List(
                self.effects
                    .iter()
                    .map(|(key, effect)| effect.term(key))
                    .collect(),
            ),
            Term::List(self.ledger.iter().map(Event::term).collect()),
            Term::int(self.denials),
        ]);
        let mut out = Vec::new();
        term.encode(&mut out)?;
        Some(out)
    }

    /// Reads the stored form; `None` when it is not one.
    #[must_use]
    pub fn decode(bytes: &[u8]) -> Option<Self> {
        let Some((Term::List(parts), [])) = Term::decode(bytes) else {
            return None;
        };
        let [Term::List(effects), Term::List(ledger), Term::Int(denials)] = parts.as_slice() else {
            return None;
        };
        Some(Self {
            effects: effects
                .iter()
                .map(Effect::from_term)
                .collect::<Option<_>>()?,
            ledger: ledger.iter().map(Event::from_term).collect::<Option<_>>()?,
            denials: u64::try_from(*denials).ok()?,
        })
    }

    /// `Ledger.put_event/3`: appends `event` in time order (an equal event
    /// already present is kept once), then keeps the newest
    /// [`LEDGER_MAX_EVENTS`] within [`LEDGER_MAX_BYTES`].
    ///
    /// # Errors
    /// Elixir's text when another event holds the id.
    pub fn append(&mut self, event: Event) -> Result<(), String> {
        if let Some(existing) = self.ledger.iter().find(|held| held.id == event.id) {
            return if *existing == event {
                Ok(())
            } else {
                Err("ERR flow governance ledger event id already exists".to_owned())
            };
        }
        self.ledger.push(event);
        // A stable sort keeps arrival order among events at one time.
        self.ledger.sort_by_key(|event| event.at_ms);
        let excess = self.ledger.len().saturating_sub(LEDGER_MAX_EVENTS);
        self.ledger.drain(..excess);
        // Drop the oldest until the encoded list fits, keeping the newest.
        let sizes: Vec<usize> = self.ledger.iter().map(encoded_size).collect();
        let mut total = sizes
            .iter()
            .fold(LIST_HEADER_BYTES, |sum, size| sum.saturating_add(*size));
        let mut dropped: usize = 0;
        for size in &sizes {
            if total <= LEDGER_MAX_BYTES || dropped.saturating_add(1) >= sizes.len() {
                break;
            }
            total = total.saturating_sub(*size);
            dropped = dropped.saturating_add(1);
        }
        self.ledger.drain(..dropped);
        Ok(())
    }
}

/// A list's tag and count before its items.
const LIST_HEADER_BYTES: usize = 5;

fn encoded_size(event: &Event) -> usize {
    let mut out = Vec::new();
    match event.term().encode(&mut out) {
        Some(()) => out.len(),
        None => usize::MAX,
    }
}
