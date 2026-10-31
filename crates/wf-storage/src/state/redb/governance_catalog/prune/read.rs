//! Exact member comparison under one existing transaction revision.
use super::super::super::admission::{storage_error, table_error, total};
use super::super::{E, RedbStateTransaction, read, validate_row};
use redb::ReadableTable;
use wf_domain::{
    governance::catalog::{CatalogPruneReply as P, CatalogPruneRequest as C},
    workflow::{control::ControlState, request::RequestScope},
};
pub(crate) fn decide(
    tx: &RedbStateTransaction<'_>,
    scope: &RequestScope,
    command: C<'_>,
    control: ControlState,
) -> Result<P, E> {
    tx.checked(|| {
        if scope.group() != tx.group || scope.recovery() != tx.recovery {
            return Err(E::NamespaceConflict);
        }
        command.validate().map_err(|_| E::InvalidChanges)?;
        tx.require_catalog_prune()?;
        let key = read::key(tx, scope, command.catalog, Some(command.member))?;
        tx.step()?;
        let table = tx.write.open_table(tx.rows_table).map_err(table_error)?;
        tx.step()?;
        let Some(value) = table.get(key.as_slice()).map_err(|e| storage_error(&e))? else {
            return Ok(P::Missing);
        };
        tx.recovery_row(&key, value.value())?;
        tx.admit_read(total(64, [value.value().len()])?)?;
        let actual = validate_row(value.value(), Some(control))?.ok_or(E::Corrupt)?;
        Ok(if actual == command.expected {
            P::Removed
        } else {
            P::Changed
        })
    })
}
