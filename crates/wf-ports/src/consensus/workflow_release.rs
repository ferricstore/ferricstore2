//! Configured routing and supervised release-maintenance effects.
use super::{MutationNotSubmitted, MutationSubmission};
use crate::{
    log::PreparedBytes,
    state::{GovernanceCatalogReadLimits, WorkflowReleaseReadPort},
};
use wf_domain::workflow::request::RequestScope;
use wf_types::{identity::RequestId, version::CommittedTime};

/// Configured credit namespaces; member bytes alone cannot supply routing authority.
pub struct WorkflowReleaseCreditRoute {
    /// Actual resolved original owner namespace.
    pub source: RequestScope,
    /// Actual configured owner-catalog namespace.
    pub target: RequestScope,
    /// Exact catalog identity used by the public caller.
    pub catalog: Vec<u8>,
    /// Actual immutable owner shard count.
    pub shard_count: u64,
}
/// Current routing/authorization and supervised command custody belong to this owner.
pub trait WorkflowReleaseSubmitPort: WorkflowReleaseReadPort {
    /// Resolves only configured credit routing for the exact original scope.
    /// # Errors
    /// Refuses unknown source/routing, authorization or bounded ownership admission.
    fn workflow_release_credit_route(
        &mut self,
        outbox: &RequestScope,
        credit_scope: &[u8],
        limits: GovernanceCatalogReadLimits,
    ) -> Result<WorkflowReleaseCreditRoute, MutationNotSubmitted>;
    /// Supplies a fresh identity and trusted stamp at or after the explicit floor.
    /// # Errors
    /// Refuses before proposal; returned bytes grant no submission authority.
    fn prepare_workflow_release_command(
        &mut self,
        scope: &RequestScope,
        not_before_ms: u64,
    ) -> Result<(RequestId, CommittedTime), MutationNotSubmitted>;
    /// Retains original bytes/admission through supervised completion, including
    /// cancellation/Unknown. Exact replay must resolve to retained authority.
    fn submit_workflow_release_command(
        &mut self,
        scope: &RequestScope,
        id: &RequestId,
        frame: PreparedBytes,
        workspace_bytes: usize,
    ) -> MutationSubmission;
}
