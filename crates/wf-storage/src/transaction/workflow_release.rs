//! Explicit GC4 successor of the stored-value workflow mutation driver.
use super::{
    RecordBatchTransactionOutcome, WorkflowValueTransactionError, WorkflowValueTransactionLimits,
};
use crate::state::RedbStateStore;
use wf_domain::workflow::request::RequestScope;
use wf_types::version::LogIdentity;

/// Applies workflow final writes together with their original release intents.
/// # Errors
/// Refuses unsupported capabilities, authority, resources or unproved commit.
pub fn apply_workflow_release_transaction(
    _store: &mut RedbStateStore,
    _scope: &RequestScope,
    _encoded: &[u8],
    _log: LogIdentity,
    _limits: WorkflowValueTransactionLimits,
) -> Result<RecordBatchTransactionOutcome, WorkflowValueTransactionError> {
    Err(WorkflowValueTransactionError::UnsupportedProfile)
}
