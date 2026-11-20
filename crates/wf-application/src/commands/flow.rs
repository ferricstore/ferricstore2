//! Every Flow command beyond the queue fast path (ADR-014).
//!
//! A transport hands a command its name and its options as a [`Term`] map.
//! [`parse`] reads them as the Elixir server does, with the same defaults and
//! `ERR ...` texts, into a typed [`FlowCommand`]. The node decides it on the
//! shard that owns the Flow and replies with the [`Term`] the Elixir server
//! would send.

pub mod create;
pub mod get;
pub mod keys;
pub mod meta;
mod opts;
pub mod record;
pub mod term;

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
}

impl FlowCommand {
    /// The Flow the command addresses: its id and explicit partition.
    #[must_use]
    pub fn target(&self) -> (&[u8], Option<&[u8]>) {
        match self {
            Self::Create(create) => (&create.id, create.partition_key.as_deref()),
            Self::Get(get) => (&get.id, get.partition_key.as_deref()),
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
