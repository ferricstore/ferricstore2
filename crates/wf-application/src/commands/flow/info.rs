//! `FLOW.INFO`, `FLOW.STATS`, `FLOW.ATTRIBUTES` and `FLOW.ATTRIBUTE_VALUES`
//! (ADR-014 step 11), as `Ferricstore.Flow.InfoAPI.info/3` and
//! `Ferricstore.Flow.ReadAPI.stats/3`, `attributes/3` and
//! `attribute_values/4` read and answer them.
//!
//! Each reads the Flows of one type in one partition, or in every auto
//! bucket when no partition is given (`partition_key: :auto`):
//!
//! - INFO counts the five built-in states and the leased Flows.
//! - STATS counts one state, or with `attributes` the Flows matching them;
//!   state `any` needs attributes and sees only attributes the type's policy
//!   indexed when the Flow was last written (Elixir's broad index).
//! - ATTRIBUTES lists the policy's indexed attribute names by how many
//!   values Flows hold, and `ATTRIBUTE_VALUES` one name's values by count.
//!
//! Attribute counts leave out terminal Flows whose retention has ended,
//! as Elixir's projection rows expire; plain state counts keep them until
//! cleanup removes them. Counts are exact: this server scans its in-memory
//! catalog, so it has no scan limit and never marks a count `approximate`.

mod inspect;
mod tally;
#[cfg(test)]
mod tests;

pub use tally::{Counter, Seen, Tally, reply};

use super::FlowError;
use super::meta::{self, Attributes};
use super::opts::Opts;
use super::term::Term;
use crate::routing::Partition;

/// Elixir's default `count` for discovery lists.
const DEFAULT_COUNT: u64 = 100;
/// Elixir's `flow_max_count`.
const MAX_COUNT: u64 = 1_000;
/// The states INFO counts, in reply order.
pub const INFO_STATES: [&[u8]; 5] = [b"queued", b"running", b"completed", b"failed", b"cancelled"];

/// Where a read looks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Scope {
    /// Every auto bucket: Flows created without a partition.
    Auto,
    /// One partition.
    Partition(Partition),
}

/// The state a read looks at.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StateFilter {
    /// Every state, through the broad attribute index.
    Any,
    /// One state.
    Only(Vec<u8>),
}

/// What a read answers.
#[derive(Clone, Debug, PartialEq)]
pub enum Kind {
    /// `FLOW.INFO`.
    Info,
    /// `FLOW.STATS`, with its attribute filters (empty for a state count).
    Stats {
        /// The state counted.
        state: StateFilter,
        /// Attribute filters.
        filters: Attributes,
    },
    /// `FLOW.ATTRIBUTES`.
    Names {
        /// The state looked at.
        state: StateFilter,
        /// Most names listed.
        count: usize,
    },
    /// `FLOW.ATTRIBUTE_VALUES`.
    Values {
        /// The state looked at.
        state: StateFilter,
        /// The attribute name.
        attribute: String,
        /// Most values listed.
        count: usize,
    },
}

/// One read over a type's Flows.
#[derive(Clone, Debug, PartialEq)]
pub struct Read {
    /// The type.
    pub flow_type: Vec<u8>,
    /// Where to look.
    pub scope: Scope,
    /// What to answer.
    pub kind: Kind,
}

impl Read {
    /// The states whose Flows the read visits; `None` for every state.
    #[must_use]
    pub fn states(&self) -> Option<Vec<&[u8]>> {
        let filter = match &self.kind {
            Kind::Info => return Some(INFO_STATES.to_vec()),
            Kind::Stats { state, .. } | Kind::Names { state, .. } | Kind::Values { state, .. } => {
                state
            }
        };
        match filter {
            StateFilter::Any => None,
            StateFilter::Only(state) => Some(vec![state.as_slice()]),
        }
    }

    /// Whether the read counts Flows in `partition`.
    #[must_use]
    pub fn covers(&self, partition: &Partition) -> bool {
        match &self.scope {
            Scope::Auto => matches!(partition, Partition::Auto(_)),
            Scope::Partition(only) => only == partition,
        }
    }
}

fn rejected(text: &str) -> FlowError {
    FlowError::rejected(text.to_owned())
}

/// The type every read names, as the native layer and `validate_type/1`
/// check it.
fn flow_type(opts: &Opts<'_>) -> Result<Vec<u8>, FlowError> {
    match opts.get("type") {
        Some(Term::Bin(flow_type)) if flow_type.is_empty() => {
            Err(rejected("ERR flow type must be a non-empty string"))
        }
        Some(Term::Bin(flow_type)) => Ok(flow_type.clone()),
        _ => Err(rejected("ERR native field type must be binary")),
    }
}

/// `optional_auto_partition_key/1`; an explicit nil reads as absent.
fn scope(opts: &Opts<'_>) -> Result<Scope, FlowError> {
    match opts.get("partition_key") {
        None | Some(Term::Nil) => Ok(Scope::Auto),
        Some(Term::Bin(name)) if !name.is_empty() => Ok(Scope::Partition(Partition::parse(name))),
        Some(_) => Err(rejected(
            "ERR flow partition_key must be a non-empty string",
        )),
    }
}

/// `flow_state/1`: `queued` when absent, `any`, or a non-empty state.
fn state(opts: &Opts<'_>) -> Result<StateFilter, FlowError> {
    match opts.get("state") {
        None => Ok(StateFilter::Only(b"queued".to_vec())),
        Some(Term::Bin(state)) if state.as_slice() == b"any" => Ok(StateFilter::Any),
        Some(Term::Bin(state)) if !state.is_empty() => Ok(StateFilter::Only(state.clone())),
        Some(_) => Err(rejected("ERR flow state must be a non-empty string")),
    }
}

/// `flow_count/1`: 100 when absent, at most 1000.
fn count(opts: &Opts<'_>) -> Result<usize, FlowError> {
    let count = match opts.get("count") {
        None => DEFAULT_COUNT,
        Some(Term::Int(count)) if *count > 0 => u64::try_from(*count).unwrap_or(u64::MAX),
        Some(_) => return Err(rejected("ERR flow count must be a positive integer")),
    };
    if count > MAX_COUNT {
        return Err(rejected(&format!(
            "ERR flow count exceeds maximum {MAX_COUNT}"
        )));
    }
    Ok(usize::try_from(count).unwrap_or(usize::MAX))
}

/// The read hints every read accepts and this server ignores: it has no
/// cold tier and its counts are always current.
fn hints(opts: &Opts<'_>, cold: bool) -> Result<(), FlowError> {
    if cold {
        opts.boolean("include_cold", false)?;
    }
    opts.boolean("consistent_projection", false)?;
    Ok(())
}

/// Reads `FLOW.INFO`, which accepts and ignores the other read options.
///
/// # Errors
/// The Elixir server's refusal text for an invalid option.
pub fn parse_info(args: &Term) -> Result<Read, FlowError> {
    let opts = Opts::new(
        "FLOW.INFO",
        args,
        &[
            "type",
            "partition_key",
            "include_cold",
            "consistent_projection",
            "deadline_ms",
            "count",
            "state",
            "attributes",
        ],
    )?;
    let flow_type = flow_type(&opts)?;
    let scope = scope(&opts)?;
    hints(&opts, true)?;
    Ok(Read {
        flow_type,
        scope,
        kind: Kind::Info,
    })
}

/// Reads `FLOW.STATS`, whose `count` is accepted and ignored.
///
/// # Errors
/// The Elixir server's refusal text for an invalid option.
pub fn parse_stats(args: &Term) -> Result<Read, FlowError> {
    let opts = Opts::new(
        "FLOW.STATS",
        args,
        &[
            "type",
            "state",
            "attributes",
            "partition_key",
            "count",
            "consistent_projection",
            "deadline_ms",
        ],
    )?;
    let flow_type = flow_type(&opts)?;
    let state = state(&opts)?;
    let scope = scope(&opts)?;
    let filters = meta::attributes(opts.get("attributes"))?;
    hints(&opts, false)?;
    if state == StateFilter::Any && filters.is_empty() {
        return Err(rejected("ERR flow state any requires attributes"));
    }
    Ok(Read {
        flow_type,
        scope,
        kind: Kind::Stats { state, filters },
    })
}

/// Reads `FLOW.ATTRIBUTES`.
///
/// # Errors
/// The Elixir server's refusal text for an invalid option.
pub fn parse_attributes(args: &Term) -> Result<Read, FlowError> {
    let opts = Opts::new(
        "FLOW.ATTRIBUTES",
        args,
        &[
            "type",
            "state",
            "partition_key",
            "count",
            "consistent_projection",
            "deadline_ms",
        ],
    )?;
    let flow_type = flow_type(&opts)?;
    let state = state(&opts)?;
    let scope = scope(&opts)?;
    let count = count(&opts)?;
    hints(&opts, false)?;
    Ok(Read {
        flow_type,
        scope,
        kind: Kind::Names { state, count },
    })
}

/// Reads `FLOW.ATTRIBUTE_VALUES`.
///
/// # Errors
/// The Elixir server's refusal text for an invalid option.
pub fn parse_attribute_values(args: &Term) -> Result<Read, FlowError> {
    let opts = Opts::new(
        "FLOW.ATTRIBUTE_VALUES",
        args,
        &[
            "type",
            "attribute",
            "state",
            "partition_key",
            "count",
            "consistent_projection",
            "deadline_ms",
        ],
    )?;
    // The native layer checks both fields are binaries before the type.
    if !matches!(opts.get("type"), Some(Term::Bin(_))) {
        return Err(rejected("ERR native field type must be binary"));
    }
    let Some(Term::Bin(name)) = opts.get("attribute") else {
        return Err(rejected("ERR native field attribute must be binary"));
    };
    let flow_type = flow_type(&opts)?;
    let attribute = meta::attribute_key(name)?;
    let state = state(&opts)?;
    let scope = scope(&opts)?;
    let count = count(&opts)?;
    hints(&opts, false)?;
    Ok(Read {
        flow_type,
        scope,
        kind: Kind::Values {
            state,
            attribute,
            count,
        },
    })
}
