//! Bounded physical history capture on one coherent backend snapshot.

use wf_domain::workflow::request::RequestScope;
use wf_ports::state::{
    StateTransactionError, WorkflowHistoryPage, WorkflowHistoryQuery, WorkflowHistoryReadLimits,
};
use wf_types::identity::{PartitionId, RecordId};

mod page;
mod read;
mod row;

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
        scope: &RequestScope,
        partition: &PartitionId,
        id: &RecordId,
        query: WorkflowHistoryQuery<'_>,
        limits: WorkflowHistoryReadLimits,
    ) -> Result<WorkflowHistoryPage, StateTransactionError> {
        let mut context =
            read::open(self, scope, query, limits).map_err(|error| self.failed(error))?;
        let rows = context.open_rows().map_err(|error| self.failed(error))?;
        let result = page::capture(&mut context, &rows, scope, partition, id, query);
        read::close(context, rows);
        result.map_err(|error| self.failed(error))
    }
}
