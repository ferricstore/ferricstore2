//! Private disk custody of a completely validated incoming canonical image.

use super::super::{StateStoreLimits, store::RedbStateStore};
use super::{
    SnapshotError, SnapshotLimits, SnapshotMetadata, admission, read, receive, select, validate,
};
use std::{fs::OpenOptions, path::Path};
use wf_domain::policy::complete::PolicySchemaCharge;
use wf_ports::state::StateTransactionError;
use wf_types::identity::{GroupId, RecoveryId};

/// A consumed database with checked immutable data and no public apply access.
/// It retains no typed metadata owner and exposes no raw database handle.
pub struct RedbSnapshotStage {
    pub(super) store: RedbStateStore,
    facts: ([u8; 32], usize, usize),
}

pub(super) fn source(error: SnapshotError) -> SnapshotError {
    match error {
        SnapshotError::State(error) => SnapshotError::Stage(error),
        other => other,
    }
}

impl RedbSnapshotStage {
    /// Consumes a genuinely empty database. Caller owns file/cache/dir durability.
    /// # Errors
    /// Refuses framing, resources, existing inventory or candidate storage errors.
    pub fn receive_database(
        database: redb::Database,
        group: GroupId,
        recovery: RecoveryId,
        input: &mut impl std::io::Read,
        target: StateStoreLimits,
        limits: SnapshotLimits,
    ) -> Result<Self, SnapshotError> {
        super::admit(&group, &recovery, target, limits, 0)?;
        let store = RedbStateStore::unvalidated(database, group, recovery, target);
        let facts = receive::receive(&store, input, limits)?;
        Ok(Self { store, facts })
    }

    /// Exclusively creates a new stage file without creating its parent directory.
    /// A failed receive may leave an unserved file for caller cleanup.
    /// # Errors
    /// Refuses existing paths, bad input, limits or unproved file/dir durability.
    pub fn receive(
        path: &Path,
        group: GroupId,
        recovery: RecoveryId,
        input: &mut impl std::io::Read,
        target: StateStoreLimits,
        limits: SnapshotLimits,
    ) -> Result<Self, SnapshotError> {
        super::admit(&group, &recovery, target, limits, 0)?;
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(path)
            .map_err(|_| SnapshotError::Stage(StateTransactionError::Unavailable))?;
        let backend = redb::backends::FileBackend::new(file)
            .map_err(admission::database_error)
            .map_err(SnapshotError::Stage)?;
        let database = redb::Builder::new()
            .set_cache_size(16_777_216)
            .create_with_backend(backend)
            .map_err(admission::database_error)
            .map_err(SnapshotError::Stage)?;
        let stage = Self::receive_database(database, group, recovery, input, target, limits)?;
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        std::fs::File::open(parent)
            .and_then(|file| file.sync_all())
            .map_err(|_| SnapshotError::Stage(StateTransactionError::Uncertain))?;
        Ok(stage)
    }

    /// Materializes one admitted observation from the original frozen authority.
    /// # Errors
    /// Refuses resource exhaustion or actual stage-source storage failure.
    pub fn metadata(&mut self, limits: SnapshotLimits) -> Result<SnapshotMetadata, SnapshotError> {
        self.store.ready().map_err(SnapshotError::Stage)?;
        super::admit(
            &self.store.group,
            &self.store.recovery,
            self.store.limits,
            limits,
            0,
        )?;
        let result = self.observe(limits).map_err(source);
        if let Err(SnapshotError::Stage(error)) = result {
            self.store.failed(error);
        }
        result
    }

    fn observe(&self, limits: SnapshotLimits) -> Result<SnapshotMetadata, SnapshotError> {
        let mut tx = super::transaction(&self.store, limits, self.store.limits)
            .map_err(SnapshotError::Stage)?;
        let mut charge = PolicySchemaCharge::new(self.store.limits.schema());
        let facts = validate::complete(&mut tx, &mut charge, limits)
            .map_err(SnapshotError::Stage)?
            .ok_or(SnapshotError::Stage(StateTransactionError::Corrupt))?;
        if facts != self.facts {
            return Err(SnapshotError::Stage(StateTransactionError::Corrupt));
        }
        select(&mut tx, true);
        read::metadata(&tx, facts)
    }

    /// Moves the already validated fresh restore, without a second authority copy.
    /// # Errors
    /// Refuses a stage whose actual storage handle is fatally fenced.
    pub fn into_store(self) -> Result<RedbStateStore, SnapshotError> {
        self.store.ready().map_err(SnapshotError::Stage)?;
        Ok(self.store)
    }
}
