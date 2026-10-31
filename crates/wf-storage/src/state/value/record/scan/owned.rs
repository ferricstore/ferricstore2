//! Admitted materialization of the complete already-scanned core record.
use super::super::super::copy;
use super::{ScannedWorkflowRecord, StateValueError};
use wf_domain::lease::{Lease, LeaseToken};
use wf_domain::workflow::restoration::{WorkflowRecordData, restore_record};
use wf_ports::state::StoredWorkflowRecord;
use wf_types::identity::{PartitionId, RecordId, TenantId};
use wf_types::version::{Fence, TimestampMillis};

impl ScannedWorkflowRecord<'_> {
    /// Consumes parsed fields after admitting their complete logical owner size.
    ///
    /// # Errors
    /// Rejects a zero/short owner cap, failed field allocation or typed restoration.
    pub fn into_owned(
        self,
        maximum_owned_bytes: usize,
    ) -> Result<StoredWorkflowRecord, StateValueError> {
        if maximum_owned_bytes == 0 {
            return Err(StateValueError::InvalidValue);
        }
        if self.owned_bytes > maximum_owned_bytes {
            return Err(StateValueError::Limit {
                bytes: self.owned_bytes,
                maximum: maximum_owned_bytes,
            });
        }
        let tenant =
            TenantId::new(copy(self.tenant)?).map_err(|_| StateValueError::InvalidValue)?;
        let partition =
            PartitionId::new(copy(self.partition)?).map_err(|_| StateValueError::InvalidValue)?;
        let lease = self
            .lease
            .map(|(worker, token, deadline)| {
                Ok::<_, StateValueError>(Lease {
                    worker: copy(worker)?,
                    token: LeaseToken::new(copy(token)?)
                        .map_err(|_| StateValueError::InvalidValue)?,
                    deadline: TimestampMillis::new(deadline),
                })
            })
            .transpose()?;
        let record = restore_record(
            WorkflowRecordData {
                id: RecordId::new(copy(self.id)?).map_err(|_| StateValueError::InvalidValue)?,
                workflow_type: copy(self.workflow_type)?,
                state: copy(self.state)?,
                version: self.version,
                fence: Fence::new(self.fence),
                attempts: self.attempts,
                created_at: TimestampMillis::new(self.created),
                updated_at: TimestampMillis::new(self.updated),
                next_run_at: self.next.map(TimestampMillis::new),
                run_state: self.run_state.map(copy).transpose()?,
                lease,
                payload: self.payload.map(copy).transpose()?,
                result: self.result.map(copy).transpose()?,
                error: self.error.map(copy).transpose()?,
                latest_kind: self.latest.0,
                latest_version: self.latest.1,
                latest_fence: Fence::new(self.latest.2),
            },
            self.limits,
        )
        .map_err(|_| StateValueError::InvalidValue)?;
        Ok(StoredWorkflowRecord {
            tenant,
            partition,
            record,
            ordering: self.ordering,
        })
    }
}
