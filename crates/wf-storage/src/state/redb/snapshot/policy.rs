//! Policy-aware birth/current metadata and canonical snapshot format dispatch.

use super::super::{
    admission::{storage_error, table_error},
    store::RedbStateTransaction,
};
use super::{catalog, format, validate};
use redb::ReadableTable;
use wf_ports::state::StateTransactionError as Error;

/// Scalar descriptor carry only; frozen DATA was validated by the strict owner.
pub(in crate::state::redb) fn publication_catalog(
    tx: &RedbStateTransaction<'_>,
) -> Result<catalog::SnapshotCatalog, Error> {
    tx.step()?;
    let table = tx
        .write
        .open_table(super::super::META)
        .map_err(table_error)?;
    tx.step()?;
    let bytes = table
        .get(super::KEY)
        .map_err(|e| storage_error(&e))?
        .ok_or(Error::Corrupt)?;
    tx.recovery_row(super::KEY, bytes.value())?;
    if bytes.value().starts_with(b"FWSD\0\x03") {
        return catalog::decode(tx, bytes.value());
    }
    let facts = format::decode_descriptor(tx, bytes.value())?;
    Ok(catalog::SnapshotCatalog {
        slots: [facts, None],
        current: facts.map(|_| 0),
        anchor: if bytes.value().get(5) == Some(&2) {
            facts.map(|_| 0)
        } else {
            None
        },
        pending: None,
    })
}

pub(in crate::state::redb) fn require_served(tx: &RedbStateTransaction<'_>) -> Result<(), Error> {
    if publication_catalog(tx)?
        .pending
        .is_some_and(wf_ports::snapshot::SnapshotTransition::is_received)
    {
        return Err(Error::InvalidChanges);
    }
    Ok(())
}

pub(in crate::state::redb) fn upgrade_catalog(
    tx: &RedbStateTransaction<'_>,
    saved: &catalog::SnapshotCatalog,
) -> Result<(), Error> {
    if saved.pending.is_some() {
        return Err(Error::InvalidChanges);
    }
    for definition in [super::ALT_META, super::ALT_ROWS] {
        tx.step()?;
        drop(tx.write.open_table(definition).map_err(table_error)?);
    }
    catalog::put(tx, saved)
}
/// Borrows the actual immutable canonical origin when a manifest is current5.
pub(super) fn origin<'a>(
    tx: &RedbStateTransaction<'_>,
    table: &'a impl ReadableTable<&'static [u8], &'static [u8]>,
    manifest: &[u8],
) -> Result<Option<redb::AccessGuard<'a, &'static [u8]>>, Error> {
    if !matches!(manifest.get(4..6), Some([0, 5..=7])) {
        return Ok(None);
    }
    validate::get(tx, table, b"origin").map(Some)
}
