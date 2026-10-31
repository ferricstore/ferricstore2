//! Real original inquiries and writes; faults alter only delivery/admission.
use super::{fixtures::*, *};
use std::sync::Arc;
use wf_domain::{
    governance::credit::CreditOwner,
    workflow::request::{ClientIntent, RequestScope},
};
use wf_ports::{
    consensus::{
        CreditCatalogPublicationPort, CreditRequestLookup, MutationNotSubmitted as N,
        MutationSubmission as M, MutationUnknown as U, PublicCreditSubmitPort,
    },
    log::{PreparedBytes, prepare_catalog_membership_request},
    state::{
        CreditCatalogEntry, CreditCatalogOwnerState, CreditCatalogPage, CreditCatalogReadLimits,
        CreditCatalogReadPort, CreditOwnerReadPort, GovernanceCatalogPage, GovernanceCatalogRead,
        GovernanceCatalogReadLimits, GovernanceCatalogReadPort, GroupApplyStore,
        GroupApplyTransaction, StateTransactionError as S,
    },
};
use wf_types::{identity::RequestId, version::CommittedTime};
impl CreditCatalogReadPort for PublicCreditFixture {
    fn credit_catalog_page(
        &mut self,
        scope: &RequestScope,
        limit: u16,
        limits: CreditCatalogReadLimits,
    ) -> Result<CreditCatalogPage, S> {
        let (_, mut state) = self.open().map_err(|_| S::Unavailable)?;
        state.credit_catalog_page(scope, limit, limits)
    }
    fn credit_catalog_owners(
        &mut self,
        scope: &RequestScope,
        entries: &[CreditCatalogEntry],
        limits: CreditCatalogReadLimits,
    ) -> Result<Vec<CreditCatalogOwnerState>, S> {
        let (_, mut state) = self.open().map_err(|_| S::Unavailable)?;
        state.credit_catalog_owners(scope, entries, limits)
    }
}
impl CreditOwnerReadPort for PublicCreditFixture {
    fn credit_catalog_owner(
        &self,
        scope: &RequestScope,
        name: &[u8],
        limits: CreditCatalogReadLimits,
    ) -> Result<Option<CreditOwner>, S> {
        self.trace.borrow_mut().push(PublicCreditTrace::Owner);
        if self.fault.get() == PublicCreditFault::OwnerRead {
            return Err(S::Unavailable);
        }
        let (_, state) = self.open().map_err(|_| S::Unavailable)?;
        state.credit_catalog_owner(scope, name, limits)
    }
}
impl GovernanceCatalogReadPort for PublicCreditFixture {
    fn governance_catalog_page(
        &self,
        read: GovernanceCatalogRead<'_>,
        limits: GovernanceCatalogReadLimits,
    ) -> Result<GovernanceCatalogPage, S> {
        let (_, state) = self.open().map_err(|_| S::Unavailable)?;
        state.governance_catalog_page(read, limits)
    }
    fn governance_catalog_contains(
        &self,
        scope: &RequestScope,
        catalog: &[u8],
        member: &[u8],
        limits: GovernanceCatalogReadLimits,
    ) -> Result<bool, S> {
        self.trace.borrow_mut().push(PublicCreditTrace::Member);
        let (_, state) = self.open().map_err(|_| S::Unavailable)?;
        state.governance_catalog_contains(scope, catalog, member, limits)
    }
}
impl CreditCatalogPublicationPort for PublicCreditFixture {
    fn prepare_catalog_command(
        &mut self,
        scope: &RequestScope,
    ) -> Result<(RequestId, CommittedTime), N> {
        if scope != &self.scope {
            return Err(N::InvalidScope);
        }
        self.trace.borrow_mut().push(PublicCreditTrace::Maintenance);
        self.next += 1;
        Ok((
            RequestId::new(format!("app-maintenance-{}", self.next).into_bytes())
                .map_err(|_| N::Preparation)?,
            f::clock(self.now, 0, self.now).map_err(|_| N::Preparation)?,
        ))
    }
    fn submit_catalog_command(
        &mut self,
        scope: &RequestScope,
        id: &RequestId,
        frame: PreparedBytes,
        _workspace: usize,
    ) -> M {
        if scope != &self.scope {
            return M::NotSubmitted(N::InvalidScope);
        }
        let fault = self.fault.get();
        if frame.as_bytes().get(6) == Some(&12) {
            let inspected = prepare_catalog_membership_request(
                frame.as_bytes(),
                f::driver_limits().codec,
                16 * f::MIB,
            );
            if inspected.is_err() {
                return M::NotSubmitted(N::Preparation);
            }
            let exists = self.owner().ok().flatten().is_some();
            self.trace
                .borrow_mut()
                .push(PublicCreditTrace::Register(exists));
            if fault == PublicCreditFault::RefuseRegister {
                self.fault.set(PublicCreditFault::None);
                return M::NotSubmitted(N::Unavailable);
            }
        } else if frame.as_bytes().get(6) == Some(&11) {
            self.trace.borrow_mut().push(PublicCreditTrace::Ack);
        } else {
            return M::NotSubmitted(N::Preparation);
        }
        match self.commit(id, &frame) {
            Ok(outcome) => {
                if fault == PublicCreditFault::UnknownRegister {
                    self.fault.set(PublicCreditFault::None);
                    M::Unknown(U::Unavailable)
                } else {
                    M::Retained(outcome)
                }
            }
            Err(e) => {
                self.last_error = Some(e.to_string());
                M::Unknown(U::CommitUncertain)
            }
        }
    }
}
impl PublicCreditSubmitPort for PublicCreditFixture {
    fn lookup_public_credit(
        &mut self,
        scope: &RequestScope,
        id: &RequestId,
        intent: ClientIntent,
        limits: CreditCatalogReadLimits,
    ) -> Result<CreditRequestLookup, N> {
        self.trace.borrow_mut().push(PublicCreditTrace::Lookup);
        if self.fault.get() == PublicCreditFault::LookupUnknown {
            return Ok(CreditRequestLookup::Resolved(M::Unknown(U::Unavailable)));
        }
        if scope != &self.scope {
            return Err(N::InvalidScope);
        }
        let (_, mut state) = self.open().map_err(|_| N::Unavailable)?;
        let tx = state
            .begin_apply(
                scope.group(),
                scope.recovery(),
                f::driver_limits().codec.admission(),
                limits.maximum_steps,
                limits.maximum_bytes,
                limits.maximum_temporary_bytes,
            )
            .map_err(|_| N::Unavailable)?;
        let outcome = tx.outcome(scope.tenant(), id).map_err(|_| N::Unavailable)?;
        let Some(outcome) = outcome else {
            return Ok(CreditRequestLookup::Fresh);
        };
        let result = if outcome.intent() != intent {
            M::IntentConflict
        } else if self.now >= outcome.expires_at().get() {
            M::Expired
        } else {
            M::Retained(Box::new(outcome))
        };
        Ok(CreditRequestLookup::Resolved(result))
    }
    fn prepare_public_credit(
        &mut self,
        scope: &RequestScope,
        _id: &RequestId,
        call: wf_domain::governance::caller::NormalizedCreditCall<'_>,
    ) -> Result<(CommittedTime, Option<[u8; 16]>), N> {
        self.trace.borrow_mut().push(PublicCreditTrace::Prepare);
        if scope != &self.scope {
            return Err(N::InvalidScope);
        }
        if call.now_ms().is_some_and(|n| n > self.now) {
            return Err(N::Preparation);
        }
        let batch = if call.action().kind() == K::Spend {
            self.batch = self.batch.checked_add(1).ok_or(N::Preparation)?;
            self.trace.borrow_mut().push(PublicCreditTrace::Entropy);
            Some([self.batch; 16])
        } else {
            None
        };
        Ok((
            f::clock(self.now, 0, self.now).map_err(|_| N::Preparation)?,
            batch,
        ))
    }
    fn submit_public_credit(
        &mut self,
        scope: &RequestScope,
        id: &RequestId,
        frame: Arc<PreparedBytes>,
        _workspace: usize,
    ) -> M {
        self.trace.borrow_mut().push(PublicCreditTrace::Submit);
        self.frames.push(Arc::clone(&frame));
        if scope != &self.scope {
            return M::NotSubmitted(N::InvalidScope);
        }
        match self.commit(id, &frame) {
            Ok(outcome) => match self.fault.replace(PublicCreditFault::None) {
                PublicCreditFault::LosePublic => M::Unknown(U::Unavailable),
                PublicCreditFault::WrongPublicIds => {
                    corrupt_ids(&outcome).unwrap_or(M::Unknown(U::InvalidReply))
                }
                other => {
                    self.fault.set(other);
                    M::Retained(outcome)
                }
            },
            Err(e) => {
                self.last_error = Some(e.to_string());
                M::Unknown(U::CommitUncertain)
            }
        }
    }
}
