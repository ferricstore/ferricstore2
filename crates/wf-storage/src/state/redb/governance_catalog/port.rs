//! Actual bounded coherent catalog capabilities; no unlogged writer.
use super::RedbStateStore;
use wf_domain::workflow::request::RequestScope;
use wf_ports::state::{
    GovernanceCatalogPage, GovernanceCatalogRead, GovernanceCatalogReadLimits,
    GovernanceCatalogReadPort, StateTransactionError as E,
};
impl GovernanceCatalogReadPort for RedbStateStore {
    fn governance_catalog_page(
        &self,
        request: GovernanceCatalogRead<'_>,
        limits: GovernanceCatalogReadLimits,
    ) -> Result<GovernanceCatalogPage, E> {
        let _ = (request, limits);
        Err(E::UnsupportedPolicyProfile)
    }
    fn governance_catalog_contains(
        &self,
        scope: &RequestScope,
        catalog: &[u8],
        member: &[u8],
        limits: GovernanceCatalogReadLimits,
    ) -> Result<bool, E> {
        let _ = (scope, catalog, member, limits);
        Err(E::UnsupportedPolicyProfile)
    }
}
