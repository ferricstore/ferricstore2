//! Authorized immutable value acquisition after an exact record observation.
use super::{WorkflowValueReadLimits, WorkflowValueSizedReadBatch};
use crate::state::StrongWorkflowRecordReadError;
use wf_domain::workflow::request::RequestScope;

/// Current tenant/placement authorization and one coherent batch expiry clock.
/// Exact versioned refs are data, not read authority. Reauthorize after any earlier
/// record read or wait. Honor complete stored-size omission and every read limit;
/// unsafe drift/profile/read failures must not become known missing values.
/// This separate batch does not claim a shared record/value physical revision.
pub trait AuthorizedWorkflowValueReadPort {
    /// Read exact immutable refs under this authenticated request lifecycle.
    /// # Errors
    /// Preserve current authorization, scope, admission, clock and storage refusals.
    fn read_authorized_workflow_values(
        &mut self,
        scope: &RequestScope,
        references: &[&[u8]],
        omission_threshold: usize,
        limits: WorkflowValueReadLimits,
    ) -> Result<WorkflowValueSizedReadBatch, StrongWorkflowRecordReadError>;
}
