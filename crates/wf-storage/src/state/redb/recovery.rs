//! Exclusive noncreating recovery with a cumulative census and shared grammar.

use super::{
    META, ROWS,
    admission::{storage_error, table_error, total, value_error},
    bootstrap::hash_row,
    store::{RedbStateStore, RedbStateTransaction},
};
use crate::state::{key, value};
use redb::{ReadableTable, TableHandle};
use sha2::{Digest, Sha256};
use wf_domain::policy::complete::PolicySchemaCharge;
use wf_ports::state::{GroupApplyTransaction, StateTransactionError};
use wf_types::version::{CommittedTime, HlcTimestamp};

mod index;
mod rows;

pub(super) fn validate(store: &RedbStateStore) -> Result<(), StateTransactionError> {
    let tx = store.transaction(
        true,
        store.limits.admission(),
        store.limits.maximum_recovery_steps(),
        usize::MAX,
        store.limits.maximum_temporary_bytes(),
    )?;
    inventory(&tx)?;
    tx.step()?;
    let metadata = tx.write.open_table(META).map_err(table_error)?;
    tx.step()?;
    let mut meta_rows = metadata.iter().map_err(|e| storage_error(&e))?;
    tx.step()?;
    let (meta_key, manifest) = meta_rows
        .next()
        .transpose()
        .map_err(|e| storage_error(&e))?
        .ok_or(StateTransactionError::Corrupt)?;
    tx.recovery_row(meta_key.value(), manifest.value())?;
    if meta_key.value() != b"manifest" {
        return Err(StateTransactionError::Corrupt);
    }
    tx.step()?;
    if let Some(row) = meta_rows.next() {
        let (key, value) = row.map_err(|e| storage_error(&e))?;
        tx.recovery_row(key.value(), value.value())?;
        return Err(StateTransactionError::Corrupt);
    }
    let (initial, placements, policies, expected) = check_manifest(manifest.value(), &tx)?;
    let control = tx.control()?;
    if control.last_entry().is_none() && control.time() != initial {
        return Err(StateTransactionError::Corrupt);
    }
    if control.time().hlc() < initial.hlc() || control.time().wall() < initial.wall() {
        return Err(StateTransactionError::Corrupt);
    }
    let (actual_placements, actual_policies, hash) =
        validate_rows(&tx, control.last_entry().is_some())?;
    if placements != actual_placements || policies != actual_policies || expected != hash {
        return Err(StateTransactionError::Corrupt);
    }
    Ok(())
}

fn inventory(tx: &RedbStateTransaction<'_>) -> Result<(), StateTransactionError> {
    tx.step()?;
    let mut tables = tx.write.list_tables().map_err(|e| storage_error(&e))?;
    let mut count = 0;
    loop {
        tx.step()?;
        let Some(table) = tables.next() else {
            break;
        };
        if !matches!(table.name(), "wf_state_meta_v1" | "wf_state_rows_v1") {
            return Err(StateTransactionError::Corrupt);
        }
        count += 1;
    }
    tx.step()?;
    let mut multimaps = tx
        .write
        .list_multimap_tables()
        .map_err(|e| storage_error(&e))?;
    tx.step()?;
    if count != 2 || multimaps.next().is_some() {
        return Err(StateTransactionError::Corrupt);
    }
    Ok(())
}

fn check_manifest(
    bytes: &[u8],
    tx: &RedbStateTransaction<'_>,
) -> Result<(CommittedTime, u64, u64, [u8; 32]), StateTransactionError> {
    let mut at = value::start(bytes, *b"FWSS", bytes.len()).map_err(value_error)?;
    let n = usize::from(value::read_u16(bytes, &mut at).map_err(value_error)?);
    let group = value::take(bytes, &mut at, n).map_err(value_error)?;
    let n = usize::from(value::read_u16(bytes, &mut at).map_err(value_error)?);
    let recovery = value::take(bytes, &mut at, n).map_err(value_error)?;
    let physical = value::read_u64(bytes, &mut at).map_err(value_error)?;
    let logical = value::read_u16(bytes, &mut at).map_err(value_error)?;
    let wall = value::read_u64(bytes, &mut at).map_err(value_error)?;
    let initial = CommittedTime::new(
        HlcTimestamp::new(physical, logical).map_err(|_| StateTransactionError::Corrupt)?,
        wall,
    )
    .map_err(|_| StateTransactionError::Corrupt)?;
    let placements = value::read_u64(bytes, &mut at).map_err(value_error)?;
    let policies = value::read_u64(bytes, &mut at).map_err(value_error)?;
    let hash: [u8; 32] = value::take(bytes, &mut at, 32)
        .map_err(value_error)?
        .try_into()
        .map_err(|_| StateTransactionError::Corrupt)?;
    let checksum = Sha256::digest(&bytes[..at]);
    if value::take(bytes, &mut at, 32).map_err(value_error)? != checksum.as_slice() {
        return Err(StateTransactionError::Corrupt);
    }
    value::finish(bytes, at).map_err(value_error)?;
    if group.is_empty() || group.len() > 256 || recovery.is_empty() || recovery.len() > 256 {
        return Err(StateTransactionError::Corrupt);
    }
    if group != tx.group.as_bytes() || recovery != tx.recovery.as_bytes() {
        return Err(StateTransactionError::NamespaceConflict);
    }
    Ok((initial, placements, policies, hash))
}

fn validate_rows(
    tx: &RedbStateTransaction<'_>,
    applied: bool,
) -> Result<(u64, u64, [u8; 32]), StateTransactionError> {
    tx.step()?;
    let table = tx.write.open_table(ROWS).map_err(table_error)?;
    tx.step()?;
    let mut entries = table.iter().map_err(|e| storage_error(&e))?;
    let mut counts = [0_u64; 2];
    let mut hash = Sha256::new();
    hash.update(b"wf-state-genesis-v1");
    let mut charge = PolicySchemaCharge::new(tx.limits.schema());
    loop {
        tx.step()?;
        let Some(row) = entries.next() else {
            break;
        };
        let (physical, value) = row.map_err(|e| storage_error(&e))?;
        tx.recovery_row(physical.value(), value.value())?;
        let maximum = total(256, [physical.value().len()])?;
        tx.temporary(maximum)?;
        let decoded = key::decode_state_key(
            physical.value(),
            tx.group,
            tx.recovery,
            physical.value().len(),
            maximum,
        )
        .map_err(value_error)?;
        if !applied && matches!(decoded.family(), 1 | 2 | 5) {
            return Err(StateTransactionError::Corrupt);
        }
        rows::validate(tx, &table, &decoded, value.value(), &mut charge)?;
        if matches!(decoded.family(), 6 | 7) {
            counts[usize::from(decoded.family() - 6)] = counts[usize::from(decoded.family() - 6)]
                .checked_add(1)
                .ok_or(StateTransactionError::Limit)?;
            hash_row(&mut hash, physical.value(), value.value())?;
        }
    }
    Ok((counts[1], counts[0], hash.finalize().into()))
}
