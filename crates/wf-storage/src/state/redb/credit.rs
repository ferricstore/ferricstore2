//! Explicit credit capability contracts; no synthetic owner or success.

use super::{RedbStateStore, StateStoreGenesis, StateStoreLimits};
use wf_domain::governance::credit::{CreditLimits, CreditOwner};
use wf_domain::workflow::request::RequestScope;
use wf_ports::consensus::WorkflowValueDefaults;
use wf_ports::state::StateTransactionError;
use wf_types::identity::{GroupId, RecoveryId};

mod read;

impl RedbStateStore {
    /// Creates one atomic empty credit-capable authority with actual shard count.
    /// # Errors
    /// Refuses unsupported capability, input, profile or unproved durability.
    pub fn create_credit_database(
        database: redb::Database,
        group: GroupId,
        recovery: RecoveryId,
        genesis: StateStoreGenesis<'_>,
        defaults: WorkflowValueDefaults,
        shard_count: u64,
        limits: StateStoreLimits,
    ) -> Result<Self, StateTransactionError> {
        super::values::genesis::create_profile(
            database,
            group,
            recovery,
            genesis,
            defaults,
            limits,
            (2, Some(shard_count)),
        )
    }

    /// Reads an exact owner under its own bounded coherent revision.
    /// This observation grants no authority for another transaction.
    /// # Errors
    /// Refuses unsupported capability, corruption or operational admission.
    pub fn read_credit_owner(
        &self,
        scope: &RequestScope,
        credit_scope: &[u8],
        credit: CreditLimits,
        maximum_steps: usize,
        maximum_bytes: usize,
    ) -> Result<Option<CreditOwner>, StateTransactionError> {
        self.ready()?;
        let tx = self.transaction(
            false,
            self.limits.admission(),
            maximum_steps,
            maximum_bytes,
            self.limits.maximum_temporary_bytes(),
        )?;
        tx.checked(|| read::owner(&tx, scope, credit_scope, credit))
    }
}
