//! Explicit native metadata capability; no independent attachment operation.
use super::super::{RedbStateStore, StateStoreGenesis, StateStoreLimits};
use wf_domain::workflow::request::RequestScope;
use wf_ports::blob::WorkflowValueReadLimits;
use wf_ports::consensus::WorkflowValueDefaults;
use wf_ports::state::StateTransactionError as Error;
use wf_types::identity::{GroupId, PartitionId, RecordId, RecoveryId};
pub(crate) mod publication;
mod read;
mod record;
pub(crate) mod recovery;
pub(crate) use read::load_current;

impl RedbStateStore {
    /// Creates fresh complete-history authority requiring explicit metadata births.
    /// # Errors
    /// Refuses nonempty/invalid genesis, unsupported capability or uncertain commit.
    pub fn create_metadata_value_database(
        database: redb::Database,
        group: GroupId,
        recovery: RecoveryId,
        genesis: StateStoreGenesis<'_>,
        defaults: WorkflowValueDefaults,
        limits: StateStoreLimits,
    ) -> Result<Self, Error> {
        super::genesis::create_selected_profile(
            database,
            group,
            recovery,
            genesis,
            defaults,
            limits,
            super::genesis::ValueGenesisCapabilities {
                binding_format: 2,
                alias_format: 2,
                credit_shard_count: None,
                credit_catalog: false,
                governance_catalog: super::manifest::GovernanceCatalogProfile::Disabled,
            },
        )
    }

    /// Reads an independently owned opaque capture at one exact current/history point.
    /// Unsupported legacy capture is an error, never an invented empty value.
    /// # Errors
    /// Refuses unsupported capability, mismatched companions or explicit limits.
    pub fn workflow_record_metadata(
        &self,
        scope: &RequestScope,
        partition: &PartitionId,
        id: &RecordId,
        history: Option<&str>,
        limits: WorkflowValueReadLimits,
    ) -> Result<Option<Vec<u8>>, Error> {
        read::observe(self, scope, partition, id, history, limits)
            .map_err(|error| self.failed(error))
    }
}
