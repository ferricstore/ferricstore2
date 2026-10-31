//! Explicit workflow-release custody; callers still need actual execution authority.
use super::{RedbStateStore, RedbStateTransaction, StateStoreGenesis, StateStoreLimits};
use wf_ports::{consensus::WorkflowValueDefaults, state::StateTransactionError as E};
use wf_types::identity::{GroupId, RecoveryId};
impl RedbStateStore {
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
    /// Checks actual immutable GC4 under this same coherent transaction guard.
    /// # Errors
    /// Refuses absent/older/corrupt capability or current admission failure.
    pub fn require_workflow_release(&self) -> Result<(), E> {
        Err(E::UnsupportedPolicyProfile)
    }
}
