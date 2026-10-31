//! Same-guard hint inspection; invalid payloads never cause implicit repair.
use super::super::super::admission::{copy_bytes, storage_error, table_error, total};
use super::{E, RedbStateTransaction};
use crate::state::key;
use redb::ReadableTable;
use wf_domain::workflow::request::RequestScope;
use wf_ports::state::{GovernanceCatalogProgress as P, inspect_catalog_progress_cursor};

pub(super) fn key(
    tx: &RedbStateTransaction<'_>,
    scope: &RequestScope,
    catalog: &[u8],
) -> Result<Vec<u8>, E> {
    if scope.group() != tx.group || scope.recovery() != tx.recovery {
        return Err(E::NamespaceConflict);
    }
    if catalog.len() > 65_535 {
        return Err(E::InvalidChanges);
    }
    tx.key(
        &[
            scope.group().as_bytes(),
            scope.recovery().as_bytes(),
            scope.tenant().as_bytes(),
            catalog,
        ],
        0,
        |cap| key::governance_catalog::progress(scope, catalog, cap),
    )
}
pub(super) fn read(
    tx: &RedbStateTransaction<'_>,
    scope: &RequestScope,
    catalog: &[u8],
) -> Result<P, E> {
    if scope.group() != tx.group || scope.recovery() != tx.recovery {
        return Err(E::NamespaceConflict);
    }
    tx.require_catalog_progress()?;
    let key = key(tx, scope, catalog)?;
    tx.step()?;
    let table = tx.write.open_table(tx.rows_table).map_err(table_error)?;
    tx.step()?;
    let Some(value) = table.get(key.as_slice()).map_err(|e| storage_error(&e))? else {
        return Ok(P::Missing);
    };
    tx.recovery_row(&key, value.value())?;
    tx.admit_read(total(128, [value.value().len()])?)?;
    let Some(cursor) = inspect_catalog_progress_cursor(value.value()) else {
        return Ok(P::Invalid);
    };
    tx.admit_read(total(24, [cursor.len()])?)?;
    Ok(P::Cursor(copy_bytes(cursor)?))
}
