//! Explicit value-capable genesis, same-guard mutation and owned read boundary.

use wf_domain::workflow::request::RequestScope;
use wf_ports::blob::{
    WorkflowValueBindingKnowledge, WorkflowValueReadBatch, WorkflowValueReadLimits,
    WorkflowValueSizedReadBatch,
};
use wf_ports::clock::WorkflowValueReadClock;
use wf_ports::consensus::WorkflowValueDefaults;
use wf_ports::state::StateTransactionError;
use wf_types::identity::{GroupId, PartitionId, RecordId, RecoveryId};

use super::{RedbStateStore, StateStoreGenesis, StateStoreLimits};

pub(crate) mod binding;
pub(crate) mod budget;
mod genesis;
pub(crate) mod manifest;
pub(crate) mod read;
pub(crate) mod validate;

impl RedbStateStore {
    /// Creates a fresh authority with explicit workflow value capability.
    ///
    /// # Errors
    /// Refuses nonempty/invalid genesis or unproved durable completion.
    pub fn create_value_database(
        database: redb::Database,
        group: GroupId,
        recovery: RecoveryId,
        genesis: StateStoreGenesis<'_>,
        defaults: WorkflowValueDefaults,
        limits: StateStoreLimits,
    ) -> Result<Self, StateTransactionError> {
        genesis::create(database, group, recovery, genesis, defaults, limits)
    }

    /// Reads ordered references against one committed snapshot and clock pair.
    ///
    /// # Errors
    /// Refuses unsupported persistent value layouts or operational failure.
    pub fn read_workflow_value_batch<C: WorkflowValueReadClock>(
        &self,
        clock: &mut C,
        scope: &RequestScope,
        refs: &[&[u8]],
        limits: WorkflowValueReadLimits,
    ) -> Result<WorkflowValueReadBatch, StateTransactionError> {
        read::read(self, clock, scope, refs, limits)
    }

    /// Reads authenticated live values, omitting bytes at a positive threshold.
    ///
    /// Omission uses complete stored size. Copied-byte exhaustion stays an error.
    /// # Errors
    /// Refuses zero threshold, invalid scope/profile or operational failure.
    pub fn read_workflow_value_batch_with_sizes<C: WorkflowValueReadClock>(
        &self,
        clock: &mut C,
        scope: &RequestScope,
        refs: &[&[u8]],
        omission_threshold: usize,
        limits: WorkflowValueReadLimits,
    ) -> Result<WorkflowValueSizedReadBatch, StateTransactionError> {
        read::read_sized(self, clock, scope, refs, omission_threshold, limits)
    }

    /// Observes a current binding or one explicit historical binding.
    ///
    /// # Errors
    /// Refuses unsupported persistent value layouts or operational failure.
    pub fn workflow_value_binding(
        &self,
        scope: &RequestScope,
        partition: &PartitionId,
        id: &RecordId,
        history: Option<&str>,
        limits: WorkflowValueReadLimits,
    ) -> Result<WorkflowValueBindingKnowledge, StateTransactionError> {
        binding::observe(self, scope, partition, id, history, limits)
    }
}

impl super::RedbStateTransaction<'_> {
    pub(crate) fn value_manifest(&self) -> Result<manifest::ValueManifest, StateTransactionError> {
        if self.layout != 7 {
            return Err(StateTransactionError::UnsupportedLayout);
        }
        manifest::read(self)
    }

    pub(crate) fn commit_values(
        self,
        changes: &wf_ports::state::GroupApplyChanges<'_, '_>,
        publication: &mut crate::transaction::ValuePublicationPlan<'_>,
    ) -> Result<(), StateTransactionError> {
        super::commit::values::apply(self, changes, publication)
    }
}
