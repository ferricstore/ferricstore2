//! Actual bounded source batches; presence summaries never grant current authority.

use crate::state::RedbStateStore;
use crate::state::RedbStateTransaction;
use wf_domain::workflow::request::RequestScope;
use wf_ports::state::{
    CreditCatalogEntry, CreditCatalogOwnerState, CreditCatalogPage, CreditCatalogReadLimits,
    CreditCatalogReadPort, StateTransactionError as E,
};

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
                let state = if owner.is_some() {
                    CreditCatalogOwnerState::Present
                } else {
                    CreditCatalogOwnerState::Missing
                };
                drop(owner);
                states.push(state);
            }
            Ok(states)
        })
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
