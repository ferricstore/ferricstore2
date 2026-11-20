//! The queue workflow service (ADR-011).
//!
//! Batches of Flows are created, claimed and completed through one service
//! that the node owns. The service decides every record with the domain
//! reducer, makes the decision durable, and only then calls the reply.
//! Transports translate their own requests into these types and never touch
//! shard state. Partition routing matches the Elixir server.

use std::fmt;

use wf_domain::value::DomainError;

pub use crate::routing::{Partition, SLOTS, shard_for_slot};

/// One Flow to create.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewFlow {
    /// Flow id.
    pub id: Vec<u8>,
    /// Partition; the auto bucket of the id when absent.
    pub partition: Option<Vec<u8>>,
    /// Payload, if any.
    pub payload: Option<Vec<u8>>,
}

/// Flows of one type and state. Each Flow succeeds or fails on its own.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CreateFlows {
    /// Flow type.
    pub flow_type: Vec<u8>,
    /// Initial state.
    pub state: Vec<u8>,
    /// First eligible time; the committed time when absent.
    pub run_at_ms: Option<u64>,
    /// Claim priority, 0 through 2.
    pub priority: u8,
    /// The Flows, in request order.
    pub flows: Vec<NewFlow>,
}

/// A claim of due Flows of one type.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClaimFlows {
    /// Flow type.
    pub flow_type: Vec<u8>,
    /// Required state; any state when absent.
    pub state: Option<Vec<u8>>,
    /// Claiming worker.
    pub worker: Vec<u8>,
    /// Lease length.
    pub lease_ms: u64,
    /// Most Flows to claim.
    pub limit: usize,
    /// Only this priority; higher priorities first when absent.
    pub priority: Option<u8>,
    /// Only these partitions; any partition when absent.
    pub partitions: Option<Vec<Vec<u8>>>,
}

/// A claimed Flow as the client sees it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClaimedFlow {
    /// Flow id.
    pub id: Vec<u8>,
    /// Partition name.
    pub partition: Vec<u8>,
    /// Lease token to present on completion.
    pub lease_token: Vec<u8>,
    /// Fencing token to present on completion.
    pub fence: u64,
    /// Logical state the Flow was claimed from.
    pub run_state: Vec<u8>,
}

/// One completion.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompleteFlow {
    /// Flow id.
    pub id: Vec<u8>,
    /// Partition, if the client sent it; the auto bucket of the id when absent.
    pub partition: Option<Vec<u8>>,
    /// Lease token from the claim.
    pub lease_token: Vec<u8>,
    /// Fencing token from the claim.
    pub fence: u64,
    /// Result, if any.
    pub result: Option<Vec<u8>>,
}

/// Longest accepted Flow id, as in the Elixir server.
pub const MAX_FLOW_ID_BYTES: usize = 256;

/// One item's outcome; the error renders as the Elixir reply text.
pub type ItemOutcome = Result<(), ServiceError>;

/// Why an item or a whole request failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ServiceError {
    /// A domain rule refused it.
    Domain(DomainError),
    /// The Flow id is empty or longer than [`MAX_FLOW_ID_BYTES`].
    InvalidId,
    /// The node could not make the decision durable or is shutting down.
    Unavailable,
}

impl fmt::Display for ServiceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Domain(error) => error.fmt(formatter),
            Self::InvalidId => formatter.write_str("ERR flow id must be 1 to 256 bytes"),
            Self::Unavailable => formatter.write_str("ERR flow store unavailable"),
        }
    }
}

impl std::error::Error for ServiceError {}

/// Called exactly once, after the outcome is durable.
pub type Reply<T> = Box<dyn FnOnce(Result<T, ServiceError>) + Send>;

/// The node-owned queue workflow service.
pub trait QueueService: Send + Sync {
    /// Creates Flows; one outcome per Flow, in request order.
    fn create(&self, request: CreateFlows, reply: Reply<Vec<ItemOutcome>>);

    /// Claims up to `limit` due Flows.
    fn claim(&self, request: ClaimFlows, reply: Reply<Vec<ClaimedFlow>>);

    /// Completes claimed Flows; one outcome per completion, in request order.
    fn complete(&self, completions: Vec<CompleteFlow>, reply: Reply<Vec<ItemOutcome>>);
}
