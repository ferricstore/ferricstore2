//! Actual bounded source batches; presence summaries never grant current authority.

use crate::state::RedbStateStore;
use wf_domain::workflow::request::RequestScope;
use wf_ports::state::{
    CreditCatalogEntry, CreditCatalogOwnerState, CreditCatalogPage, CreditCatalogReadLimits,
    CreditCatalogReadPort, StateTransactionError as E,
};

impl CreditCatalogReadPort for RedbStateStore {
    fn credit_catalog_page(
        &mut self,
        _scope: &RequestScope,
        _limit: u16,
        _limits: CreditCatalogReadLimits,
    ) -> Result<CreditCatalogPage, E> {
        Err(E::UnsupportedPolicyProfile)
    }
    fn credit_catalog_owners(
        &mut self,
        _scope: &RequestScope,
        _entries: &[CreditCatalogEntry],
        _limits: CreditCatalogReadLimits,
    ) -> Result<Vec<CreditCatalogOwnerState>, E> {
        Err(E::UnsupportedPolicyProfile)
    }
}
