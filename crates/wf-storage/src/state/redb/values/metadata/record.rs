//! One native committed revision for represented core and original metadata.

use wf_domain::workflow::request::RequestScope;
use wf_ports::state::{
    StateTransactionError, WorkflowRecordReadLimits, WorkflowRecordReadPort,
    WorkflowRecordReadQuery, WorkflowRecordReadSnapshot,
};
use wf_types::identity::{PartitionId, RecordId};

mod read;
mod row;

#[cfg(test)]
mod tests;

impl WorkflowRecordReadPort for super::super::RedbStateStore {
    fn read_workflow_record(
        &self,
        scope: &RequestScope,
        partition: &PartitionId,
        id: &RecordId,
        query: WorkflowRecordReadQuery<'_>,
        limits: WorkflowRecordReadLimits,
    ) -> Result<WorkflowRecordReadSnapshot, StateTransactionError> {
        read::observe(self, scope, partition, id, query, limits).map_err(|error| self.failed(error))
    }
}
