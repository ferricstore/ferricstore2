//! Every Flow command beyond the queue fast path (ADR-014).
//!
//! A transport hands a command its name and its options as a [`Term`] map.
//! [`parse`] reads them as the Elixir server does, with the same defaults and
//! `ERR ...` texts, into a typed [`FlowCommand`]. The node decides it on the
//! shard that owns the Flow and replies with the [`Term`] the Elixir server
//! would send.

pub mod claim;
pub mod create;
pub mod get;
pub mod history;
pub mod keys;
pub mod many;
pub mod meta;
pub mod mutate;
mod opts;
pub mod record;
pub mod term;
pub mod value;

pub use opts::MAX_EXACT;
pub use term::Term;

pub use super::queue::MAX_FLOW_ID_BYTES;
use super::queue::{Reply, ServiceError};

/// Why a Flow command was refused.
pub type FlowError = ServiceError;

/// One parsed Flow command.
#[derive(Clone, Debug, PartialEq)]
pub enum FlowCommand {
    /// `FLOW.CREATE`.
    Create(Box<create::CreateFlow>),
    /// `FLOW.GET`.
    Get(get::GetFlow),
    /// `FLOW.HISTORY`.
    History(history::HistoryQuery),
    /// A single-Flow lifecycle mutation.
    Mutate(Box<mutate::Mutation>),
    /// A batch of lifecycle mutations.
    Many(Box<many::ManyMutation>),
    /// `FLOW.VALUE.PUT`.
    ValuePut(Box<value::ValuePut>),
    /// `FLOW.VALUE.MGET`.
    ValueMget(value::ValueMget),
}

impl FlowCommand {
    /// The one Flow the command addresses, its id and explicit partition;
    /// `None` for a command over several Flows.
    #[must_use]
    pub fn target(&self) -> Option<(&[u8], Option<&[u8]>)> {
        match self {
            Self::Create(create) => Some((&create.id, create.partition_key.as_deref())),
            Self::Get(get) => Some((&get.id, get.partition_key.as_deref())),
            Self::History(query) => Some((&query.id, query.partition_key.as_deref())),
            Self::Mutate(mutation) => Some((&mutation.id, mutation.partition_key.as_deref())),
            Self::ValuePut(put) => put
                .named
                .as_ref()
                .map(|(owner, _)| (owner.as_slice(), put.partition_key.as_deref())),
            Self::Many(_) | Self::ValueMget(_) => None,
        }
    }
}

/// Reads command `name` with options `args`.
///
/// # Errors
/// The Elixir server's refusal text for invalid options, or an unknown
/// command.
pub fn parse(name: &str, args: &Term) -> Result<FlowCommand, FlowError> {
    match name {
        "FLOW.CREATE" => create::parse(args).map(|create| FlowCommand::Create(Box::new(create))),
        "FLOW.GET" => get::parse(args).map(FlowCommand::Get),
        "FLOW.HISTORY" => history::parse(args).map(FlowCommand::History),
        "FLOW.VALUE.PUT" => value::parse_put(args).map(|put| FlowCommand::ValuePut(Box::new(put))),
        "FLOW.VALUE.MGET" => value::parse_mget(args).map(FlowCommand::ValueMget),
        name if many::single(name).is_some() => {
            many::parse(name, args).map(|batch| FlowCommand::Many(Box::new(batch)))
        }
        name if mutate::is_mutation(name) => {
            mutate::parse(name, args).map(|mutation| FlowCommand::Mutate(Box::new(mutation)))
        }
        _ => Err(FlowError::rejected(format!(
            "ERR {name} is not supported by this server"
        ))),
    }
}

/// Runs Flow commands; implemented by the node.
pub trait FlowService: Send + Sync {
    /// Decides `command` and replies once its outcome is durable.
    fn execute(&self, command: FlowCommand, reply: Reply<Term>);
}

/// Everything a transport needs from the node: the queue fast path and the
/// other Flow commands.
pub trait NodeService: super::queue::QueueService + FlowService {}

impl<T: super::queue::QueueService + FlowService> NodeService for T {}
