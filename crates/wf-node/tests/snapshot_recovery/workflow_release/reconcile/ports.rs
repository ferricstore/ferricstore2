//! Serial local observations and real native maintenance, with delivery faults.
use super::{fixtures::*, *};
use wf_domain::workflow::request::RequestScope;
use wf_ports::{
    consensus::{
        MutationNotSubmitted as N, MutationSubmission as M, MutationUnknown as U,
        WorkflowReleaseCreditRoute, WorkflowReleaseSubmitPort,
    },
    log::{PreparedBytes, prepare_release_maintenance_request},
    state::{
        GovernanceCatalogReadLimits as L, StateTransactionError as S, WorkflowReleasePage,
        WorkflowReleaseReadPort, WorkflowReleaseRecord as R,
    },
};
use wf_types::{identity::RequestId, version::CommittedTime};
impl WorkflowReleaseReadPort for Fixture {
    fn workflow_release_page(
        &mut self,
        s: &RequestScope,
        start: Option<u64>,
        n: u16,
        l: L,
    ) -> Result<WorkflowReleasePage, S> {
        let index = self.index(s).map_err(|_| S::NamespaceConflict)?;
        self.trace.push(ReleaseReconcileTrace::Page(index, start));
        if self.fault == ReleaseReconcileFault::PageUnavailable(index) {
            return Err(S::Unavailable);
        }
        let (_, mut state) = self.open(s).map_err(|_| S::Unavailable)?;
        state.workflow_release_page(s, start, n, l)
    }
    fn workflow_release_records_for_release(
        &mut self,
        s: &RequestScope,
        p: Option<&[u8]>,
        ids: &[&[u8]],
        l: L,
    ) -> Result<Vec<R>, S> {
        self.trace.push(ReleaseReconcileTrace::Records(
            p.map(<[u8]>::to_vec),
            ids.len(),
        ));
        if self.fault == ReleaseReconcileFault::RecordRead {
            return Err(S::Unavailable);
        }
        if self.fault == ReleaseReconcileFault::RecordCardinality {
            return Ok(vec![]);
        }
        if self.fault == ReleaseReconcileFault::RecordCorrupt {
            return Ok(ids.iter().map(|_| R::Corrupt).collect());
        }
        // This fixture exclusively owns its serial local committed Node namespace.
        // It supplies no distributed leader/read-barrier implementation.
        let (_, state) = self.open(s).map_err(|_| S::Unavailable)?;
        state.read_workflow_release_records(s, p, ids, l)
    }
}
impl WorkflowReleaseSubmitPort for Fixture {
    fn workflow_release_credit_route(
        &mut self,
        s: &RequestScope,
        _credit: &[u8],
        _l: L,
    ) -> Result<WorkflowReleaseCreditRoute, N> {
        self.index(s).map_err(|_| N::InvalidScope)?;
        if self.fault == ReleaseReconcileFault::Route {
            return Err(N::Redirect);
        }
        Ok(WorkflowReleaseCreditRoute {
            source: s.clone(),
            target: s.clone(),
            catalog: b"limit".to_vec(),
            shard_count: 4,
        })
    }
    fn prepare_workflow_release_command(
        &mut self,
        s: &RequestScope,
        floor: u64,
    ) -> Result<(RequestId, CommittedTime), N> {
        let mut result = self.identity(s)?;
        if self.fault == ReleaseReconcileFault::OldStamp {
            result.1 = f::clock(0, 0, 0).map_err(|_| N::Preparation)?;
        }
        if floor > self.now {
            return Err(N::Preparation);
        }
        Ok(result)
    }
    fn submit_workflow_release_command(
        &mut self,
        s: &RequestScope,
        id: &RequestId,
        frame: PreparedBytes,
        _w: usize,
    ) -> M {
        use wf_domain::governance::release::ReleaseMaintenanceRequest as C;
        let Ok(command) = prepare_release_maintenance_request(
            frame.as_bytes(),
            f::driver_limits().codec,
            16 * f::MIB,
        ) else {
            return M::NotSubmitted(N::Preparation);
        };
        let fault = self.fault;
        let unknown = match command.command() {
            C::MarkCompleted { sequences } => {
                self.trace
                    .push(ReleaseReconcileTrace::Mark(sequences.clone()));
                fault == ReleaseReconcileFault::MarkUnknown
            }
            C::Acknowledge {
                expected_head,
                up_to,
            } => {
                self.trace
                    .push(ReleaseReconcileTrace::Ack(*expected_head, *up_to));
                fault == ReleaseReconcileFault::AckUnknown
            }
        };
        drop(command);
        let result = self.submit(s, id, &frame);
        if unknown {
            self.fault = ReleaseReconcileFault::None;
            M::Unknown(U::Unavailable)
        } else if fault == ReleaseReconcileFault::WrongMaintenanceReply {
            self.original(s, b"claim")
                .map_or(M::Unknown(U::InvalidReply), M::Retained)
        } else {
            result
        }
    }
}
