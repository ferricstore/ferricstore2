//! Explicit GC2 derived hints; no unlogged mutation or repair API.
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
mod read;
mod write;
pub(crate) use write::apply as commit;

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
        super::super::values::genesis::create_selected_profile(
            database,
            group,
            recovery,
            genesis,
            defaults,
            limits,
            super::super::values::genesis::ValueGenesisCapabilities {
                binding_format: 1,
                alias_format: 2,
                credit_shard_count,
                credit_catalog: credit_shard_count.is_some(),
                governance_catalog:
                    super::super::values::manifest::GovernanceCatalogProfile::Progress,
            },
        )
    }
}
impl RedbStateTransaction<'_> {
    /// Requires actual immutable GC2 under this same coherent guard.
    /// # Errors
    /// Refuses unsupported profiles, resources or corrupt authority.
    pub fn require_catalog_progress(&self) -> Result<(), E> {
        if !matches!(self.governance_catalog_profile()?, 2..=5) {
            return Err(E::UnsupportedPolicyProfile);
        }
        Ok(())
    }
    /// Reads actual immutable governance capability0..4, without enabling it.
    /// # Errors
    /// Refuses unsupported layouts, corruption or resource exhaustion.
    pub fn governance_catalog_profile(&self) -> Result<u16, E> {
        self.checked(|| {
            if self.layout != 8 {
                return Err(E::UnsupportedPolicyProfile);
            }
            self.admit_read(512)?;
            let manifest = self.value_manifest()?;
            Ok(manifest.governance_catalog.version())
        })
    }
}
impl GovernanceCatalogProgressPort for RedbStateStore {
    fn governance_catalog_progress(
        &self,
        scope: &RequestScope,
        catalog: &[u8],
        limits: GovernanceCatalogReadLimits,
    ) -> Result<GovernanceCatalogProgress, E> {
        if [
            limits.maximum_steps,
            limits.maximum_bytes,
            limits.maximum_temporary_bytes,
        ]
        .contains(&0)
        {
            return Err(E::InvalidLimits);
        }
        self.ready()?;
        let tx = self.transaction(
            false,
            self.limits.admission(),
            limits.maximum_steps,
            limits.maximum_bytes,
            limits.maximum_temporary_bytes,
        )?;
        tx.checked(|| read::read(&tx, scope, catalog))
    }
}
