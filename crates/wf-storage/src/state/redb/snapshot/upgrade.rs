//! Consuming explicit upgrade; immutable canonical DATA is never rewritten.

use super::super::store::RedbStateStore;
use super::{SnapshotError, SnapshotLimits, active, admit, catalog, format, validate};
use wf_domain::policy::complete::PolicySchemaCharge;

impl RedbStateStore {
    /// Explicitly enables two-generation snapshot custody on a validated owner.
    /// # Errors
    /// Consumes the owner on profile, active storage or uncertain commit failure.
    pub fn upgrade_snapshot_layout(self, limits: SnapshotLimits) -> Result<Self, SnapshotError> {
        self.ready().map_err(SnapshotError::State)?;
        admit(&self.group, &self.recovery, self.limits, limits, 0)?;
        active(&self, upgrade(&self, limits))?;
        if self.layout.get() < 5 {
            self.layout.set(4);
        }
        Ok(self)
    }
}

fn upgrade(store: &RedbStateStore, limits: SnapshotLimits) -> Result<(), SnapshotError> {
    let mut tx = super::transaction(store, limits, store.limits).map_err(SnapshotError::State)?;
    let mut charge = PolicySchemaCharge::new(store.limits.schema());
    let facts = validate::complete(&mut tx, &mut charge, store.limits.snapshot_limits())
        .map_err(SnapshotError::State)?;
    if matches!(store.layout.get(), 4..=7) {
        return Ok(());
    }
    let pin = validate::pin(&tx).map_err(SnapshotError::State)?;
    for table in [super::ALT_META, super::ALT_ROWS] {
        tx.step().map_err(SnapshotError::State)?;
        drop(
            tx.write
                .open_table(table)
                .map_err(super::admission::table_error)
                .map_err(SnapshotError::State)?,
        );
    }
    format::set_manifest(&tx, 4).map_err(SnapshotError::State)?;
    catalog::put(
        &tx,
        &catalog::SnapshotCatalog {
            slots: [facts, None],
            current: facts.map(|_| 0),
            anchor: pin.map(|_| 0),
            pending: None,
        },
    )
    .map_err(SnapshotError::State)?;
    validate::complete(&mut tx, &mut charge, store.limits.snapshot_limits())
        .map_err(SnapshotError::State)?;
    super::commit(tx).map_err(SnapshotError::State)
}
