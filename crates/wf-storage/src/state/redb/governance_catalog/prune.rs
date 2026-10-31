//! Explicit GC3 construction; ordinary observers cannot prune legacy members.
mod read;
mod write;
use super::{RedbStateStore, StateStoreGenesis, StateStoreLimits};
pub(crate) use read::decide;
use wf_ports::{consensus::WorkflowValueDefaults, state::StateTransactionError as E};
use wf_types::identity::{GroupId, RecoveryId};
pub(crate) use write::apply as commit;
impl super::RedbStateTransaction<'_> {
    /// Requires actual GC3 under the same coherent guard.
    /// # Errors
    /// Refuses older or corrupt capabilities and admission failure.
    pub fn require_catalog_prune(&self) -> Result<(), E> {
        if self.governance_catalog_profile()? != 3 {
            return Err(E::UnsupportedPolicyProfile);
        }
        Ok(())
    }
}
impl wf_ports::state::GovernanceCatalogPruneReadPort for RedbStateStore {
    fn governance_catalog_prune_page(
        &self,
        request: wf_ports::state::GovernanceCatalogRead<'_>,
        limits: wf_ports::state::GovernanceCatalogReadLimits,
    ) -> Result<wf_ports::state::GovernanceCatalogPrunePage, E> {
        if request.limit != 1 {
            return Err(E::InvalidChanges);
        }
        let tx = super::port::guard(self, limits)?;
        tx.checked(|| super::read::page_revision(&tx, request, true))
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
        super::super::values::genesis::create_selected_profile(
            database,
            group,
            recovery,
            genesis,
            defaults,
            limits,
            super::super::values::genesis::ValueGenesisCapabilities {
                alias_format: 2,
                credit_shard_count,
                credit_catalog: credit_shard_count.is_some(),
                governance_catalog: super::super::values::manifest::GovernanceCatalogProfile::Prune,
            },
        )
    }
}
