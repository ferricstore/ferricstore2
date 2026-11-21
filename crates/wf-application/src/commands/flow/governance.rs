//! Governance records that are not a Flow's own (ADR-014 step 15):
//! approvals, circuits, budgets and limits, as `Ferricstore.Flow.Governance`
//! keeps them.
//!
//! Each record lives on the shard that owns its name (an approval's id, a
//! circuit's scope) as a partition, as Elixir's key carries the name's hash
//! tag. A command on one record is decided there from the record's stored
//! form; a listing asks every shard for its matching records and joins the
//! parts.

use super::FlowError;
use super::term::Term;

pub mod approval;
pub mod budget;
pub mod circuit;
pub mod limit;
pub mod overview;
pub mod policy;
#[cfg(test)]
mod tests;

/// Longest id, scope, flow id or storage key.
pub const MAX_KEY_BYTES: usize = 65_535;
/// Longest reason, policy hash or other free-text field.
pub const MAX_FIELD_BYTES: usize = 262_144;

/// The kinds of stored record, which partition a shard's store.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Kind {
    /// An approval, by id.
    Approval,
    /// A circuit, by scope.
    Circuit,
    /// A budget, by scope.
    Budget,
    /// A limit, by scope.
    Limit,
}

impl Kind {
    /// The kind's tag in logs and images.
    #[must_use]
    pub const fn tag(self) -> u8 {
        match self {
            Self::Approval => 1,
            Self::Circuit => 2,
            Self::Budget => 3,
            Self::Limit => 4,
        }
    }

    /// The kind tagged `tag`.
    #[must_use]
    pub const fn from_tag(tag: u8) -> Option<Self> {
        match tag {
            1 => Some(Self::Approval),
            2 => Some(Self::Circuit),
            3 => Some(Self::Budget),
            4 => Some(Self::Limit),
            _ => None,
        }
    }
}

/// What a command does to its record.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Change {
    /// Store this form.
    Put(Vec<u8>),
    /// Remove the record.
    Remove,
}

/// One shard's part of a listing: replies with the keys the join orders
/// them by.
pub type Listed = Vec<(Vec<u8>, Term)>;

/// A command's reply, and what it does to its record.
pub type Decided = (Result<Term, FlowError>, Option<Change>);

/// One governance command.
#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    /// A command on one approval.
    Approval(approval::Command),
    /// `FLOW.APPROVAL.LIST`.
    ApprovalList(approval::List),
    /// A command on one circuit.
    Circuit(circuit::Command),
    /// A command on one budget.
    Budget(budget::Command),
    /// `FLOW.BUDGET.LIST`.
    BudgetList(budget::List),
    /// A command on one limit.
    Limit(limit::Command),
    /// `FLOW.LIMIT.LIST`.
    LimitList(limit::List),
    /// `FLOW.GOVERNANCE.OVERVIEW`.
    Overview(Box<overview::Overview>),
}

impl Command {
    /// The record a single-record command addresses: its kind and name.
    #[must_use]
    pub fn record(&self) -> Option<(Kind, &[u8])> {
        match self {
            Self::Approval(command) => Some((Kind::Approval, command.id())),
            Self::Circuit(command) => Some((Kind::Circuit, command.scope())),
            Self::Budget(command) => Some((Kind::Budget, command.scope())),
            Self::Limit(command) => Some((Kind::Limit, &command.scope)),
            Self::ApprovalList(_)
            | Self::BudgetList(_)
            | Self::LimitList(_)
            | Self::Overview(_) => None,
        }
    }

    /// Decides a single-record command from the record's stored form.
    pub fn decide(&self, stored: Option<&[u8]>, wall: u64) -> Decided {
        match self {
            Self::Approval(command) => command.decide(stored, wall),
            Self::Circuit(command) => command.decide(stored, wall),
            Self::Budget(command) => command.decide(stored, wall),
            Self::Limit(command) => command.decide(stored, wall),
            Self::ApprovalList(_)
            | Self::BudgetList(_)
            | Self::LimitList(_)
            | Self::Overview(_) => (Err(FlowError::Unavailable), None),
        }
    }

    /// Checks what needs the node's shard count before the command is
    /// routed: a limit command's shard id.
    ///
    /// # Errors
    /// Elixir's text for a refused shard id or a later option.
    pub fn admit(&mut self, shards: usize) -> Result<(), FlowError> {
        match self {
            Self::Limit(command) => command.admit(shards),
            _ => Ok(()),
        }
    }

    /// The kinds a listing reads; none for a single-record command.
    #[must_use]
    pub const fn listed(&self) -> &'static [Kind] {
        match self {
            Self::ApprovalList(_) => &[Kind::Approval],
            Self::BudgetList(_) => &[Kind::Budget],
            Self::LimitList(_) => &[Kind::Limit],
            Self::Overview(_) => &[Kind::Approval, Kind::Budget, Kind::Limit, Kind::Circuit],
            Self::Approval(_) | Self::Circuit(_) | Self::Budget(_) | Self::Limit(_) => &[],
        }
    }

    /// One shard's part of a listing from its records of the listed kinds:
    /// the matching records' replies, with the keys the join orders them
    /// by.
    #[must_use]
    pub fn select(&self, records: &[(Kind, &[u8])], wall: u64) -> Listed {
        let pick = |kind: Kind| {
            records
                .iter()
                .filter(move |(held, _)| *held == kind)
                .map(|(_, value)| *value)
        };
        match self {
            Self::ApprovalList(list) => list.select(pick(Kind::Approval)),
            Self::BudgetList(list) => list.select(pick(Kind::Budget)),
            Self::LimitList(list) => list.select(pick(Kind::Limit), wall),
            Self::Overview(overview) => overview.select(records, wall),
            Self::Approval(_) | Self::Circuit(_) | Self::Budget(_) | Self::Limit(_) => Vec::new(),
        }
    }

    /// Joins the shards' parts into the reply.
    #[must_use]
    pub fn join(&self, parts: Listed) -> Term {
        match self {
            Self::ApprovalList(list) => list.join(parts),
            Self::BudgetList(list) => list.join(parts),
            Self::LimitList(list) => list.join(parts),
            Self::Overview(overview) => overview.join(&parts),
            Self::Approval(_) | Self::Circuit(_) | Self::Budget(_) | Self::Limit(_) => Term::Nil,
        }
    }
}

/// Whether `name` is a governance command handled here.
#[must_use]
pub fn is_governance(name: &str) -> bool {
    [
        "FLOW.APPROVAL.",
        "FLOW.CIRCUIT.",
        "FLOW.BUDGET.",
        "FLOW.LIMIT.",
    ]
    .iter()
    .any(|prefix| name.starts_with(prefix))
        || name == "FLOW.GOVERNANCE.OVERVIEW"
}

/// Reads command `name` with options `args`.
///
/// # Errors
/// Elixir's text for an invalid option, or an unknown command.
pub fn parse(name: &str, args: &Term) -> Result<Command, FlowError> {
    match name {
        "FLOW.APPROVAL.LIST" => approval::parse_list(args).map(Command::ApprovalList),
        name if name.starts_with("FLOW.CIRCUIT.") => {
            circuit::parse(name, args).map(Command::Circuit)
        }
        "FLOW.BUDGET.LIST" => budget::parse_list(args).map(Command::BudgetList),
        name if name.starts_with("FLOW.BUDGET.") => budget::parse(name, args).map(Command::Budget),
        "FLOW.LIMIT.LIST" => limit::parse_list(args).map(Command::LimitList),
        "FLOW.GOVERNANCE.OVERVIEW" => {
            overview::parse(args).map(|overview| Command::Overview(Box::new(overview)))
        }
        name if name.starts_with("FLOW.LIMIT.") => limit::parse(name, args).map(Command::Limit),
        _ => approval::parse(name, args).map(Command::Approval),
    }
}

/// The native layer's `require_binary/2`.
pub(crate) fn native(args: &Term, key: &str) -> Result<Vec<u8>, FlowError> {
    match args.get(key.as_bytes()) {
        Some(Term::Bin(value)) => Ok(value.clone()),
        _ => Err(FlowError::BadRequest(format!(
            "ERR native field {key} must be binary"
        ))),
    }
}

/// Elixir's refusal of a storage key over [`MAX_KEY_BYTES`].
pub(crate) fn key_size(length: usize) -> Result<(), FlowError> {
    if length > MAX_KEY_BYTES {
        return Err(FlowError::rejected(format!(
            "ERR key too large (max {MAX_KEY_BYTES} bytes)"
        )));
    }
    Ok(())
}
