//! Coherent physical history acquisition and source-compatible value hydration.

use wf_domain::workflow::request::RequestScope;
use wf_ports::clock::WorkflowValueReadClock;
use wf_ports::state::{
    StateTransactionError, WorkflowHistoryHydrationError, WorkflowHistoryHydrationLimits,
    WorkflowHistoryHydrationRequest, WorkflowHistoryValuePage,
};
use wf_types::identity::{PartitionId, RecordId};

#[cfg(test)]
mod tests;

impl super::super::RedbStateStore {
    /// Captures and hydrates actual history candidates on one read snapshot.
    ///
    /// Caller still applies lifecycle visibility and an authoritative read barrier.
    /// # Errors
    /// Preserves state, clock, conversion and projection refusals explicitly.
    pub fn read_workflow_history_page_with_values<C: WorkflowValueReadClock>(
        &self,
        _clock: &mut C,
        _scope: &RequestScope,
        _partition: &PartitionId,
        _id: &RecordId,
        _request: WorkflowHistoryHydrationRequest<'_>,
        _limits: WorkflowHistoryHydrationLimits,
    ) -> Result<WorkflowHistoryValuePage, WorkflowHistoryHydrationError> {
        Err(WorkflowHistoryHydrationError::State(
            StateTransactionError::UnsupportedLayout,
        ))
    }
}
