//! Neutral preparation values; no submission or consensus authority is granted.

use std::fmt;

use wf_domain::policy::installation::{InstallCommand, PolicyValue};
use wf_domain::workflow::CommittedCommand;
use wf_domain::workflow::request::{ClientIntent, RequestScope};
use wf_types::identity::{PartitionId, RequestId};
use wf_types::version::CommittedTime;

mod claim;

pub use claim::{PreparedClaimPhase, PreparedClaimQuery};

/// One explicitly prepared item; current policies remain authoritative apply state.
#[derive(Clone, Eq, PartialEq)]
pub struct PreparedBatchItem {
    /// Exact server-resolved logical partition, revalidated at ordered apply.
    pub partition: PartitionId,
    /// Typed operation with every generated input and execution time explicit.
    pub command: CommittedCommand,
    /// Source creation priority; existing-record priority is read from authority.
    pub create_priority: u8,
}

/// Represented private operations; later variants require versioned codec controls.
#[derive(Clone, Eq, PartialEq)]
pub enum PreparedOperation {
    /// Original ordered bounded batch; no authoritative policy is serialized here.
    Batch(Vec<PreparedBatchItem>),
    /// One bounded physical-state query; committed apply chooses authoritative rows.
    ClaimPhase(PreparedClaimPhase),
    /// Prepared policy value; coherent current authority is checked at ordered apply.
    InstallPolicy {
        /// Exact normalized caller patch/CAS/mode intent, independent of the value.
        command: InstallCommand,
        /// One normalized snapshot and one private encoded envelope owner.
        value: Box<PolicyValue>,
    },
}

/// Owned, explicitly prepared request values, without an assigned Raft log ID.
///
/// Construction and encoding confer no authorization, submission, apply or
/// durability authority. Raw public DTOs must never deserialize directly here.
/// Current guards/policies and placements are revalidated by the apply owner.
#[derive(Clone, Eq, PartialEq)]
pub struct PreparedRequest {
    /// Original server-bound tenant/group/recovery scope across client retries.
    pub scope: RequestScope,
    /// Stable request identity; unrelated to a transport call identifier.
    pub request: RequestId,
    /// Versioned normalized caller intent, excluding generated preparation inputs.
    pub intent: ClientIntent,
    /// Admitted trusted leader time, never a public client's authority.
    pub stamp: CommittedTime,
    /// Complete supported typed command with explicit optionality and order.
    pub operation: PreparedOperation,
}

impl fmt::Debug for PreparedRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PreparedRequest")
            .field("intent_version", &self.intent.version())
            .finish_non_exhaustive()
    }
}
