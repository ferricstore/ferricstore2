//! Existing exact immutable/row writes, without additional stored-value owners.

use redb::ReadableTable;
use wf_domain::workflow::request::RequestScope;
use wf_ports::state::StateTransactionError as Error;

use super::{RedbStateTransaction, ValueCharge, admission, product, scope_bytes, total};

pub(super) fn immutable_value(
    tx: &RedbStateTransaction<'_>,
    key: &[u8],
    parts: [&[u8]; 2],
    charge: &mut ValueCharge,
    live: usize,
) -> Result<(), Error> {
    charge.candidate()?;
    tx.step()?;
    let table = tx
        .write
        .open_table(tx.rows_table)
        .map_err(admission::table_error)?;
    tx.step()?;
    let previous = table.get(key).map_err(|e| admission::storage_error(&e))?;
    let size = total(parts[0].len(), [parts[1].len()])?;
    if let Some(previous) = previous {
        charge.compare(previous.value().len(), size)?;
        if previous.value().len() != size
            || !previous.value().starts_with(parts[0])
            || &previous.value()[parts[0].len()..] != parts[1]
        {
            return Err(Error::Corrupt);
        }
        return Ok(());
    }
    drop(previous);
    drop(table);
    if parts[0].is_empty() {
        return put(tx, key, parts[1], charge, live);
    }
    charge.work(size)?;
    charge.temporary(total(live, [size])?)?;
    let mut stored = Vec::new();
    stored
        .try_reserve_exact(size)
        .map_err(|_| Error::Allocation)?;
    stored.extend_from_slice(parts[0]);
    stored.extend_from_slice(parts[1]);
    put(tx, key, &stored, charge, total(live, [stored.len()])?)
}

pub(super) fn write_exact(
    tx: &RedbStateTransaction<'_>,
    key: &[u8],
    bytes: &[u8],
    charge: &mut ValueCharge,
    live: usize,
) -> Result<(), Error> {
    charge.candidate()?;
    tx.step()?;
    let table = tx
        .write
        .open_table(tx.rows_table)
        .map_err(admission::table_error)?;
    tx.step()?;
    if let Some(previous) = table.get(key).map_err(|e| admission::storage_error(&e))? {
        charge.compare(previous.value().len(), bytes.len())?;
        if previous.value() != bytes {
            return Err(Error::Corrupt);
        }
        return Ok(());
    }
    drop(table);
    put(tx, key, bytes, charge, live)
}

pub(super) fn put(
    tx: &RedbStateTransaction<'_>,
    key: &[u8],
    bytes: &[u8],
    charge: &mut ValueCharge,
    live: usize,
) -> Result<(), Error> {
    charge.output(1, total(key.len(), [bytes.len()])?)?;
    charge.work(total(key.len(), [bytes.len()])?)?;
    charge.temporary(total(live, [key.len(), bytes.len()])?)?;
    tx.temporary(total(key.len(), [bytes.len()])?)?;
    admission::engine_pair(key.len(), bytes.len())?;
    tx.step()?;
    let mut table = tx
        .write
        .open_table(tx.rows_table)
        .map_err(admission::table_error)?;
    tx.step()?;
    table
        .insert(key, bytes)
        .map_err(|e| admission::storage_error(&e))?;
    Ok(())
}

pub(super) fn key_bound(
    scope: &RequestScope,
    reference: usize,
    scalar: usize,
) -> Result<usize, Error> {
    total(
        32,
        [product(total(scope_bytes(scope), [reference])?, 2)?, scalar],
    )
}
