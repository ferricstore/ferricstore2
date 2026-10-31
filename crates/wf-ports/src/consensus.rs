//! Neutral preparation values; no submission or consensus authority is granted.

use std::fmt;

use wf_domain::policy::installation::{InstallCommand, PolicyValue};
use wf_domain::workflow::CommittedCommand;
use wf_domain::workflow::caller::PublicMutationEvidence;
use wf_domain::workflow::request::{ClientIntent, RequestScope};
use wf_types::identity::{PartitionId, RequestId};
use wf_types::version::CommittedTime;

mod catalog_publication;
mod claim;
pub use catalog_publication::CreditCatalogPublicationPort;
mod membership;
mod mutation;
mod public_claim;
mod public_credit;
pub use public_claim::{PublicClaimReadLimits, PublicClaimRequestLookup, PublicClaimSubmitPort};
mod workflow_release;
pub use public_credit::{CreditRequestLookup, PublicCreditSubmitPort};
pub use workflow_release::{WorkflowReleaseCreditRoute, WorkflowReleaseSubmitPort};
mod value;
pub use membership::{
    AppliedRaftMembership, RaftMembership, RaftMembershipError, RaftMembershipLimits,
    raft_membership_owned_bytes,
};

pub use claim::{PreparedClaimPhase, PreparedClaimQuery};
pub use mutation::{MutationNotSubmitted, MutationSubmission, MutationSubmitPort, MutationUnknown};
pub use value::{
    PreparedWorkflowValueBatch, WorkflowValueDefaults, WorkflowValueItemInput,
    WorkflowValuePolicyWitness, WorkflowValueTargetGuard,
};

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
    /// One public-V1 caller effect, distinct from private aggregate identities.
    PublicMutation {
        /// Single owned effect; no singleton batch allocation is required.
        item: Box<PreparedBatchItem>,
        /// Immutable caller presence, separately verified from generated fields.
        evidence: PublicMutationEvidence,
    },
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

impl PreparedOperation {
    /// Borrows represented mutation effects without changing their profile.
    #[must_use]
    pub fn mutation_items(&self) -> Option<&[PreparedBatchItem]> {
        match self {
            Self::Batch(items) => Some(items),
            Self::PublicMutation { item, .. } => Some(std::slice::from_ref(item.as_ref())),
            _ => None,
        }
    }
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
