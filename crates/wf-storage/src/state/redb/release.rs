//! Explicit workflow-release custody; callers still need actual execution authority.
use super::{RedbStateStore, RedbStateTransaction, StateStoreGenesis, StateStoreLimits};
use wf_ports::{consensus::WorkflowValueDefaults, state::StateTransactionError as E};
use wf_types::identity::{GroupId, RecoveryId};
mod read;
pub(in crate::state::redb) mod recovery;
impl RedbStateStore {
    /// Observes at most256 original queue entries under one coherent local guard.
    /// A production caller must establish its required leader/read-barrier lifecycle.
    /// # Errors
    /// Refuses other capabilities/namespaces, invalid bounds, corrupt data or admission.
    pub fn read_workflow_release_outbox(
        &self,
        scope: &wf_domain::workflow::request::RequestScope,
        after: Option<u64>,
        limit: u16,
        maximum_steps: usize,
        maximum_bytes: usize,
    ) -> Result<wf_ports::state::WorkflowReleasePage, E> {
        self.ready()?;
        let tx = self.transaction(
            false,
            self.limits.admission(),
            maximum_steps,
            maximum_bytes,
            self.limits.maximum_temporary_bytes(),
        )?;
        tx.checked(|| read::page(&tx, scope, after, limit))
    }
    /// Creates the selected GC4 namespace, including identified credit2/catalog.
    /// This capability alone supplies neither reservation binding nor a Claim service.
    /// # Errors
    /// Refuses invalid capability/genesis/resource data or unproved durability.
    pub fn create_workflow_release_database(
        database: redb::Database,
        group: GroupId,
        recovery: RecoveryId,
        genesis: StateStoreGenesis<'_>,
        defaults: WorkflowValueDefaults,
        credit_shard_count: u64,
        limits: StateStoreLimits,
    ) -> Result<Self, E> {
        super::values::genesis::create_selected_profile(
            database,
            group,
            recovery,
            genesis,
            defaults,
            limits,
            super::values::genesis::ValueGenesisCapabilities {
                alias_format: 2,
                credit_shard_count: Some(credit_shard_count),
                credit_catalog: true,
                governance_catalog:
                    super::values::manifest::GovernanceCatalogProfile::WorkflowRelease,
            },
        )
    }
}
impl RedbStateTransaction<'_> {
    /// Reads the original paid generation and record reservation under this guard.
    /// The observation grants no current spend or mutation authority.
    /// # Errors
    /// Refuses other capabilities/namespaces, corrupt joins or cumulative admission.
    pub fn workflow_governance_binding(
        &self,
        scope: &wf_domain::workflow::request::RequestScope,
        partition: &wf_types::identity::PartitionId,
        id: &wf_types::identity::RecordId,
    ) -> Result<Option<(u64, wf_domain::governance::release::GovernanceReservation)>, E> {
        self.checked(|| read::binding(self, scope, partition, id))
    }
    /// Checks actual immutable GC4 under this same coherent transaction guard.
    /// # Errors
    /// Refuses absent/older/corrupt capability or current admission failure.
    pub fn require_workflow_release(&self) -> Result<(), E> {
        if self.governance_catalog_profile()? != 4 {
            return Err(E::UnsupportedPolicyProfile);
        }
        Ok(())
    }
}
