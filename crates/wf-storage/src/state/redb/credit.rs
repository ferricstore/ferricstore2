//! Explicit credit capability contracts; no synthetic owner or success.

use super::{RedbStateStore, StateStoreGenesis, StateStoreLimits};
use wf_domain::governance::credit::{CreditLimits, CreditOwner};
use wf_domain::workflow::request::RequestScope;
use wf_ports::consensus::WorkflowValueDefaults;
use wf_ports::state::StateTransactionError;
use wf_types::identity::{GroupId, RecoveryId};

impl RedbStateStore {
    /// Creates one atomic empty credit-capable authority with actual shard count.
    /// # Errors
    /// Refuses unsupported capability, input, profile or unproved durability.
    pub fn create_credit_database(
        _database: redb::Database,
        _group: GroupId,
        _recovery: RecoveryId,
        _genesis: StateStoreGenesis<'_>,
        _defaults: WorkflowValueDefaults,
        _shard_count: u64,
        _limits: StateStoreLimits,
    ) -> Result<Self, StateTransactionError> {
        Err(StateTransactionError::UnsupportedLayout)
    }

    /// Reads an exact owner under its own bounded coherent revision.
    /// This observation grants no authority for another transaction.
    /// # Errors
    /// Refuses unsupported capability, corruption or operational admission.
    pub fn read_credit_owner(
        &self,
        _scope: &RequestScope,
        _credit_scope: &[u8],
        _credit: CreditLimits,
        _maximum_steps: usize,
        _maximum_bytes: usize,
    ) -> Result<Option<CreditOwner>, StateTransactionError> {
        Err(StateTransactionError::UnsupportedLayout)
    }
}
