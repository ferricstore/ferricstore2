//! Two indexed immutable generations and one scalar durable custody intent.

use super::super::{admission, recovery, store::RedbStateTransaction};
use super::{KEY, SnapshotError, SnapshotLimits, validate};
use redb::ReadableTable;
use wf_domain::policy::complete::PolicySchemaCharge;
use wf_domain::workflow::control::{ControlState, EntryIdentity};
use wf_ports::{
    snapshot::SnapshotTransition,
    state::{GroupApplyTransaction, StateTransactionError as Error},
};

mod format;
pub(super) use format::decode;

#[derive(Clone, Copy)]
pub(in crate::state::redb) struct SnapshotCatalog {
    pub(in crate::state::redb) slots: [Option<([u8; 32], usize, usize)>; 2],
    pub(in crate::state::redb) current: Option<u8>,
    pub(in crate::state::redb) anchor: Option<u8>,
    pub(in crate::state::redb) pending: Option<SnapshotTransition>,
}

impl SnapshotCatalog {
    pub(super) fn check(self) -> Result<(), Error> {
        for selected in [self.current, self.anchor].into_iter().flatten() {
            if selected > 1 || self.slots[usize::from(selected)].is_none() {
                return Err(Error::Corrupt);
            }
        }
        for (i, item) in self.slots.iter().enumerate() {
            if item.is_some()
                && self.current != Some(u8::try_from(i).map_err(|_| Error::Corrupt)?)
                && self.anchor != Some(u8::try_from(i).map_err(|_| Error::Corrupt)?)
            {
                return Err(Error::Corrupt);
            }
        }
        if self.slots[0]
            .zip(self.slots[1])
            .is_some_and(|(a, b)| a.0 == b.0)
        {
            return Err(Error::Corrupt);
        }
        if self.anchor.is_some() && self.current.is_none() {
            return Err(Error::Corrupt);
        }
        if let Some(t) = self.pending
            && (self
                .current
                .and_then(|s| self.slots[usize::from(s)])
                .map(|f| f.0)
                != Some(*t.id())
                || (t.before().last_purged().is_some()
                    && self
                        .anchor
                        .and_then(|s| self.slots[usize::from(s)])
                        .map(|f| f.0)
                        != t.before().purged_snapshot_id()))
        {
            return Err(Error::Corrupt);
        }
        Ok(())
    }
    pub(super) fn find(self, id: [u8; 32]) -> Option<u8> {
        [0_u8, 1]
            .into_iter()
            .find(|&s| self.slots[usize::from(s)].is_some_and(|f| f.0 == id))
    }
    pub(super) fn destination(self) -> u8 {
        match self.anchor {
            Some(0) => 1,
            Some(_) => 0,
            None => self.current.unwrap_or(0),
        }
    }
}

pub(super) fn tables(
    slot: u8,
) -> [redb::TableDefinition<'static, &'static [u8], &'static [u8]>; 2] {
    if slot == 0 {
        [super::META, super::ROWS]
    } else {
        [super::ALT_META, super::ALT_ROWS]
    }
}
pub(super) fn select(tx: &mut RedbStateTransaction<'_>, slot: u8) {
    let [meta, rows] = tables(slot);
    tx.metadata_table = meta;
    tx.rows_table = rows;
    tx.snapshot_cohort = true;
}
pub(super) fn load(tx: &RedbStateTransaction<'_>) -> Result<SnapshotCatalog, Error> {
    tx.step()?;
    let table = tx
        .write
        .open_table(super::super::META)
        .map_err(admission::table_error)?;
    tx.step()?;
    let bytes = table
        .get(KEY)
        .map_err(|e| admission::storage_error(&e))?
        .ok_or(Error::Corrupt)?;
    tx.recovery_row(KEY, bytes.value())?;
    decode(tx, bytes.value())
}
pub(super) fn put(tx: &RedbStateTransaction<'_>, catalog: &SnapshotCatalog) -> Result<(), Error> {
    let mut bytes = [0_u8; 318];
    let n = format::encode(tx, catalog, &mut bytes)?;
    tx.temporary(n)?;
    tx.step()?;
    let mut table = tx
        .write
        .open_table(super::super::META)
        .map_err(admission::table_error)?;
    tx.step()?;
    tx.recovery_row(KEY, &bytes[..n])?;
    table
        .insert(KEY, &bytes[..n])
        .map_err(|e| admission::storage_error(&e))?;
    Ok(())
}
pub(super) fn retire(tx: &RedbStateTransaction<'_>, slot: u8) -> Result<(), Error> {
    for table in tables(slot) {
        super::retire(tx, table)?;
    }
    Ok(())
}

pub(super) fn retained(
    tx: &mut RedbStateTransaction<'_>,
    charge: &mut PolicySchemaCharge,
    limits: SnapshotLimits,
) -> Result<Option<([u8; 32], usize, usize)>, Error> {
    let catalog = load(tx)?;
    let live = tx.control()?;
    for slot in 0_u8..2 {
        select(tx, slot);
        let Some(expected) = catalog.slots[usize::from(slot)] else {
            validate::empty(tx)?;
            continue;
        };
        let (layout, saved) = recovery::cohort(tx, charge)?;
        if !matches!(layout, 3 | 5..=7) || super::format::facts(tx, limits)? != expected {
            return Err(Error::Corrupt);
        }
        compatible(tx, saved, live)?;
        if catalog.pending.is_some_and(|t| {
            catalog.current == Some(slot)
                && saved.last_entry().map(EntryIdentity::log) != Some(t.position())
        }) {
            return Err(Error::Corrupt);
        }
        let Some(t) = catalog.pending else {
            continue;
        };
        if catalog.anchor == Some(slot)
            && t.before().last_purged().is_some()
            && saved.last_entry().map(EntryIdentity::log) != t.before().last_purged()
        {
            return Err(Error::Corrupt);
        }
        if catalog.current == Some(slot) && t.is_received() && saved != live {
            return Err(Error::Corrupt);
        }
    }
    super::select(tx, false);
    Ok(catalog.current.and_then(|s| catalog.slots[usize::from(s)]))
}

fn compatible(
    tx: &RedbStateTransaction<'_>,
    saved: ControlState,
    live: ControlState,
) -> Result<(), Error> {
    tx.step()?;
    let old = tx
        .write
        .open_table(tx.metadata_table)
        .map_err(admission::table_error)?;
    tx.step()?;
    let new = tx
        .write
        .open_table(super::super::META)
        .map_err(admission::table_error)?;
    let old_manifest = validate::get(tx, &old, b"manifest")?;
    let new_manifest = validate::get(tx, &new, b"manifest")?;
    let old_origin = super::policy::origin(tx, &old, old_manifest.value())?;
    let new_origin = super::policy::origin(tx, &new, new_manifest.value())?;
    let old_member = validate::get(tx, &old, super::super::membership::KEY)?;
    let new_member = validate::get(tx, &new, super::super::membership::KEY)?;
    validate::value_defaults_compatible(tx, old_manifest.value(), new_manifest.value())
        .map_err(|_| Error::Corrupt)?;
    validate::values(
        tx,
        (saved, live),
        (
            old_origin
                .as_ref()
                .map_or(old_manifest.value(), redb::AccessGuard::value),
            new_origin
                .as_ref()
                .map_or(new_manifest.value(), redb::AccessGuard::value),
        ),
        (old_member.value(), new_member.value()),
    )
    .map_err(|_| Error::Corrupt)
}

pub(super) fn require(
    tx: &mut RedbStateTransaction<'_>,
    catalog: &SnapshotCatalog,
    t: SnapshotTransition,
    limits: SnapshotLimits,
) -> Result<u8, SnapshotError> {
    let slot = catalog
        .find(*t.id())
        .filter(|slot| Some(*slot) == catalog.current)
        .ok_or(SnapshotError::TransitionConflict)?;
    select(tx, slot);
    let actual = tx.control().map_err(SnapshotError::State)?;
    if actual.last_entry().map(EntryIdentity::log) != Some(t.position()) {
        return Err(SnapshotError::TransitionConflict);
    }
    let facts = super::format::facts(tx, limits).map_err(SnapshotError::State)?;
    if Some(facts) != catalog.slots[usize::from(slot)] {
        return Err(SnapshotError::TransitionConflict);
    }
    super::select(tx, false);
    Ok(slot)
}
