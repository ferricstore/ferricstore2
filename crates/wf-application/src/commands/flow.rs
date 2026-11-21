//! Every Flow command beyond the queue fast path (ADR-014).
//!
//! A transport hands a command its name and its options as a [`Term`] map.
//! [`parse`] reads them as the Elixir server does, with the same defaults and
//! `ERR ...` texts, into a typed [`FlowCommand`]. The node decides it on the
//! shard that owns the Flow and replies with the [`Term`] the Elixir server
//! would send.

pub mod children;
pub mod claim;
pub mod cleanup;
pub mod create;
pub mod get;
pub mod history;
pub mod info;
pub mod json;
pub mod keys;
pub mod many;
pub mod meta;
pub mod mutate;
mod opts;
pub mod policy;
pub mod record;
pub mod rewind;
pub mod signal;
pub mod start;
pub mod term;
pub mod text;
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
    /// `FLOW.START_AND_CLAIM`.
    StartAndClaim(Box<start::StartAndClaim>),
    /// `FLOW.RUN_STEPS_MANY`.
    RunSteps(Box<start::RunSteps>),
    /// `FLOW.SIGNAL`.
    Signal(Box<signal::Signal>),
    /// `FLOW.SPAWN_CHILDREN`.
    SpawnChildren(Box<children::SpawnChildren>),
    /// `FLOW.POLICY.SET`.
    PolicySet(Box<policy::PolicySet>),
    /// `FLOW.POLICY.GET`.
    PolicyGet(policy::PolicyGet),
    /// `FLOW.RETENTION_CLEANUP`.
    RetentionCleanup(cleanup::RetentionCleanup),
    /// `FLOW.REWIND`.
    Rewind(Box<rewind::Rewind>),
    /// `FLOW.INFO`, `FLOW.STATS`, `FLOW.ATTRIBUTES` or
    /// `FLOW.ATTRIBUTE_VALUES`.
    Info(Box<info::Read>),
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
            Self::Signal(signal) => Some((&signal.id, signal.partition_key.as_deref())),
            Self::Rewind(rewind) => Some((&rewind.id, rewind.partition_key.as_deref())),
            Self::SpawnChildren(spawn) => Some((&spawn.parent_id, Some(&spawn.partition_key))),
            Self::StartAndClaim(start) => {
                Some((&start.create.id, start.create.partition_key.as_deref()))
            }
            Self::Many(_)
            | Self::ValueMget(_)
            | Self::RunSteps(_)
            | Self::PolicySet(_)
            | Self::PolicyGet(_)
            | Self::RetentionCleanup(_)
            | Self::Info(_) => None,
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
        "FLOW.SIGNAL" => signal::parse(args).map(|signal| FlowCommand::Signal(Box::new(signal))),
        "FLOW.SPAWN_CHILDREN" => {
            children::parse(args).map(|spawn| FlowCommand::SpawnChildren(Box::new(spawn)))
        }
        "FLOW.POLICY.SET" => {
            policy::parse_set(args).map(|set| FlowCommand::PolicySet(Box::new(set)))
        }
        "FLOW.POLICY.GET" => policy::parse_get(args).map(FlowCommand::PolicyGet),
        "FLOW.RETENTION_CLEANUP" => cleanup::parse(args).map(FlowCommand::RetentionCleanup),
        "FLOW.REWIND" => rewind::parse(args).map(|rewind| FlowCommand::Rewind(Box::new(rewind))),
        "FLOW.INFO" => info::parse_info(args).map(|read| FlowCommand::Info(Box::new(read))),
        "FLOW.STATS" => info::parse_stats(args).map(|read| FlowCommand::Info(Box::new(read))),
        "FLOW.ATTRIBUTES" => {
            info::parse_attributes(args).map(|read| FlowCommand::Info(Box::new(read)))
        }
        "FLOW.ATTRIBUTE_VALUES" => {
            info::parse_attribute_values(args).map(|read| FlowCommand::Info(Box::new(read)))
        }
        "FLOW.START_AND_CLAIM" => {
            start::parse_start(args).map(|start| FlowCommand::StartAndClaim(Box::new(start)))
        }
        "FLOW.RUN_STEPS_MANY" => {
            start::parse_run_steps(args).map(|steps| FlowCommand::RunSteps(Box::new(steps)))
        }
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
