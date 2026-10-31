//! Actual bounded source batches; presence summaries never grant current authority.

use crate::state::RedbStateStore;
use crate::state::RedbStateTransaction;
use wf_domain::workflow::request::RequestScope;
use wf_ports::state::{
    CreditCatalogEntry, CreditCatalogOwnerState, CreditCatalogPage, CreditCatalogReadLimits,
    CreditCatalogReadPort, StateTransactionError as E,
};

impl wf_ports::state::CreditOwnerReadPort for RedbStateStore {
    fn credit_catalog_owner(
        &self,
        scope: &RequestScope,
        credit_scope: &[u8],
        limits: CreditCatalogReadLimits,
    ) -> Result<Option<wf_domain::governance::credit::CreditOwner>, E> {
        let _ = (scope, credit_scope, limits);
        Err(E::UnsupportedPolicyProfile)
    }
}

impl CreditCatalogReadPort for RedbStateStore {
    fn credit_catalog_page(
        &mut self,
        scope: &RequestScope,
        limit: u16,
        limits: CreditCatalogReadLimits,
    ) -> Result<CreditCatalogPage, E> {
        let tx = guard(self, limits)?;
        tx.checked(|| super::page(&tx, scope, limit))
    }
    fn credit_catalog_owners(
        &mut self,
        scope: &RequestScope,
        entries: &[CreditCatalogEntry],
        limits: CreditCatalogReadLimits,
    ) -> Result<Vec<CreditCatalogOwnerState>, E> {
        if scope.group() != self.group() || scope.recovery() != self.recovery() {
            return Err(E::NamespaceConflict);
        }
        if entries.len() > 256
            || entries.iter().any(|e| {
                e.credit_scope.is_empty()
                    || e.credit_scope.len() > 65535
                    || !(1..=wf_domain::workflow::MAX_EXACT_MILLIS).contains(&e.sequence)
            })
        {
            return Err(E::InvalidChanges);
        }
        let tx = guard(self, limits)?;
        tx.checked(|| {
            if tx.require_identified_credit_profile()?.0 != 2 {
                return Err(E::UnsupportedPolicyProfile);
            }
            tx.admit_read(
                entries
                    .len()
                    .checked_mul(std::mem::size_of::<CreditCatalogOwnerState>())
                    .and_then(|n| n.checked_add(128))
                    .ok_or(E::Limit)?,
            )?;
            let mut states = Vec::new();
            states
                .try_reserve_exact(entries.len())
                .map_err(|_| E::Allocation)?;
            for entry in entries {
                let owner =
                    super::super::read::owner(&tx, scope, &entry.credit_scope, limits.credit)?;
                let state = presence(owner);
                states.push(state);
            }
            Ok(states)
        })
    }
}

fn presence(owner: Option<wf_domain::governance::credit::CreditOwner>) -> CreditCatalogOwnerState {
    let exists = owner.is_some();
    drop(owner);
    if exists {
        CreditCatalogOwnerState::Present
    } else {
        CreditCatalogOwnerState::Missing
    }
}

fn guard(
    store: &RedbStateStore,
    limits: CreditCatalogReadLimits,
) -> Result<RedbStateTransaction<'_>, E> {
    if [
        limits.maximum_steps,
        limits.maximum_bytes,
        limits.maximum_temporary_bytes,
    ]
    .contains(&0)
    {
        return Err(E::InvalidLimits);
    }
    store.ready()?;
    store.transaction(
        false,
        store.limits.admission(),
        limits.maximum_steps,
        limits.maximum_bytes,
        limits.maximum_temporary_bytes,
    )
}
