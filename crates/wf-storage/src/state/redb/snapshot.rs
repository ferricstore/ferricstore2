//! Serialized immutable cohorts and private validated restore custody.

use super::{
    StateStoreLimits, admission,
    store::{RedbStateStore, RedbStateTransaction},
};
use redb::{ReadableTable, TableHandle};
use wf_ports::snapshot::{SnapshotError, SnapshotLimits, SnapshotMetadata, StateSnapshotStore};
use wf_ports::state::StateTransactionError;
use wf_types::identity::{GroupId, RecoveryId};

mod capture;
mod catalog;
mod format;
mod install;
mod io;
mod read;
mod receive;
mod stage;
mod transition;
mod upgrade;
pub(super) mod validate;
pub use stage::RedbSnapshotStage;

pub(super) const META: redb::TableDefinition<'static, &[u8], &[u8]> =
    redb::TableDefinition::new("wf_state_snapshot_meta_v1");
pub(super) const ROWS: redb::TableDefinition<'static, &[u8], &[u8]> =
    redb::TableDefinition::new("wf_state_snapshot_rows_v1");
pub(super) const ALT_META: redb::TableDefinition<'static, &[u8], &[u8]> =
    redb::TableDefinition::new("wf_state_snapshot_alt_meta_v1");
pub(super) const ALT_ROWS: redb::TableDefinition<'static, &[u8], &[u8]> =
    redb::TableDefinition::new("wf_state_snapshot_alt_rows_v1");
pub(super) const KEY: &[u8] = b"snapshot";

pub(super) fn check_descriptor(
    tx: &RedbStateTransaction<'_>,
    bytes: &[u8],
) -> Result<(), StateTransactionError> {
    if bytes.starts_with(b"FWSD\0\x03") {
        catalog::decode(tx, bytes).map(|_| ())
    } else {
        format::decode_descriptor(tx, bytes).map(|_| ())
    }
}

pub(super) fn put_descriptor(
    tx: &RedbStateTransaction<'_>,
    facts: Option<([u8; 32], usize, usize)>,
) -> Result<(), StateTransactionError> {
    write_descriptor(tx, facts, false)
}

pub(super) fn pin_descriptor(
    tx: &RedbStateTransaction<'_>,
    facts: ([u8; 32], usize, usize),
) -> Result<(), StateTransactionError> {
    write_descriptor(tx, Some(facts), true)
}

fn write_descriptor(
    tx: &RedbStateTransaction<'_>,
    facts: Option<([u8; 32], usize, usize)>,
    pinned: bool,
) -> Result<(), StateTransactionError> {
    let mut bytes = [0_u8; 87];
    let n = format::descriptor(tx, facts, pinned, &mut bytes)?;
    tx.temporary(n)?;
    tx.step()?;
    let mut table = tx
        .write
        .open_table(super::META)
        .map_err(admission::table_error)?;
    tx.step()?;
    tx.recovery_row(KEY, &bytes[..n])?;
    table
        .insert(KEY, &bytes[..n])
        .map_err(|e| admission::storage_error(&e))?;
    Ok(())
}

pub(super) fn initialize(tx: &RedbStateTransaction<'_>) -> Result<(), StateTransactionError> {
    for definition in [META, ROWS] {
        tx.step()?;
        drop(
            tx.write
                .open_table(definition)
                .map_err(admission::table_error)?,
        );
    }
    put_descriptor(tx, None)
}

pub(super) fn admit(
    group: &GroupId,
    recovery: &RecoveryId,
    target: StateStoreLimits,
    limits: SnapshotLimits,
    extra: usize,
) -> Result<(), SnapshotError> {
    let supported = target.snapshot_limits();
    if limits.maximum_rows() > supported.maximum_rows()
        || limits.maximum_encoded_bytes() > supported.maximum_encoded_bytes()
        || limits.maximum_row_bytes() > supported.maximum_row_bytes()
    {
        return Err(SnapshotError::ProfileMismatch);
    }
    let bytes = admission::total(
        2560,
        [
            group.as_bytes().len(),
            recovery.as_bytes().len(),
            extra,
            limits.maximum_metadata_bytes(),
            limits.maximum_temporary_bytes(),
        ],
    )
    .map_err(|_| SnapshotError::Limit)?;
    if bytes > limits.workspace_bytes() {
        return Err(SnapshotError::Limit);
    }
    Ok(())
}

pub(super) fn transaction(
    store: &RedbStateStore,
    limits: SnapshotLimits,
    target: StateStoreLimits,
) -> Result<RedbStateTransaction<'_>, StateTransactionError> {
    let mut tx = store.transaction(
        true,
        target.admission(),
        limits.maximum_steps(),
        limits.maximum_metadata_bytes(),
        limits
            .maximum_temporary_bytes()
            .min(target.maximum_temporary_bytes()),
    )?;
    tx.limits = target;
    tx.maximum_inspected = limits.maximum_visited_rows();
    tx.maximum_wire = limits.maximum_visited_bytes();
    Ok(tx)
}

pub(super) fn select(tx: &mut RedbStateTransaction<'_>, frozen: bool) {
    tx.snapshot_cohort = frozen;
    tx.metadata_table = if frozen { META } else { super::META };
    tx.rows_table = if frozen { ROWS } else { super::ROWS };
}

pub(super) fn commit(mut tx: RedbStateTransaction<'_>) -> Result<(), StateTransactionError> {
    tx.write
        .set_durability(redb::Durability::Immediate)
        .map_err(|_| StateTransactionError::Unavailable)?;
    tx.step()?;
    let fenced = tx.fenced;
    tx.write.commit().map_err(|_| {
        super::store::fence(fenced, StateTransactionError::Uncertain);
        StateTransactionError::Uncertain
    })
}

pub(super) fn retire(
    tx: &RedbStateTransaction<'_>,
    definition: redb::TableDefinition<'static, &'static [u8], &'static [u8]>,
) -> Result<(), StateTransactionError> {
    tx.step()?;
    {
        let table = tx
            .write
            .open_table(definition)
            .map_err(admission::table_error)?;
        tx.step()?;
        let mut entries = table.iter().map_err(|e| admission::storage_error(&e))?;
        loop {
            tx.step()?;
            let Some(entry) = entries.next() else {
                break;
            };
            let (key, value) = entry.map_err(|e| admission::storage_error(&e))?;
            tx.recovery_row(key.value(), value.value())?;
        }
    }
    tx.step()?;
    tx.write
        .delete_table(definition)
        .map_err(admission::table_error)?;
    tx.step()?;
    drop(
        tx.write
            .open_table(definition)
            .map_err(admission::table_error)?,
    );
    Ok(())
}

pub(super) fn copy(
    source: &RedbStateTransaction<'_>,
    target: &RedbStateTransaction<'_>,
    from: redb::TableDefinition<'static, &'static [u8], &'static [u8]>,
    to: redb::TableDefinition<'static, &'static [u8], &'static [u8]>,
) -> Result<(), SnapshotError> {
    source.carry_from(target);
    source.step().map_err(SnapshotError::Stage)?;
    let input = source
        .write
        .open_table(from)
        .map_err(admission::table_error)
        .map_err(SnapshotError::Stage)?;
    target.carry_from(source);
    target.step().map_err(SnapshotError::State)?;
    let mut output = target
        .write
        .open_table(to)
        .map_err(admission::table_error)
        .map_err(SnapshotError::State)?;
    source.carry_from(target);
    source.step().map_err(SnapshotError::Stage)?;
    let mut entries = input
        .iter()
        .map_err(|e| SnapshotError::Stage(admission::storage_error(&e)))?;
    loop {
        source.step().map_err(SnapshotError::Stage)?;
        let Some(row) = entries.next() else {
            break;
        };
        let (key, value) = row.map_err(|e| SnapshotError::Stage(admission::storage_error(&e)))?;
        source
            .recovery_row(key.value(), value.value())
            .map_err(SnapshotError::Stage)?;
        if from.name() == super::META.name()
            && matches!(
                to.name(),
                "wf_state_snapshot_meta_v1" | "wf_state_snapshot_alt_meta_v1"
            )
            && key.value() == KEY
        {
            continue;
        }
        admission::engine_pair(key.value().len(), value.value().len())
            .map_err(SnapshotError::Stage)?;
        target.carry_from(source);
        target.step().map_err(SnapshotError::State)?;
        target
            .recovery_row(key.value(), value.value())
            .map_err(SnapshotError::State)?;
        let normalized = if key.value() == b"manifest" && to.name() != super::META.name() {
            if value.value().get(4..6) == Some([0, 3].as_slice()) {
                None
            } else {
                Some(format::manifest(target, value.value(), 3).map_err(SnapshotError::State)?)
            }
        } else {
            None
        };
        output
            .insert(key.value(), normalized.as_deref().unwrap_or(value.value()))
            .map_err(|e| SnapshotError::State(admission::storage_error(&e)))?;
        source.carry_from(target);
    }
    target.carry_from(source);
    Ok(())
}

fn active<T>(store: &RedbStateStore, result: Result<T, SnapshotError>) -> Result<T, SnapshotError> {
    result.inspect_err(|error| {
        if let SnapshotError::State(error) = error {
            store.failed(*error);
        }
    })
}

impl StateSnapshotStore for RedbStateStore {
    type Stage = RedbSnapshotStage;
    fn capture_snapshot<W: std::io::Write>(
        &mut self,
        output: &mut W,
        limits: SnapshotLimits,
    ) -> Result<SnapshotMetadata, SnapshotError> {
        self.ready().map_err(SnapshotError::State)?;
        admit(&self.group, &self.recovery, self.limits, limits, 0)?;
        active(self, capture::capture(self, output, limits))
    }
    fn current_snapshot<W: std::io::Write>(
        &mut self,
        output: &mut W,
        limits: SnapshotLimits,
    ) -> Result<Option<SnapshotMetadata>, SnapshotError> {
        self.ready().map_err(SnapshotError::State)?;
        admit(&self.group, &self.recovery, self.limits, limits, 0)?;
        active(self, read::current(self, output, limits))
    }
    fn install_snapshot(
        &mut self,
        stage: Self::Stage,
        limits: SnapshotLimits,
    ) -> Result<SnapshotMetadata, SnapshotError> {
        self.ready().map_err(SnapshotError::State)?;
        let extra = admission::total(
            512,
            [
                stage.store.group.as_bytes().len(),
                stage.store.recovery.as_bytes().len(),
            ],
        )
        .map_err(|_| SnapshotError::Limit)?;
        admit(&self.group, &self.recovery, self.limits, limits, extra)?;
        active(self, install::install(self, stage, limits))
    }
    fn pin_snapshot(
        &mut self,
        expected_id: [u8; 32],
        limits: SnapshotLimits,
    ) -> Result<(), SnapshotError> {
        self.ready().map_err(SnapshotError::State)?;
        admit(&self.group, &self.recovery, self.limits, limits, 0)?;
        active(self, read::pin(self, Some(expected_id), limits)).map(|_| ())
    }
    fn snapshot_pin(&mut self, limits: SnapshotLimits) -> Result<Option<[u8; 32]>, SnapshotError> {
        self.ready().map_err(SnapshotError::State)?;
        admit(&self.group, &self.recovery, self.limits, limits, 0)?;
        active(self, read::pin(self, None, limits))
    }
}
