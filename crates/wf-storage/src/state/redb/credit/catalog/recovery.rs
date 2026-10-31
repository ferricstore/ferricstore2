//! Complete queue range validation reuses the actual open recovery table.

use super::{catalog_key, read, read_meta};
use crate::state::redb::admission::value_error;
use crate::state::{key::DecodedStateKey, redb::store::RedbStateTransaction, value};
use redb::ReadableTable;
use wf_domain::workflow::request::RequestScope;
use wf_ports::state::StateTransactionError as E;

pub(in crate::state::redb) fn row(
    tx: &RedbStateTransaction<'_>,
    table: &impl ReadableTable<&'static [u8], &'static [u8]>,
    key: &DecodedStateKey,
    bytes: &[u8],
) -> Result<(), E> {
    let scope = tx.scope_bytes(key.part(2))?;
    if key.family() == 16 {
        let meta = value::catalog::meta(bytes).map_err(value_error)?;
        if meta.tail() + 1 - meta.head() > 65_536 {
            return Err(E::Corrupt);
        }
        for sequence in meta.head()..=meta.tail() {
            let key = catalog_key(tx, &scope, Some(sequence))?;
            read(tx, table, &key, |bytes| {
                value::catalog::intent(bytes)
                    .map(|_| ())
                    .map_err(value_error)
            })?
            .ok_or(E::Corrupt)?;
        }
    } else {
        value::catalog::intent(bytes).map_err(value_error)?;
        let meta = read_meta(tx, table, &scope)?.ok_or(E::Corrupt)?;
        let sequence = u64::try_from(key.number()).map_err(|_| E::Corrupt)?;
        if sequence < meta.head() || sequence > meta.tail() {
            return Err(E::Corrupt);
        }
    }
    Ok(())
}

pub(in crate::state::redb) fn require_meta(
    tx: &RedbStateTransaction<'_>,
    table: &impl ReadableTable<&'static [u8], &'static [u8]>,
    scope: &RequestScope,
) -> Result<(), E> {
    read_meta(tx, table, scope)?.ok_or(E::Corrupt).map(|_| ())
}
