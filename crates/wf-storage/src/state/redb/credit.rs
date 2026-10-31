//! Explicit credit capability contracts; no synthetic owner or success.

use super::{RedbStateStore, RedbStateTransaction, StateStoreGenesis, StateStoreLimits};
use wf_domain::governance::credit::{CreditLimits, CreditOwner};
use wf_domain::workflow::request::RequestScope;
use wf_ports::consensus::WorkflowValueDefaults;
use wf_ports::state::StateTransactionError;
use wf_types::identity::{GroupId, RecoveryId};

pub(super) mod catalog;
mod context;
mod read;
pub(super) mod recovery;
mod write;
pub(crate) use catalog::ack::{apply as commit_catalog_ack, current as catalog_ack_meta};
pub(crate) use catalog::observation as publication_observation;
pub(crate) use context::CreditReadContext;
pub(crate) use write::apply as commit;

impl RedbStateTransaction<'_> {
    /// Observes exact credit format and immutable shard count under this guard.
    /// # Errors
    /// Refuses absent capability, corruption or continuous read admission.
    pub fn require_identified_credit_profile(&self) -> Result<(u16, u64), StateTransactionError> {
        self.checked(|| {
            self.admit_read(512)?;
            let manifest = self.value_manifest()?;
            let count = manifest
                .credit_shard_count
                .ok_or(StateTransactionError::UnsupportedPolicyProfile)?;
            Ok((if manifest.credit_catalog { 2 } else { 1 }, count))
        })
    }

    /// Observes the actual immutable credit capability under this same guard.
    /// Absence is never inferred from owner or reservation row counts.
    /// # Errors
    /// Refuses unsupported capability, corruption or continuous admission.
    pub fn require_identified_credit(&self) -> Result<u64, StateTransactionError> {
        self.checked(|| {
            self.admit_read(512)?;
            self.value_manifest()?
                .credit_shard_count
                .ok_or(StateTransactionError::UnsupportedPolicyProfile)
        })
    }
}

impl RedbStateStore {
    /// Creates an explicit credit-format2 authority with atomic publication intents.
    /// # Errors
    /// Refuses unsupported capability, invalid genesis or unproved durability.
    pub fn create_credit_catalog_database(
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
            (2, Some(shard_count), true, false),
        )
    }

    /// Reads at most256 consecutive publication entries under one coherent guard.
    /// Missing/corrupt active entries never become an empty successful page.
    /// # Errors
    /// Refuses old capabilities, invalid bounds, corruption or read admission.
    pub fn read_credit_catalog(
        &self,
        scope: &RequestScope,
        limit: u16,
        maximum_steps: usize,
        maximum_bytes: usize,
    ) -> Result<wf_ports::state::CreditCatalogPage, StateTransactionError> {
        self.ready()?;
        let tx = self.transaction(
            false,
            self.limits.admission(),
            maximum_steps,
            maximum_bytes,
            self.limits.maximum_temporary_bytes(),
        )?;
        tx.checked(|| catalog::page(&tx, scope, limit))
    }

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
            (2, Some(shard_count), false, false),
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
