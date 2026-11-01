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
pub(in crate::state::redb) mod genesis;
pub(crate) mod manifest;
pub(crate) mod metadata;
pub(crate) mod read;
pub(crate) mod validate;

impl RedbStateStore {
    /// Creates fresh explicit stored-alias capability without seeded records.
    /// # Errors
    /// Refuses unsupported capability, invalid/nonempty genesis or durability failure.
    pub fn create_stored_value_database(
        database: redb::Database,
        group: GroupId,
        recovery: RecoveryId,
        genesis: StateStoreGenesis<'_>,
        defaults: WorkflowValueDefaults,
        limits: StateStoreLimits,
    ) -> Result<Self, StateTransactionError> {
        genesis::create(database, group, recovery, genesis, defaults, limits, 2)
    }

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
        genesis::create(database, group, recovery, genesis, defaults, limits, 1)
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
    /// Requires actual stored-value capability in this same bounded guard.
    ///
    /// No control, catalog or workflow row is changed by this observation.
    /// # Errors
    /// Refuses absent capability, invalid metadata or read/temporary admission.
    pub fn require_stored_workflow_values(&self) -> Result<(), StateTransactionError> {
        self.checked(|| {
            let owner = super::admission::total(
                512,
                [
                    super::admission::total(
                        208,
                        [self.group.as_bytes().len(), self.recovery.as_bytes().len()],
                    )?,
                    std::mem::size_of::<manifest::ValueManifest>(),
                ],
            )?;
            self.admit_read(owner)?;
            self.temporary(owner)?;
            if self.value_manifest()?.alias_format != 2 {
                return Err(StateTransactionError::UnsupportedPolicyProfile);
            }
            Ok(())
        })
    }

    pub(crate) fn value_manifest(&self) -> Result<manifest::ValueManifest, StateTransactionError> {
        if !matches!(self.layout, 7 | 8) {
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

    pub(crate) fn commit_claim_values(
        self,
        changes: &wf_ports::state::GroupClaimChanges<'_, '_>,
        publication: &mut crate::transaction::ClaimValuePublicationPlan<'_>,
    ) -> Result<(), StateTransactionError> {
        super::commit::claim::apply_values(self, changes, publication)
    }
    pub(crate) fn commit_governed_claim_values(
        self,
        changes: &wf_ports::state::GroupClaimChanges<'_, '_>,
        publication: &mut crate::transaction::ClaimValuePublicationPlan<'_>,
        paid: (
            wf_ports::log::CapturedGovernedClaimPayment<'_>,
            wf_domain::governance::credit::CreditLimits,
        ),
    ) -> Result<(), StateTransactionError> {
        super::commit::claim::apply_governed(self, changes, publication, paid)
    }
}
