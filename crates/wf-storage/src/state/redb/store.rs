//! The one scoped database owner and its exclusive, consumed transaction.

use super::{StateStoreGenesis, StateStoreLimits, admission, bootstrap, recovery};
use std::{cell::Cell, path::Path};
use wf_domain::workflow::admission::AdmissionLimits;
use wf_ports::state::{GroupApplyStore, StateTransactionError};
use wf_types::identity::{GroupId, RecoveryId};

/// Exclusive fixed group/recovery authority. Fatal failure requires reopening.
pub struct RedbStateStore {
    pub(super) database: redb::Database,
    pub(super) group: GroupId,
    pub(super) recovery: RecoveryId,
    pub(super) limits: StateStoreLimits,
    fenced: Cell<bool>,
}

/// One writer revision, with cumulative returned and temporary owner accounts.
pub struct RedbStateTransaction<'s> {
    pub(super) write: redb::WriteTransaction,
    pub(super) group: &'s GroupId,
    pub(super) recovery: &'s RecoveryId,
    pub(super) admission: AdmissionLimits,
    pub(super) limits: StateStoreLimits,
    pub(super) fenced: &'s Cell<bool>,
    pub(super) maximum_steps: usize,
    pub(super) maximum_read: usize,
    pub(super) maximum_temporary: usize,
    pub(super) steps: Cell<usize>,
    pub(super) read: Cell<usize>,
    pub(super) temporary: Cell<usize>,
    pub(super) recovering: bool,
    pub(super) inspected: Cell<usize>,
    pub(super) wire: Cell<usize>,
}

impl RedbStateStore {
    /// Creates a wholly empty inventory and syncs its new parent directory entry.
    /// A bare relative filename uses the current directory; no directory is made.
    /// # Errors
    /// Refuses input, namespace, resource or unproved durability failures.
    pub fn create(
        path: &Path,
        group: GroupId,
        recovery: RecoveryId,
        genesis: StateStoreGenesis<'_>,
        limits: StateStoreLimits,
    ) -> Result<Self, StateTransactionError> {
        let scratch = bootstrap::admit(&group, &recovery, &genesis, limits)?;
        let database = redb::Builder::new()
            .set_cache_size(16_777_216)
            .create(path)
            .map_err(admission::database_error)?;
        let store = Self::initialized(database, group, recovery, &genesis, limits, scratch)?;
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        std::fs::File::open(parent)
            .and_then(|file| file.sync_all())
            .map_err(|_| StateTransactionError::Uncertain)?;
        Ok(store)
    }
    /// Opens an existing namespace without creating any table or default row.
    /// # Errors
    /// Refuses missing, corrupt, foreign or operationally inadmissible data.
    pub fn open(
        path: &Path,
        group: GroupId,
        recovery: RecoveryId,
        limits: StateStoreLimits,
    ) -> Result<Self, StateTransactionError> {
        admission::workspace(&group, &recovery, limits, 0)?;
        let database = redb::Builder::new()
            .set_cache_size(16_777_216)
            .open(path)
            .map_err(admission::database_error)?;
        Self::open_database(database, group, recovery, limits)
    }
    /// Consumes a database; the caller owns cache/file/directory durability setup.
    /// # Errors
    /// Refuses input, existing tables or unproved initial commit durability.
    pub fn create_database(
        database: redb::Database,
        group: GroupId,
        recovery: RecoveryId,
        genesis: StateStoreGenesis<'_>,
        limits: StateStoreLimits,
    ) -> Result<Self, StateTransactionError> {
        let scratch = bootstrap::admit(&group, &recovery, &genesis, limits)?;
        Self::initialized(database, group, recovery, &genesis, limits, scratch)
    }
    fn initialized(
        database: redb::Database,
        group: GroupId,
        recovery: RecoveryId,
        genesis: &StateStoreGenesis<'_>,
        limits: StateStoreLimits,
        scratch: usize,
    ) -> Result<Self, StateTransactionError> {
        let store = Self {
            database,
            group,
            recovery,
            limits,
            fenced: Cell::new(false),
        };
        bootstrap::initialize(&store, genesis, scratch)?;
        Ok(store)
    }
    /// Consumes a database and validates the complete coherent persisted revision.
    /// The caller owns cache/file/directory setup. No tables are manufactured.
    /// # Errors
    /// Refuses foreign/corrupt inventory or bounded recovery exhaustion.
    pub fn open_database(
        database: redb::Database,
        group: GroupId,
        recovery: RecoveryId,
        limits: StateStoreLimits,
    ) -> Result<Self, StateTransactionError> {
        admission::workspace(&group, &recovery, limits, 0)?;
        let store = Self {
            database,
            group,
            recovery,
            limits,
            fenced: Cell::new(false),
        };
        recovery::validate(&store)?;
        Ok(store)
    }
    /// Fixed authoritative group; available even after a fatal error.
    #[must_use]
    pub const fn group(&self) -> &GroupId {
        &self.group
    }
    /// Fixed authoritative recovery; available even after a fatal error.
    #[must_use]
    pub const fn recovery(&self) -> &RecoveryId {
        &self.recovery
    }

    pub(super) fn transaction(
        &self,
        recovering: bool,
        admission: AdmissionLimits,
        steps: usize,
        read: usize,
        temporary: usize,
    ) -> Result<RedbStateTransaction<'_>, StateTransactionError> {
        if self.fenced.get() {
            return Err(StateTransactionError::Unavailable);
        }
        if steps == 0 || read == 0 || temporary == 0 {
            return Err(StateTransactionError::Limit);
        }
        let write = self
            .database
            .begin_write()
            .map_err(admission::transaction_error)
            .inspect_err(|error| fence(&self.fenced, *error))?;
        Ok(RedbStateTransaction {
            write,
            group: &self.group,
            recovery: &self.recovery,
            admission,
            limits: self.limits,
            fenced: &self.fenced,
            maximum_steps: steps,
            maximum_read: read,
            maximum_temporary: temporary,
            steps: Cell::new(0),
            read: Cell::new(0),
            temporary: Cell::new(0),
            recovering,
            inspected: Cell::new(0),
            wire: Cell::new(0),
        })
    }
}

impl GroupApplyStore for RedbStateStore {
    type Transaction<'s> = RedbStateTransaction<'s>;
    fn begin_apply<'s>(
        &'s mut self,
        group: &'s GroupId,
        recovery: &'s RecoveryId,
        profile: AdmissionLimits,
        steps: usize,
        read: usize,
        temporary: usize,
    ) -> Result<Self::Transaction<'s>, StateTransactionError> {
        if self.fenced.get() {
            return Err(StateTransactionError::Unavailable);
        }
        let cap = self.limits.admission();
        if group != &self.group
            || recovery != &self.recovery
            || profile.inline_bytes() > cap.inline_bytes()
            || profile.decision_bytes() > cap.decision_bytes()
            || profile.batch_items() > cap.batch_items()
        {
            return Err(StateTransactionError::InvalidChanges);
        }
        self.transaction(false, profile, steps, read, temporary)
    }
}

impl RedbStateTransaction<'_> {
    pub(super) fn ready(&self) -> Result<(), StateTransactionError> {
        if self.fenced.get() {
            Err(StateTransactionError::Unavailable)
        } else {
            Ok(())
        }
    }
    pub(super) fn checked<T>(
        &self,
        work: impl FnOnce() -> Result<T, StateTransactionError>,
    ) -> Result<T, StateTransactionError> {
        self.ready()?;
        work().inspect_err(|error| fence(self.fenced, *error))
    }
}

pub(super) fn fence(fenced: &Cell<bool>, error: StateTransactionError) {
    if matches!(
        error,
        StateTransactionError::Corrupt
            | StateTransactionError::Unavailable
            | StateTransactionError::Uncertain
    ) {
        fenced.set(true);
    }
}
