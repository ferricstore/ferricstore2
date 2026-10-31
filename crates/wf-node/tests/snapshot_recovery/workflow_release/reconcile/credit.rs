//! Existing public-credit lifecycle backed by actual local committed Node state.
use super::{fixtures::*, *};
use std::sync::Arc;
use wf_domain::{
    governance::{
        caller::{CreditCallAction, NormalizedCreditCall},
        credit::CreditOwner,
    },
    workflow::request::{ClientIntent, RequestScope},
};
use wf_ports::{
    consensus::{
        CreditCatalogPublicationPort, CreditRequestLookup, MutationNotSubmitted as N,
        MutationSubmission as M, MutationUnknown as U, PublicCreditSubmitPort,
    },
    log::PreparedBytes,
    state::{
        CreditCatalogEntry, CreditCatalogOwnerState, CreditCatalogPage, CreditCatalogReadLimits,
        CreditCatalogReadPort, CreditOwnerReadPort, GovernanceCatalogPage, GovernanceCatalogRead,
        GovernanceCatalogReadLimits, GovernanceCatalogReadPort, GroupApplyStore,
        GroupApplyTransaction, StateTransactionError as S,
    },
};
use wf_types::{identity::RequestId, version::CommittedTime};
impl CreditCatalogReadPort for Fixture {
    fn credit_catalog_page(
        &mut self,
        s: &RequestScope,
        n: u16,
        l: CreditCatalogReadLimits,
    ) -> Result<CreditCatalogPage, S> {
        let (_, mut state) = self.open(s).map_err(|_| S::Unavailable)?;
        state.credit_catalog_page(s, n, l)
    }
    fn credit_catalog_owners(
        &mut self,
        s: &RequestScope,
        e: &[CreditCatalogEntry],
        l: CreditCatalogReadLimits,
    ) -> Result<Vec<CreditCatalogOwnerState>, S> {
        let (_, mut state) = self.open(s).map_err(|_| S::Unavailable)?;
        state.credit_catalog_owners(s, e, l)
    }
}
impl CreditOwnerReadPort for Fixture {
    fn credit_catalog_owner(
        &self,
        s: &RequestScope,
        name: &[u8],
        l: CreditCatalogReadLimits,
    ) -> Result<Option<CreditOwner>, S> {
        let (_, state) = self.open(s).map_err(|_| S::Unavailable)?;
        state.credit_catalog_owner(s, name, l)
    }
}
impl GovernanceCatalogReadPort for Fixture {
    fn governance_catalog_page(
        &self,
        r: GovernanceCatalogRead<'_>,
        l: GovernanceCatalogReadLimits,
    ) -> Result<GovernanceCatalogPage, S> {
        let (_, state) = self.open(r.scope).map_err(|_| S::Unavailable)?;
        state.governance_catalog_page(r, l)
    }
    fn governance_catalog_contains(
        &self,
        s: &RequestScope,
        c: &[u8],
        m: &[u8],
        l: GovernanceCatalogReadLimits,
    ) -> Result<bool, S> {
        let (_, state) = self.open(s).map_err(|_| S::Unavailable)?;
        state.governance_catalog_contains(s, c, m, l)
    }
}
impl CreditCatalogPublicationPort for Fixture {
    fn prepare_catalog_command(
        &mut self,
        s: &RequestScope,
    ) -> Result<(RequestId, CommittedTime), N> {
        self.identity(s)
    }
    fn submit_catalog_command(
        &mut self,
        s: &RequestScope,
        id: &RequestId,
        f: PreparedBytes,
        _w: usize,
    ) -> M {
        self.submit(s, id, &f)
    }
}
impl PublicCreditSubmitPort for Fixture {
    fn lookup_public_credit(
        &mut self,
        s: &RequestScope,
        id: &RequestId,
        intent: ClientIntent,
        l: CreditCatalogReadLimits,
    ) -> Result<CreditRequestLookup, N> {
        let (_, mut state) = self.open(s).map_err(|_| N::InvalidScope)?;
        let tx = state
            .begin_apply(
                s.group(),
                s.recovery(),
                f::driver_limits().codec.admission(),
                l.maximum_steps,
                l.maximum_bytes,
                l.maximum_temporary_bytes,
            )
            .map_err(|_| N::Unavailable)?;
        let original = tx.outcome(s.tenant(), id).map_err(|_| N::Unavailable)?;
        Ok(match original {
            None => CreditRequestLookup::Fresh,
            Some(o) => CreditRequestLookup::Resolved(if o.intent() != intent {
                M::IntentConflict
            } else if self.now >= o.expires_at().get() {
                M::Expired
            } else {
                M::Retained(Box::new(o))
            }),
        })
    }
    fn prepare_public_credit(
        &mut self,
        s: &RequestScope,
        _id: &RequestId,
        call: NormalizedCreditCall<'_>,
    ) -> Result<(CommittedTime, Option<[u8; 16]>), N> {
        self.index(s).map_err(|_| N::InvalidScope)?;
        if call.now_ms().is_some_and(|n| n > self.now) {
            return Err(N::Preparation);
        }
        if let CreditCallAction::Release {
            reservation_ids, ..
        } = call.action()
        {
            self.trace.push(ReleaseReconcileTrace::Credit(
                reservation_ids.iter().map(<[u8]>::to_vec).collect(),
            ));
        }
        Ok((
            f::clock(self.now, 0, self.now).map_err(|_| N::Preparation)?,
            None,
        ))
    }
    fn submit_public_credit(
        &mut self,
        s: &RequestScope,
        id: &RequestId,
        f: Arc<PreparedBytes>,
        _w: usize,
    ) -> M {
        if self.fault == ReleaseReconcileFault::PublicRefuse {
            self.fault = ReleaseReconcileFault::None;
            return M::NotSubmitted(N::Unavailable);
        }
        self.frames.push(Arc::clone(&f));
        let result = self.submit(s, id, &f);
        if self.fault == ReleaseReconcileFault::PublicUnknown {
            self.fault = ReleaseReconcileFault::None;
            M::Unknown(U::Unavailable)
        } else {
            result
        }
    }
}
