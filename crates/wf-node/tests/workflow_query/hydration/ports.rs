//! Controlled authority faults around real native sized reads and a real clock.
use super::*;
use wf_domain::workflow::request::RequestScope;
use wf_ports::{
    blob::{
        AuthorizedWorkflowValueReadPort, WorkflowValueReadFailure, WorkflowValueReadLimits,
        WorkflowValueSizedReadBatch,
    },
    state::{
        StrongWorkflowRecordRead, StrongWorkflowRecordReadError as E, StrongWorkflowRecordReadPort,
        WorkflowReadPosition, WorkflowRecordPolicyReadLimits,
    },
};
use wf_types::identity::PartitionId;
impl StrongWorkflowRecordReadPort for HydrationFixture {
    fn read_strong_workflow_record(
        &mut self,
        scope: &RequestScope,
        partition: &PartitionId,
        id: &RecordId,
        after: Option<WorkflowReadPosition<'_>>,
        limits: WorkflowRecordPolicyReadLimits,
    ) -> Result<StrongWorkflowRecordRead, E> {
        let result = self
            .base
            .read_strong_workflow_record(scope, partition, id, after, limits)?;
        if matches!(self.fault, HydrationFault::RevokeAfterRecord) {
            self.fault = HydrationFault::Deny;
        }
        Ok(result)
    }
}
impl AuthorizedWorkflowValueReadPort for HydrationFixture {
    fn read_authorized_workflow_values(
        &mut self,
        scope: &RequestScope,
        references: &[&[u8]],
        threshold: usize,
        limits: WorkflowValueReadLimits,
    ) -> Result<WorkflowValueSizedReadBatch, E> {
        self.value_reads = self.value_reads.checked_add(1).ok_or(E::Limit)?;
        if matches!(self.fault, HydrationFault::Deny) {
            return Err(E::Unauthorized);
        }
        if matches!(self.fault, HydrationFault::Unavailable) {
            return Err(E::Unavailable);
        }
        let mut batch = self
            .base
            .store
            .read_workflow_value_batch_with_sizes(
                &mut HydrationClock {
                    now: self.value_now.unwrap_or(self.base.now),
                },
                scope,
                references,
                threshold,
                limits,
            )
            .map_err(E::State)?;
        match self.fault {
            HydrationFault::ReadFailure => {
                batch.values.clear();
                batch.slots.fill(Err(WorkflowValueReadFailure::ReadFailed));
            }
            HydrationFault::Missing => {
                batch.values.clear();
                batch.slots.fill(Ok(None));
            }
            HydrationFault::UnsafeDrift => {
                batch.values.clear();
                batch.slots.fill(Err(WorkflowValueReadFailure::UnsafeDrift));
            }
            HydrationFault::Mismatch => batch.slots.clear(),
            _ => {}
        }
        Ok(batch)
    }
}
