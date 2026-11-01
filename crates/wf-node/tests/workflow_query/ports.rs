//! Controlled lifecycle provider; this fixture does not prove a real quorum.
use super::*;
use wf_domain::workflow::{control::ControlState, request::RequestScope};
use wf_ports::state::{
    StrongWorkflowRecordRead, StrongWorkflowRecordReadError as R, StrongWorkflowRecordReadPort,
    WorkflowRecordPolicyReadLimits, WorkflowRecordPolicyReadPort, WorkflowRecordReadQuery,
};
use wf_types::identity::PartitionId;
impl StrongWorkflowRecordReadPort for QueryFixture {
    fn read_strong_workflow_record(
        &mut self,
        scope: &RequestScope,
        partition: &PartitionId,
        id: &RecordId,
        _after: Option<WorkflowReadPosition<'_>>,
        limits: WorkflowRecordPolicyReadLimits,
    ) -> Result<StrongWorkflowRecordRead, R> {
        self.reads = self.reads.checked_add(1).ok_or(R::Limit)?;
        if matches!(self.fault, QueryFault::Deny) {
            return Err(R::Unauthorized);
        }
        if matches!(self.fault, QueryFault::Unavailable) {
            return Err(R::Unavailable);
        }
        let mut snapshot = self
            .store
            .read_workflow_record_with_policy(
                scope,
                partition,
                id,
                WorkflowRecordReadQuery::default(),
                limits,
            )
            .map_err(R::State)?;
        let mut barrier = snapshot
            .record
            .control
            .last_entry()
            .map(wf_domain::workflow::control::EntryIdentity::log);
        match self.fault {
            QueryFault::BarrierAhead => {
                barrier = Some(f::at(
                    snapshot
                        .record
                        .control
                        .position()
                        .get()
                        .checked_add(1)
                        .ok_or(R::Limit)?,
                ));
            }
            QueryFault::WrongIdentity => {
                barrier = Some(wf_types::version::LogIdentity::new(
                    99,
                    99,
                    snapshot.record.control.position(),
                ));
            }
            QueryFault::WrongPoint => {
                if let Some(capture) = &mut snapshot.record.capture {
                    capture.record.partition =
                        PartitionId::new(b"q".to_vec()).map_err(|_| R::InvalidScope)?;
                }
            }
            QueryFault::EmptyControl => {
                snapshot.record.control =
                    ControlState::initial(f::clock(0, 0, 0).map_err(|_| R::Unavailable)?);
                barrier = None;
            }
            _ => {}
        }
        Ok(StrongWorkflowRecordRead {
            snapshot,
            barrier,
            now: self.now,
        })
    }
}
