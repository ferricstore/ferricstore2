//! Explicit derived progress capability; no successful placeholder behavior.
use super::{RedbStateStore, RedbStateTransaction, StateStoreGenesis, StateStoreLimits};
use wf_domain::workflow::request::RequestScope;
use wf_ports::{
    consensus::WorkflowValueDefaults,
    state::{
        GovernanceCatalogProgress, GovernanceCatalogProgressPort, GovernanceCatalogReadLimits,
        StateTransactionError as E,
    },
};
use wf_types::identity::{GroupId, RecoveryId};

impl RedbStateStore {
    /// Creates explicit GC2, optionally combined with identified credit2 owners.
    /// # Errors
    /// Refuses invalid profiles, resources and unproved durability.
    pub fn create_governance_progress_database(
        database: redb::Database,
        group: GroupId,
        recovery: RecoveryId,
        genesis: StateStoreGenesis<'_>,
        defaults: WorkflowValueDefaults,
        credit_shard_count: Option<u64>,
        limits: StateStoreLimits,
    ) -> Result<Self, E> {
        let _ = (
            database,
            group,
            recovery,
            genesis,
            defaults,
            credit_shard_count,
            limits,
        );
        Err(E::UnsupportedPolicyProfile)
    }
}
impl RedbStateTransaction<'_> {
    /// Requires actual immutable GC2 under this same coherent guard.
    /// # Errors
    /// Refuses namespace/profile/resources/corrupt authority.
    pub fn require_catalog_progress(&self) -> Result<(), E> {
        Err(E::UnsupportedPolicyProfile)
    }
    /// Reads actual immutable governance capability0/1/2, without enabling it.
    /// # Errors
    /// Refuses unsupported layouts, corruption or resource exhaustion.
    pub fn governance_catalog_profile(&self) -> Result<u16, E> {
        Err(E::UnsupportedPolicyProfile)
    }
}
impl GovernanceCatalogProgressPort for RedbStateStore {
    fn governance_catalog_progress(
        &self,
        scope: &RequestScope,
        catalog: &[u8],
        limits: GovernanceCatalogReadLimits,
    ) -> Result<GovernanceCatalogProgress, E> {
        let _ = (scope, catalog, limits);
        Err(E::UnsupportedPolicyProfile)
    }
}
