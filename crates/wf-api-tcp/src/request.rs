//! FSNP v1 workflow request bodies (ADR-011): typed-map `FLOW.CREATE`,
//! `FLOW.CREATE_MANY` and `FLOW.COMPLETE`, and the compact `FLOW.CLAIM_DUE`
//! and `FLOW.COMPLETE_MANY` payloads. Options this server does not implement
//! are refused by name instead of being ignored. Parsing checks shape only;
//! size limits, authorization and partition placement belong to the
//! application.

mod compact;
mod compact_many;
mod term;
mod typed;

use std::fmt;

pub use compact::{claim_due, complete_many};
pub use compact_many::{is_compact, many_options};
pub use term::{PipelineCommand, options, pipeline};
pub use typed::{complete, create, create_many, startup};

/// A refused request body. The message is the reply text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RequestError {
    message: String,
}

impl RequestError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    /// The reply text, in the Elixir server's `ERR ...` form.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for RequestError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for RequestError {}

/// `FLOW.CREATE_MANY`: one type, state and schedule for many Flows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CreateMany {
    /// Flow type.
    pub flow_type: Vec<u8>,
    /// Initial state.
    pub state: Vec<u8>,
    /// Client clock reading, if sent.
    pub now_ms: Option<i64>,
    /// First eligible time, if not immediately.
    pub run_at_ms: Option<i64>,
    /// Claim priority; 0 when absent.
    pub priority: i64,
    /// Items succeed or fail independently instead of all or nothing.
    pub independent: bool,
    /// Reply with a single `OK` when every item succeeds.
    pub ok_on_success: bool,
    /// Partition for items without their own.
    pub partition_key: Option<Vec<u8>>,
    /// The Flows, in request order.
    pub items: Vec<CreateItem>,
}

/// One Flow of a `FLOW.CREATE_MANY`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CreateItem {
    /// Flow id.
    pub id: Vec<u8>,
    /// Partition, if given for this item.
    pub partition_key: Option<Vec<u8>>,
    /// Payload, if any.
    pub payload: Option<Vec<u8>>,
}

/// `FLOW.CREATE` of one Flow.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Create {
    /// Flow id.
    pub id: Vec<u8>,
    /// Flow type.
    pub flow_type: Vec<u8>,
    /// Initial state.
    pub state: Vec<u8>,
    /// Client clock reading, if sent.
    pub now_ms: Option<i64>,
    /// Partition, if given.
    pub partition_key: Option<Vec<u8>>,
    /// Payload, if any.
    pub payload: Option<Vec<u8>>,
    /// First eligible time, if not immediately.
    pub run_at_ms: Option<i64>,
    /// Claim priority; 0 when absent.
    pub priority: i64,
}

/// `FLOW.COMPLETE` of one claimed Flow.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Complete {
    /// Flow id.
    pub id: Vec<u8>,
    /// Partition, if given.
    pub partition_key: Option<Vec<u8>>,
    /// Lease token from the claim.
    pub lease_token: Vec<u8>,
    /// Fencing token from the claim.
    pub fencing_token: i64,
    /// Client clock reading, if sent.
    pub now_ms: Option<i64>,
}

/// Which fields each claimed job carries in the reply.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClaimShape {
    /// Id, partition, lease token and fencing token.
    Jobs,
    /// The job fields plus the run state.
    JobsState,
    /// The job fields plus the attributes map.
    JobsAttributes,
    /// The job fields plus run state and attributes.
    JobsStateAttributes,
}

/// The partitions a claim may take Flows from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Partitions {
    /// Any partition.
    Any,
    /// One partition.
    One(Vec<u8>),
    /// Any of these partitions.
    Many(Vec<Vec<u8>>),
}

/// `FLOW.CLAIM_DUE` in its compact form.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClaimDue {
    /// Flow type.
    pub flow_type: Vec<u8>,
    /// Required state, if any.
    pub state: Option<Vec<u8>>,
    /// Claiming worker.
    pub worker: Vec<u8>,
    /// Lease length.
    pub lease_ms: i64,
    /// Most Flows to claim.
    pub limit: i64,
    /// How long to wait for due Flows, if at all.
    pub block_ms: Option<i64>,
    /// Also take Flows whose lease expired.
    pub reclaim_expired: bool,
    /// Share of the limit for expired leases, as sent.
    pub reclaim_ratio: i64,
    /// Only this priority, if given.
    pub priority: Option<i64>,
    /// Reply shape.
    pub shape: ClaimShape,
    /// Partitions to claim from.
    pub partitions: Partitions,
}

/// `FLOW.COMPLETE_MANY` in its compact form.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompleteMany {
    /// Partition for items without their own.
    pub partition_key: Option<Vec<u8>>,
    /// Client clock reading.
    pub now_ms: i64,
    /// Items succeed or fail independently.
    pub independent: bool,
    /// The client asked for terminal-local-only completion.
    pub terminal_local_only: bool,
    /// Reply with a single `OK` when every item succeeds.
    pub ok_on_success: bool,
    /// The completions, in request order.
    pub items: Vec<CompleteItem>,
}

/// One completion of a `FLOW.COMPLETE_MANY`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompleteItem {
    /// Flow id.
    pub id: Vec<u8>,
    /// Partition, if given for this item.
    pub partition_key: Option<Vec<u8>>,
    /// Lease token from the claim.
    pub lease_token: Vec<u8>,
    /// Fencing token from the claim.
    pub fencing_token: i64,
}

/// What a client asked for in `STARTUP`. Other keys are accepted and unused.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Startup {
    /// The client accepts compact Flow replies.
    pub compact_flow_responses: bool,
    /// Compact reply codecs the client can read, by name.
    pub codecs: Vec<Vec<u8>>,
}
