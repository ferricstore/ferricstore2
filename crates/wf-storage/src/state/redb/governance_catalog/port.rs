//! Actual bounded coherent catalog capabilities; no unlogged writer.
use super::{RedbStateStore, RedbStateTransaction};
use wf_domain::workflow::request::RequestScope;
use wf_ports::state::{
    GovernanceCatalogPage, GovernanceCatalogRead, GovernanceCatalogReadLimits,
    GovernanceCatalogReadPort, GroupApplyTransaction, StateTransactionError as E,
};

impl GovernanceCatalogReadPort for RedbStateStore {
    fn governance_catalog_page(
        &self,
        request: GovernanceCatalogRead<'_>,
        limits: GovernanceCatalogReadLimits,
    ) -> Result<GovernanceCatalogPage, E> {
        let tx = guard(self, limits)?;
        tx.checked(|| super::read::page(&tx, request))
    }
    fn governance_catalog_contains(
        &self,
        scope: &RequestScope,
        catalog: &[u8],
        member: &[u8],
        limits: GovernanceCatalogReadLimits,
    ) -> Result<bool, E> {
        let tx = guard(self, limits)?;
        tx.checked(|| {
            let revisions = super::require(&tx, scope)?;
            let control = if revisions { Some(tx.control()?) } else { None };
            let key = super::read::key(&tx, scope, catalog, Some(member))?;
            tx.step()?;
            let table = tx
                .write
                .open_table(tx.rows_table)
                .map_err(super::super::admission::table_error)?;
            super::read::contains(&tx, &table, &key, control)
        })
    }
}
pub(super) fn guard(
    store: &RedbStateStore,
    limits: GovernanceCatalogReadLimits,
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
