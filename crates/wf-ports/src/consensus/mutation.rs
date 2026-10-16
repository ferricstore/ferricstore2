//! Effect capability for individually scoped prepared public mutations.

use wf_domain::workflow::caller::NormalizedMutation;
use wf_domain::workflow::request::{RequestScope, RetainedOutcome};
use wf_types::identity::RequestId;
use wf_types::version::CommittedTime;

use crate::log::PreparedBytes;

/// Proof supplied by the execution owner that THIS attempt was never proposed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MutationNotSubmitted {
    /// The original namespace is no longer admissible.
    InvalidScope,
    /// Current caller authorization refused.
    Unauthorized,
    /// No proposal was made because execution was unavailable.
    Unavailable,
    /// Current routing requires another execution owner.
    Redirect,
    /// Prospective resource admission refused before proposal.
    Limit,
    /// Local encoding or stamp preparation refused before proposal.
    Preparation,
    /// A prior slot may still apply, so this slot was never prepared/proposed.
    PredecessorUnknown,
}

/// The caller cannot establish whether the submitted attempt applied.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MutationUnknown {
    /// Execution disconnected without no-proposal proof.
    Unavailable,
    /// Durable completion is uncertain.
    CommitUncertain,
    /// Returned retained authority could not be validated.
    InvalidReply,
}

/// One submission result; no original operation/current-state borrow escapes.
#[derive(Debug, Eq, PartialEq)]
pub enum MutationSubmission {
    /// Exact original retained response, including business refusal.
    Retained(Box<RetainedOutcome>),
    /// The identity already names another caller intent.
    IntentConflict,
    /// The original retained identity expired and cannot be reexecuted.
    Expired,
    /// This attempt never reached proposal.
    NotSubmitted(MutationNotSubmitted),
    /// Bytes may still apply; later dependent submissions must stop.
    Unknown(MutationUnknown),
}

/// Current authorization/scope and supervised admission belong to this owner.
///
/// Neither constructing this capability nor supplying mathematical caller data
/// grants authority. Recheck scope/authorization at both stages, including waits.
/// After handoff, retain bytes/permits through actual completion even if a caller
/// cancels or receives Unknown. A physical abort alone is not no-proposal proof.
pub trait MutationSubmitPort {
    /// Supplies a trusted admitted stamp without canonical record planning reads.
    /// # Errors
    /// Returns a definitive preparation refusal before this attempt is proposed.
    fn prepare(
        &mut self,
        scope: &RequestScope,
        request: &RequestId,
        caller: &NormalizedMutation<'_>,
    ) -> Result<CommittedTime, MutationNotSubmitted>;

    /// Consumes exact prepared bytes under the declared execution owner budget.
    /// Log replay alone must be resolved to retained authority or reported Unknown.
    fn submit(
        &mut self,
        scope: &RequestScope,
        request: &RequestId,
        prepared: PreparedBytes,
        workspace_limit: usize,
    ) -> MutationSubmission;
}
