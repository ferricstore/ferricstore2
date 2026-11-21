//! The queue workflow service (ADR-011).
//!
//! Batches of Flows are created, claimed and completed through one service
//! that the node owns. The service decides every record with the domain
//! reducer, makes the decision durable, and only then calls the reply.
//! Transports translate their own requests into these types and never touch
//! shard state. Partition routing matches the Elixir server.

use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use wf_domain::value::DomainError;

use super::flow::Term;
use super::flow::record::Include;

pub use crate::routing::{Partition, SLOTS, shard_for_slot, slot_for_tag};

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
    /// Allowed states; any state but `running` when empty. A claim for
    /// `running` alone takes only expired leases.
    pub states: Vec<Vec<u8>>,
    /// Claiming worker.
    pub worker: Vec<u8>,
    /// Lease length.
    pub lease_ms: u64,
    /// Most Flows to claim.
    pub limit: usize,
    /// Only this priority; higher priorities first when absent.
    pub priority: Option<u8>,
    /// Only these partitions. When absent, auto partitions only (the
    /// buckets of Flows created without one), unless `any_partition`.
    pub partitions: Option<Vec<Vec<u8>>>,
    /// Without listed partitions, claim from every partition (Elixir's
    /// `ANY`) instead of auto partitions only.
    pub any_partition: bool,
    /// Also take Flows whose lease expired, with Elixir's `reclaim_ratio`
    /// (0 to 100): the percentage of the limit reclaimed before due Flows.
    /// No reclaim when absent. A claim for state `running` takes only
    /// expired leases either way.
    pub reclaim: Option<u8>,
    /// Wait for a Flow when none can be claimed: `Some(0)` waits until one
    /// arrives, `Some(ms)` at most that long, then replies with no Flows.
    /// No wait when absent.
    pub block_ms: Option<u64>,
    /// Set when the requester is gone; a waiting claim then replies with no
    /// Flows and takes nothing.
    pub cancellation: Option<Cancellation>,
    /// The client's clock; claims are decided at it when sent, as in Elixir.
    pub now_ms: Option<u64>,
    /// Reply with each claimed Flow's record, including these extras
    /// (ADR-014); jobs only when absent.
    pub records: Option<Include>,
    /// Elixir's governed-claim options as given (ADR-014 step 15).
    pub governance: ClaimGovernance,
}

/// A claim's `governance_limit_scope` and `governance_shard_id`, checked
/// against the node's shard count when the claim starts.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ClaimGovernance {
    /// Either option was given.
    pub given: bool,
    /// The scope, when a non-empty string.
    pub scope: Option<Vec<u8>>,
    /// The shard id, when an integer.
    pub shard: Option<i64>,
}

impl ClaimFlows {
    /// Whether a Flow in `partition` is in the claim's scope; `listed` is
    /// the parsed form of [`Self::partitions`].
    #[must_use]
    pub fn allows(&self, listed: Option<&[Partition]>, partition: &Partition) -> bool {
        match listed {
            Some(listed) => listed.contains(partition),
            None => self.any_partition || matches!(partition, Partition::Auto(_)),
        }
    }

    /// Whether the claim takes only expired leases.
    #[must_use]
    pub fn running_only(&self) -> bool {
        matches!(self.states.as_slice(), [state] if state.as_slice() == b"running")
    }
}

/// Cancels a waiting claim; clones share one flag.
#[derive(Clone, Debug, Default)]
pub struct Cancellation(Arc<AtomicBool>);

impl Cancellation {
    /// A flag that is not set.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the flag for every clone.
    pub fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst);
    }

    /// Whether any clone was cancelled.
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

impl PartialEq for Cancellation {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl Eq for Cancellation {}

/// A claimed Flow as the client sees it.
#[derive(Clone, Debug, PartialEq)]
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
    /// The claimed Flow's record, when the claim asked for records.
    pub record: Option<Term>,
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
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ServiceError {
    /// A domain rule refused it.
    Domain(DomainError),
    /// The Flow id is empty or longer than [`MAX_FLOW_ID_BYTES`].
    InvalidId,
    /// The node could not make the decision durable or is shutting down.
    Unavailable,
    /// Too many claims are already waiting for Flows.
    TooManyWaiting,
    /// The shards the request needs have too much work waiting (ADR-017);
    /// nothing of the request ran, so it is safe to retry.
    Busy,
    /// This member does not lead the request's slot (ADR-018); nothing ran,
    /// and the client should refresh its routes and retry.
    Reroute,
    /// Refused with the Elixir server's reply text.
    Rejected(String),
    /// A malformed request, which Elixir answers with status `bad_request`.
    BadRequest(String),
    /// A failed Flow query, with its diagnostic (ADR-014 step 12).
    Query(Box<super::flow::query::Failure>),
    /// A structured governance refusal (ADR-014 step 14).
    Governance(Box<super::flow::effect::denial::Denial>),
}

impl ServiceError {
    /// A refusal with this reply text.
    #[must_use]
    pub const fn rejected(text: String) -> Self {
        Self::Rejected(text)
    }

    /// `ERR flow <key> <problem>`.
    #[must_use]
    pub fn flow(key: &str, problem: &str) -> Self {
        Self::Rejected(format!("ERR flow {key} {problem}"))
    }
}

impl fmt::Display for ServiceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Domain(error) => error.fmt(formatter),
            Self::InvalidId => formatter.write_str("ERR flow id must be 1 to 256 bytes"),
            Self::Unavailable => formatter.write_str("ERR flow store unavailable"),
            Self::TooManyWaiting => {
                formatter.write_str("ERR too many blocked FLOW.CLAIM_DUE requests are waiting")
            }
            Self::Busy => formatter.write_str("BUSY flow shards are overloaded, retry later"),
            Self::Reroute => {
                formatter.write_str("REROUTE this member does not lead the slot; refresh SHARDS")
            }
            Self::Rejected(text) | Self::BadRequest(text) => formatter.write_str(text),
            Self::Query(failure) => formatter.write_str(failure.message()),
            Self::Governance(denial) => {
                formatter.write_str(&String::from_utf8_lossy(denial.message()))
            }
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

    /// Where a replicated node's group is led (ADR-018); `None` for a node
    /// that is not replicated.
    fn route(&self) -> Option<Route> {
        None
    }
}

/// A replicated group's leadership as one member knows it (ADR-018).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Route {
    /// The leading member, if one is known.
    pub leader: Option<u64>,
    /// The Raft term: it changes whenever leadership does.
    pub term: u64,
}
