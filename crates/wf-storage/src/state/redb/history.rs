//! Bounded physical history capture on one coherent backend snapshot.

use wf_domain::workflow::request::RequestScope;
use wf_ports::state::{
    StateTransactionError, WorkflowHistoryPage, WorkflowHistoryQuery, WorkflowHistoryReadLimits,
};
use wf_types::identity::{PartitionId, RecordId};

#[cfg(test)]
mod tests;

impl super::RedbStateStore {
    /// Captures selected physical history and the complete next candidate.
    ///
    /// Caller applies lifecycle visibility and an authoritative read barrier.
    /// # Errors
    /// Refuses query, scope, control, resource, stored identity or backend failures.
    pub fn read_workflow_history_page(
        &self,
        _scope: &RequestScope,
        _partition: &PartitionId,
        _id: &RecordId,
        _query: WorkflowHistoryQuery<'_>,
        _limits: WorkflowHistoryReadLimits,
    ) -> Result<WorkflowHistoryPage, StateTransactionError> {
        Err(StateTransactionError::UnsupportedLayout)
    }
}
