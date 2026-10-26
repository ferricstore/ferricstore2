//! Explicit value-capable genesis and coherent read boundary.

use wf_domain::workflow::request::RequestScope;
use wf_ports::blob::{
    WorkflowValueBindingKnowledge, WorkflowValueReadBatch, WorkflowValueReadLimits,
};
use wf_ports::clock::WorkflowValueReadClock;
use wf_ports::consensus::WorkflowValueDefaults;
use wf_ports::state::StateTransactionError;
use wf_types::identity::{GroupId, PartitionId, RecordId, RecoveryId};

use super::{RedbStateStore, StateStoreGenesis, StateStoreLimits};

impl RedbStateStore {
    /// Creates a fresh authority with explicit workflow value capability.
    ///
    /// # Errors
    /// Refuses unsupported persistent value layouts.
    pub fn create_value_database(
        _database: redb::Database,
        _group: GroupId,
        _recovery: RecoveryId,
        _genesis: StateStoreGenesis<'_>,
        _defaults: WorkflowValueDefaults,
        _limits: StateStoreLimits,
    ) -> Result<Self, StateTransactionError> {
        Err(StateTransactionError::UnsupportedLayout)
    }

    /// Reads ordered references against one committed snapshot and clock pair.
    ///
    /// # Errors
    /// Refuses unsupported persistent value layouts or operational failure.
    pub fn read_workflow_value_batch<C: WorkflowValueReadClock>(
        &self,
        _clock: &mut C,
        _scope: &RequestScope,
        _refs: &[&[u8]],
        _limits: WorkflowValueReadLimits,
    ) -> Result<WorkflowValueReadBatch, StateTransactionError> {
        Err(StateTransactionError::UnsupportedLayout)
    }

    /// Observes a current binding or one explicit historical binding.
    ///
    /// # Errors
    /// Refuses unsupported persistent value layouts or operational failure.
    pub fn workflow_value_binding(
        &self,
        _scope: &RequestScope,
        _partition: &PartitionId,
        _id: &RecordId,
        _history: Option<&str>,
        _limits: WorkflowValueReadLimits,
    ) -> Result<WorkflowValueBindingKnowledge, StateTransactionError> {
        Err(StateTransactionError::UnsupportedLayout)
    }
}
