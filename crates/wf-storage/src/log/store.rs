//! Exclusive backend ownership and durable creation, including directory entry sync.

use std::{ops::Bound, path::Path};

use redb::{Durability, WriteTransaction};
use wf_ports::log::{
    RaftLogEntry, RaftLogError, RaftLogLimits, RaftLogState, RaftLogStore, RaftVote,
};
use wf_types::identity::{GroupId, RecoveryId};
use wf_types::version::LogIdentity;

use super::{format, read, recovery, write};

/// One exclusively owned scoped database; uncertainty requires dropping and reopening.
///
/// Engine pages/cache and externally retained pages are outside logical owner caps.
pub struct RedbLogStore {
    pub(super) database: redb::Database,
    pub(super) group: GroupId,
    pub(super) recovery: RecoveryId,
    pub(super) limits: RaftLogLimits,
    fenced: bool,
}

impl RedbLogStore {
    /// Creates the two-table namespace, commits it, then syncs the parent directory.
    /// No directory is created; a bare relative filename uses the current directory.
    /// # Errors
    /// Refuses limits, an existing namespace, engine failure or unproved durability.
    pub fn create(
        path: &Path,
        group: GroupId,
        recovery: RecoveryId,
        limits: RaftLogLimits,
    ) -> Result<Self, RaftLogError> {
        base(&group, &recovery, limits)?;
        let database = redb::Builder::new()
            .set_cache_size(16_777_216)
            .create(path)
            .map_err(format::database_error)?;
        let store = Self::create_database(database, group, recovery, limits)?;
        let parent = path
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        std::fs::File::open(parent)
            .and_then(|directory| directory.sync_all())
            .map_err(|_| RaftLogError::Uncertain)?;
        Ok(store)
    }

    /// Opens an existing file and validates its complete bounded cohort.
    /// # Errors
    /// Refuses missing/corrupt data, foreign namespace, profile or engine failure.
    pub fn open(
        path: &Path,
        group: GroupId,
        recovery: RecoveryId,
        limits: RaftLogLimits,
    ) -> Result<Self, RaftLogError> {
        base(&group, &recovery, limits)?;
        let database = redb::Builder::new()
            .set_cache_size(16_777_216)
            .open(path)
            .map_err(format::database_error)?;
        Self::open_database(database, group, recovery, limits)
    }

    /// Consumes an engine and initializes only a completely empty table inventory.
    /// The caller owns engine/cache configuration and durable file/directory setup.
    /// # Errors
    /// Refuses limits, existing tables, engine failure or uncertain commit.
    pub fn create_database(
        database: redb::Database,
        group: GroupId,
        recovery: RecoveryId,
        limits: RaftLogLimits,
    ) -> Result<Self, RaftLogError> {
        base(&group, &recovery, limits)?;
        let store = Self {
            database,
            group,
            recovery,
            limits,
            fenced: false,
        };
        initialize(&store)?;
        Ok(store)
    }

    /// Consumes an engine without manufacturing missing tables or metadata.
    /// The caller owns engine/cache configuration and durable file/directory setup.
    /// # Errors
    /// Refuses inconsistent/foreign cohorts, profile or engine failure.
    pub fn open_database(
        database: redb::Database,
        group: GroupId,
        recovery: RecoveryId,
        limits: RaftLogLimits,
    ) -> Result<Self, RaftLogError> {
        base(&group, &recovery, limits)?;
        let store = Self {
            database,
            group,
            recovery,
            limits,
            fenced: false,
        };
        recovery::validate(&store)?;
        Ok(store)
    }

    pub(super) fn workspace(&self, extra: usize) -> Result<(), RaftLogError> {
        let bytes = format::sum(&[
            512,
            self.group.as_bytes().len(),
            self.recovery.as_bytes().len(),
            2048,
            extra,
        ])?;
        if bytes > self.limits.maximum_workspace_bytes() {
            return Err(RaftLogError::Limit);
        }
        Ok(())
    }

    fn ready(&self) -> Result<(), RaftLogError> {
        if self.fenced {
            Err(RaftLogError::Fenced)
        } else {
            self.workspace(0)
        }
    }

    fn failure(&mut self, error: RaftLogError) -> RaftLogError {
        if matches!(
            error,
            RaftLogError::Unavailable | RaftLogError::Uncertain | RaftLogError::Corrupt
        ) {
            self.fenced = true;
        }
        error
    }
}

impl RaftLogStore for RedbLogStore {
    fn group(&self) -> &GroupId {
        &self.group
    }
    fn recovery(&self) -> &RecoveryId {
        &self.recovery
    }
    fn state(&mut self) -> Result<RaftLogState, RaftLogError> {
        self.ready()?;
        read::state(self).map_err(|error| self.failure(error))
    }
    fn save_vote(&mut self, vote: RaftVote) -> Result<(), RaftLogError> {
        self.ready()?;
        write::save_vote(self, vote).map_err(|error| self.failure(error))
    }
    fn save_committed(&mut self, id: LogIdentity) -> Result<(), RaftLogError> {
        self.ready()?;
        write::save_committed(self, id).map_err(|error| self.failure(error))
    }
    fn append(&mut self, entries: Vec<RaftLogEntry>) -> Result<(), RaftLogError> {
        self.ready()?;
        write::append(self, &entries).map_err(|error| self.failure(error))
    }
    fn read_entry(
        &mut self,
        index: u64,
        maximum_owned_bytes: usize,
    ) -> Result<Option<RaftLogEntry>, RaftLogError> {
        self.ready()?;
        read::point(self, index, maximum_owned_bytes).map_err(|error| self.failure(error))
    }
    fn read_page(
        &mut self,
        start: Bound<u64>,
        end: Bound<u64>,
    ) -> Result<Vec<RaftLogEntry>, RaftLogError> {
        self.ready()?;
        read::page(self, start, end).map_err(|error| self.failure(error))
    }
    fn truncate_from(&mut self, index: u64) -> Result<(), RaftLogError> {
        self.ready()?;
        write::truncate(self, index).map_err(|error| self.failure(error))
    }
    fn purge_through(
        &mut self,
        position: wf_types::version::LogIdentity,
        snapshot_id: [u8; 32],
    ) -> Result<(), RaftLogError> {
        self.ready()?;
        super::purge::through(self, position, snapshot_id).map_err(|error| self.failure(error))
    }
}

fn base(group: &GroupId, recovery: &RecoveryId, limits: RaftLogLimits) -> Result<(), RaftLogError> {
    if limits.maximum_entry_bytes() > 3 * 1024 * 1024 * 1024 - 70 {
        return Err(RaftLogError::InvalidLimits);
    }
    let required = format::sum(&[512, group.as_bytes().len(), recovery.as_bytes().len(), 2048])?;
    if required > limits.maximum_workspace_bytes() {
        return Err(RaftLogError::Limit);
    }
    Ok(())
}

fn initialize(store: &RedbLogStore) -> Result<(), RaftLogError> {
    let tx = store
        .database
        .begin_write()
        .map_err(format::transaction_error)?;
    if tx
        .list_tables()
        .map_err(|error| format::storage_error(&error))?
        .next()
        .is_some()
        || tx
            .list_multimap_tables()
            .map_err(|error| format::storage_error(&error))?
            .next()
            .is_some()
    {
        return Err(RaftLogError::Conflict);
    }
    {
        let mut metadata = tx.open_table(format::META).map_err(format::table_error)?;
        let manifest = format::manifest(&store.group, &store.recovery)?;
        let state = format::state(
            RaftLogState::new(None, None, None),
            &store.group,
            &store.recovery,
        )?;
        metadata
            .insert(format::MANIFEST, manifest.as_slice())
            .map_err(|error| format::storage_error(&error))?;
        metadata
            .insert(format::STATE, state.as_slice())
            .map_err(|error| format::storage_error(&error))?;
        tx.open_table(format::LOGS).map_err(format::table_error)?;
    }
    commit(tx)
}

pub(super) fn commit(mut tx: WriteTransaction) -> Result<(), RaftLogError> {
    tx.set_durability(Durability::Immediate)
        .map_err(|_| RaftLogError::Unavailable)?;
    // Conservatively fence every failed commit, including documented poison rollback.
    tx.commit().map_err(|_| RaftLogError::Uncertain)
}
