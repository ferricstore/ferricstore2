//! Explicit GC3 construction; ordinary observers cannot prune legacy members.
use super::{RedbStateStore, StateStoreGenesis, StateStoreLimits};
use wf_ports::{consensus::WorkflowValueDefaults, state::StateTransactionError as E};
use wf_types::identity::{GroupId, RecoveryId};
impl super::RedbStateTransaction<'_> {
    /// Requires actual GC3 under the same coherent guard.
    /// # Errors
    /// Refuses older or corrupt capabilities and admission failure.
    pub fn require_catalog_prune(&self) -> Result<(), E> {
        Err(E::UnsupportedPolicyProfile)
    }
}
impl wf_ports::state::GovernanceCatalogPruneReadPort for RedbStateStore {
    fn governance_catalog_prune_page(
        &self,
        request: wf_ports::state::GovernanceCatalogRead<'_>,
        limits: wf_ports::state::GovernanceCatalogReadLimits,
    ) -> Result<wf_ports::state::GovernanceCatalogPrunePage, E> {
        let _ = (request, limits);
        Err(E::UnsupportedPolicyProfile)
    }
}
impl RedbStateStore {
    /// Creates GC3, optionally combined with actual identified credit2 shards.
    /// # Errors
    /// Refuses unsupported profiles, invalid genesis or unproved durability.
    pub fn create_governance_prune_database(
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
