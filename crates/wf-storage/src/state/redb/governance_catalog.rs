//! Explicit target membership authority; no unlogged mutation API.

use super::{RedbStateStore, StateStoreGenesis, StateStoreLimits};
use wf_domain::workflow::request::RequestScope;
use wf_ports::{
    consensus::WorkflowValueDefaults,
    state::{GovernanceCatalogPage, GovernanceCatalogRead, StateTransactionError as E},
};
use wf_types::identity::{GroupId, RecoveryId};

impl RedbStateStore {
    /// Creates a target catalog authority without credit-owner shard assumptions.
    /// # Errors
    /// Refuses invalid genesis, unsupported capability or unproved durability.
    pub fn create_governance_catalog_database(
        database: redb::Database,
        group: GroupId,
        recovery: RecoveryId,
        genesis: StateStoreGenesis<'_>,
        defaults: WorkflowValueDefaults,
        limits: StateStoreLimits,
    ) -> Result<Self, E> {
        super::values::genesis::create_profile(
            database,
            group,
            recovery,
            genesis,
            defaults,
            limits,
            (2, None, false, true),
        )
    }
    /// Reads one coherent page, requiring an actual surviving cursor member.
    /// # Errors
    /// Refuses capability/namespace/cursor corruption or explicit read limits.
    pub fn read_governance_catalog(
        &self,
        _request: GovernanceCatalogRead<'_>,
        _maximum_steps: usize,
        _maximum_bytes: usize,
    ) -> Result<GovernanceCatalogPage, E> {
        Err(E::UnsupportedPolicyProfile)
    }
    /// Reads exact binary membership under one bounded coherent revision.
    /// # Errors
    /// Refuses capability/namespace/corruption or continuous read admission.
    pub fn governance_catalog_member(
        &self,
        _scope: &RequestScope,
        _catalog: &[u8],
        _member: &[u8],
        _maximum_steps: usize,
        _maximum_bytes: usize,
    ) -> Result<bool, E> {
        Err(E::UnsupportedPolicyProfile)
    }
}
