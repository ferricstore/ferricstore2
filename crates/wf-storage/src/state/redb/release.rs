//! Explicit workflow-release custody; callers still need actual execution authority.
use super::{RedbStateStore, RedbStateTransaction, StateStoreGenesis, StateStoreLimits};
use wf_ports::{consensus::WorkflowValueDefaults, state::StateTransactionError as E};
use wf_types::identity::{GroupId, RecoveryId};
pub(super) mod claimed;
mod plan;
mod read;
mod write;
pub(crate) use plan::{ReleasePreparation, ReleasePublicationPlan};
pub(super) use write::{VerifiedReleaseWrites, publish, validate};

pub(crate) fn prepare(
    tx: &RedbStateTransaction<'_>,
    scope: &wf_domain::workflow::request::RequestScope,
    changes: &wf_ports::log::RecordBatchApplyPlan<'_>,
) -> Result<ReleasePreparation, E> {
    tx.checked(|| plan::prepare(tx, scope, changes))
}
pub(in crate::state::redb) mod recovery;
impl RedbStateStore {
    /// Creates GC5 custody for consumed paid IDs, including GC4 release support.
    /// The capability supplies no caller authorization or Claim execution by itself.
    /// # Errors
    /// Refuses invalid genesis, dependencies, resource limits or unproved durability.
    pub fn create_governed_claim_database(
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
                    super::values::manifest::GovernanceCatalogProfile::GovernedClaim,
            },
        )
    }
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
    pub(crate) fn require_governed_claim(&self) -> Result<(), E> {
        if self.governance_catalog_profile()? != 5 {
            return Err(E::UnsupportedPolicyProfile);
        }
        Ok(())
    }
    pub(crate) fn commit_workflow_release_values(
        self,
        changes: &wf_ports::state::GroupApplyChanges<'_, '_>,
        values: &mut crate::transaction::ValuePublicationPlan<'_>,
        release: &ReleasePublicationPlan,
    ) -> Result<(), E> {
        super::commit::values::apply_release(self, changes, values, release)
    }
    pub(super) fn refuse_legacy_governance_write(
        &self,
        scope: &wf_domain::workflow::request::RequestScope,
        partition: &wf_types::identity::PartitionId,
        id: &wf_types::identity::RecordId,
    ) -> Result<(), E> {
        if !matches!(self.catalog_profile, 4 | 5) {
            return Ok(());
        }
        read::refuse_legacy_write(self, scope, partition, id)
    }
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
        if !matches!(self.governance_catalog_profile()?, 4 | 5) {
            return Err(E::UnsupportedPolicyProfile);
        }
        Ok(())
    }
}
