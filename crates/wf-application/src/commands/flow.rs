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
pub mod create_many;
pub mod effect;
pub mod get;
pub mod governance;
pub mod history;
pub mod info;
pub mod json;
pub mod keys;
pub mod many;
pub mod meta;
pub mod mutate;
mod opts;
pub mod policy;
pub mod query;
pub mod record;
pub mod rewind;
pub mod schedule;
pub mod signal;
pub mod start;
pub mod term;
pub mod text;
pub mod value;
pub mod value_codec;

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
    /// `FLOW.CREATE_MANY` beyond the queue fast path (audit gap G2).
    CreateMany(Box<create_many::CreateMany>),
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
    /// `FLOW.QUERY`.
    Query(Box<query::Query>),
    /// `FLOW.QUERY.INDEXES`, a text command.
    QueryIndexes(query::indexes::Indexes),
    /// A `FLOW.SCHEDULE.*` command.
    Schedule(Box<schedule::Command>),
    /// A `FLOW.EFFECT.*` command or `FLOW.GOVERNANCE.LEDGER`.
    Effect(Box<effect::Command>),
    /// A governance record command (ADR-014 step 15).
    Governance(Box<governance::Command>),
}

impl FlowCommand {
    /// Whether it only reads: a group member that does not lead answers it
    /// once it applied what the leader had committed (ADR-018).
    #[must_use]
    pub const fn reads_only(&self) -> bool {
        match self {
            Self::Get(_)
            | Self::History(_)
            | Self::Query(_)
            | Self::QueryIndexes(_)
            | Self::ValueMget(_)
            | Self::PolicyGet(_)
            | Self::Info(_) => true,
            Self::Governance(command) => !command.listed().is_empty(),
            _ => false,
        }
    }

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
            // A point read or history without a partition is in the run
            // id's auto bucket; collections and counts name a partition.
            Self::Query(query) => Some((query.run_id().unwrap_or_default(), query.partition_key())),
            Self::SpawnChildren(spawn) => Some((&spawn.parent_id, Some(&spawn.partition_key))),
            Self::StartAndClaim(start) => {
                Some((&start.create.id, start.create.partition_key.as_deref()))
            }
            Self::Effect(command) => Some(command.target()),
            // A record lives where its name, as a partition, routes.
            Self::Governance(command) => command.record().map(|(_, name)| (name, Some(name))),
            Self::Many(_)
            | Self::CreateMany(_)
            | Self::ValueMget(_)
            | Self::RunSteps(_)
            | Self::PolicySet(_)
            | Self::PolicyGet(_)
            | Self::RetentionCleanup(_)
            | Self::Info(_)
            | Self::QueryIndexes(_)
            | Self::Schedule(_) => None,
        }
    }
}

/// The options without the client's clock reading: `now_ms` at the top
/// level and in batch items and children. A node that decides with its own
/// clock drops it before reading a client's command (ADR-015).
#[must_use]
pub fn without_client_time(args: &Term) -> Term {
    let Term::Map(entries) = args else {
        return args.clone();
    };
    Term::Map(
        entries
            .iter()
            .filter(|(key, _)| key.as_slice() != b"now_ms")
            .map(|(key, value)| match (key.as_slice(), value) {
                (b"items" | b"children", Term::List(items)) => (
                    key.clone(),
                    Term::List(items.iter().map(without_client_time).collect()),
                ),
                _ => (key.clone(), value.clone()),
            })
            .collect(),
    )
}

/// Elixir's `Internal.reject_reserved_id/2`: a client may not read or
/// change a schedule's internal Flow (its history stays readable). A batch
/// naming one is refused whole, as Elixir builds every item first; creates
/// refuse it while reading (`create::parse`) and single mutations in the
/// domain (`reduce_profile`).
fn public_ids(command: FlowCommand) -> Result<FlowCommand, FlowError> {
    let ids: Vec<&[u8]> = match &command {
        FlowCommand::Get(get) => vec![&get.id],
        FlowCommand::Many(batch) => batch.items.iter().map(|item| item.id.as_slice()).collect(),
        FlowCommand::Rewind(rewind) => vec![&rewind.id],
        FlowCommand::SpawnChildren(spawn) => vec![&spawn.parent_id],
        _ => Vec::new(),
    };
    if ids
        .into_iter()
        .any(|id| id.starts_with(schedule::FLOW_ID_PREFIX.as_bytes()))
    {
        return Err(FlowError::rejected(
            "ERR flow id is reserved for internal use".to_owned(),
        ));
    }
    Ok(command)
}

/// Reads command `name` with options `args`.
///
/// # Errors
/// The Elixir server's refusal text for invalid options, or an unknown
/// command.
pub fn parse(name: &str, args: &Term) -> Result<FlowCommand, FlowError> {
    commands(name, args).and_then(public_ids)
}

fn commands(name: &str, args: &Term) -> Result<FlowCommand, FlowError> {
    match name {
        "FLOW.CREATE" => create::parse(args).map(|create| FlowCommand::Create(Box::new(create))),
        "FLOW.CREATE_MANY" => {
            create_many::parse(args).map(|batch| FlowCommand::CreateMany(Box::new(batch)))
        }
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
        name if governance::is_governance(name) => {
            governance::parse(name, args).map(|command| FlowCommand::Governance(Box::new(command)))
        }
        name if effect::is_effect(name) => {
            effect::parse(name, args).map(|command| FlowCommand::Effect(Box::new(command)))
        }
        name if name.starts_with("FLOW.SCHEDULE.") => {
            schedule::parse(name, args).map(|command| FlowCommand::Schedule(Box::new(command)))
        }
        "FLOW.QUERY" => query::parse(args).map(|query| FlowCommand::Query(Box::new(query))),
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
