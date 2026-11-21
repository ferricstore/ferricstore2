//! `FLOW.EFFECT.*` and `FLOW.GOVERNANCE.LEDGER` (ADR-014 step 14), as
//! `Ferricstore.Flow.Governance.Effect` and `Governance.Ledger` decide them.
//!
//! An effect is a side effect a leased Flow announces before it performs
//! it: reserved once per key (a retry with the same `operation_digest` is
//! told it was already reserved; another digest is a conflict), then moved
//! to one terminal status. Each reservation and terminal move, and each
//! refusal by policy, is appended to the Flow's governance ledger. Effects
//! and the ledger belong to the Flow: they live in its stored fields and
//! go when it is removed, as Elixir ties them to the Flow's retention.
//!
//! Effects are decided under the resolved governance policy of the Flow's
//! state. A leased Flow is `running`, and no policy may name that state;
//! the typed protocol has no type-level governance. Every effect is
//! therefore decided under Elixir's default policy: any effect type, an
//! idempotency key required, no approvals and no circuits.

use super::FlowError;
use super::term::Term;

pub mod decide;
pub mod denial;
pub mod request;
pub mod store;
#[cfg(test)]
mod tests;

pub use request::{Get, Lease, LedgerList, Reserve, ReserveFields, Update};
pub use store::Governance;

/// Longest operation digest, idempotency key, external id, error or reason.
pub const MAX_FIELD_BYTES: usize = 262_144;
/// Longest id, effect key, effect type, partition key or storage key.
pub const MAX_KEY_BYTES: usize = 65_535;
/// Most events a Flow's ledger keeps; older ones are dropped.
pub const LEDGER_MAX_EVENTS: usize = 1_000;
/// Largest encoded ledger; the oldest events are dropped to fit.
pub const LEDGER_MAX_BYTES: usize = 900_000;

/// Where an effect is in its lifecycle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    /// Announced, not yet performed.
    Reserved,
    /// Performed.
    Confirmed,
    /// Failed.
    Failed,
    /// Undone.
    Compensated,
}

impl Status {
    /// The status as Elixir names it.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Reserved => "reserved",
            Self::Confirmed => "confirmed",
            Self::Failed => "failed",
            Self::Compensated => "compensated",
        }
    }

    /// The status named `name`.
    #[must_use]
    pub fn parse(name: &[u8]) -> Option<Self> {
        [
            Self::Reserved,
            Self::Confirmed,
            Self::Failed,
            Self::Compensated,
        ]
        .into_iter()
        .find(|status| status.name().as_bytes() == name)
    }
}

/// One parsed effect or ledger command.
#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    /// `FLOW.EFFECT.RESERVE`.
    Reserve(Reserve),
    /// `FLOW.EFFECT.CONFIRM`, `FAIL` or `COMPENSATE`.
    Update(Update),
    /// `FLOW.EFFECT.GET`.
    Get(Get),
    /// `FLOW.GOVERNANCE.LEDGER`.
    Ledger(LedgerList),
}

impl Command {
    /// The Flow the command addresses and its explicit partition.
    #[must_use]
    pub fn target(&self) -> (&[u8], Option<&[u8]>) {
        let (id, partition) = match self {
            Self::Reserve(reserve) => (&reserve.id, &reserve.partition_key),
            Self::Update(update) => (&update.id, &update.partition_key),
            Self::Get(get) => (&get.id, &get.partition_key),
            Self::Ledger(list) => (&list.id, &list.partition_key),
        };
        (id, partition.as_deref())
    }

    /// Whether the command only reads.
    #[must_use]
    pub const fn reads(&self) -> bool {
        matches!(self, Self::Get(_) | Self::Ledger(_))
    }
}

/// Whether `name` is an effect or ledger command.
#[must_use]
pub fn is_effect(name: &str) -> bool {
    name.starts_with("FLOW.EFFECT.") || name == "FLOW.GOVERNANCE.LEDGER"
}

/// Reads command `name` with options `args`.
///
/// # Errors
/// Elixir's text for an invalid option, or an unknown command.
pub fn parse(name: &str, args: &Term) -> Result<Command, FlowError> {
    match name {
        "FLOW.EFFECT.RESERVE" => request::reserve(args).map(Command::Reserve),
        "FLOW.EFFECT.CONFIRM" => {
            request::update(name, args, Status::Confirmed).map(Command::Update)
        }
        "FLOW.EFFECT.FAIL" => request::update(name, args, Status::Failed).map(Command::Update),
        "FLOW.EFFECT.COMPENSATE" => {
            request::update(name, args, Status::Compensated).map(Command::Update)
        }
        "FLOW.EFFECT.GET" => request::get(args).map(Command::Get),
        "FLOW.GOVERNANCE.LEDGER" => request::ledger(args).map(Command::Ledger),
        _ => Err(FlowError::rejected(format!(
            "ERR {name} is not supported by this server"
        ))),
    }
}
