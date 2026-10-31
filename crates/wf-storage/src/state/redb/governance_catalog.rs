//! Explicit target membership authority; no unlogged mutation API.

use super::{RedbStateStore, RedbStateTransaction, StateStoreGenesis, StateStoreLimits};
use wf_domain::workflow::request::RequestScope;
use wf_ports::{
    consensus::WorkflowValueDefaults,
    state::{GovernanceCatalogPage, GovernanceCatalogRead, StateTransactionError as E},
};
use wf_types::identity::{GroupId, RecoveryId};

mod read;
mod write;
pub(crate) use write::apply as commit;

pub(crate) fn require(tx: &RedbStateTransaction<'_>, scope: &RequestScope) -> Result<(), E> {
    if scope.group() != tx.group || scope.recovery() != tx.recovery {
        return Err(E::NamespaceConflict);
    }
    if tx.layout != 8 {
        return Err(E::UnsupportedPolicyProfile);
    }
    tx.admit_read(512)?;
    if !tx.value_manifest()?.governance_catalog {
        return Err(E::UnsupportedPolicyProfile);
    }
    Ok(())
}

pub(crate) fn validate_row(value: &[u8]) -> Result<(), E> {
    if value == b"FWGM\0\x01" {
        Ok(())
    } else {
        Err(E::Corrupt)
    }
}

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
        request: GovernanceCatalogRead<'_>,
        maximum_steps: usize,
        maximum_bytes: usize,
    ) -> Result<GovernanceCatalogPage, E> {
        self.ready()?;
        let tx = self.transaction(
            false,
            self.limits.admission(),
            maximum_steps,
            maximum_bytes,
            self.limits.maximum_temporary_bytes(),
        )?;
        tx.checked(|| read::page(&tx, request))
    }
    /// Reads exact binary membership under one bounded coherent revision.
    /// # Errors
    /// Refuses capability/namespace/corruption or continuous read admission.
    pub fn governance_catalog_member(
        &self,
        scope: &RequestScope,
        catalog: &[u8],
        member: &[u8],
        maximum_steps: usize,
        maximum_bytes: usize,
    ) -> Result<bool, E> {
        self.ready()?;
        let tx = self.transaction(
            false,
            self.limits.admission(),
            maximum_steps,
            maximum_bytes,
            self.limits.maximum_temporary_bytes(),
        )?;
        tx.checked(|| {
            require(&tx, scope)?;
            let key = read::key(&tx, scope, catalog, Some(member))?;
            tx.step()?;
            let table = tx
                .write
                .open_table(tx.rows_table)
                .map_err(super::admission::table_error)?;
            read::contains(&tx, &table, &key)
        })
    }
}
