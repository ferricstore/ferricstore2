//! Complete bounded namespace validation; no entry body is materialized.

use redb::{ReadableTable, TableHandle};
use wf_ports::log::{RaftLogError, RaftLogState};
use wf_types::version::LogIdentity;

use super::{format, read, store::RedbLogStore};

pub(super) fn census(
    store: &RedbLogStore,
    count: &mut usize,
    bytes: &mut usize,
    wire: usize,
) -> Result<(), RaftLogError> {
    let next_count = format::sum(&[*count, 1])?;
    let next_bytes = format::sum(&[*bytes, wire])?;
    if next_count > store.limits.maximum_recovery_entries()
        || next_bytes > store.limits.maximum_recovery_bytes()
    {
        return Err(RaftLogError::Limit);
    }
    *count = next_count;
    *bytes = next_bytes;
    Ok(())
}

pub(super) fn validate(store: &RedbLogStore) -> Result<(), RaftLogError> {
    store.workspace(0)?;
    // One writer also waits out any transaction retained by a Database donor.
    // Inventory is checked before open_table can create anything; no rows change.
    let tx = store
        .database
        .begin_write()
        .map_err(format::transaction_error)?;
    let mut count = 0;
    for table in tx
        .list_tables()
        .map_err(|error| format::storage_error(&error))?
    {
        if !matches!(table.name(), "wf_raft_meta_v1" | "wf_raft_log_v1") {
            return Err(RaftLogError::Corrupt);
        }
        count += 1;
    }
    if count != 2
        || tx
            .list_multimap_tables()
            .map_err(|error| format::storage_error(&error))?
            .next()
            .is_some()
    {
        return Err(RaftLogError::Corrupt);
    }
    let metadata = tx.open_table(format::META).map_err(format::table_error)?;
    let mut count = 0;
    for row in metadata
        .iter()
        .map_err(|error| format::storage_error(&error))?
    {
        let (key, _) = row.map_err(|error| format::storage_error(&error))?;
        if !matches!(key.value(), b"manifest" | b"state") {
            return Err(RaftLogError::Corrupt);
        }
        count += 1;
    }
    if count != 2 {
        return Err(RaftLogError::Corrupt);
    }
    {
        let guard = metadata
            .get(format::MANIFEST)
            .map_err(|error| format::storage_error(&error))?
            .ok_or(RaftLogError::Corrupt)?;
        format::check_manifest(guard.value(), &store.group, &store.recovery)?;
    }
    let state = read::metadata(&metadata, store)?;
    let table = tx.open_table(format::LOGS).map_err(format::table_error)?;
    validate_entries(store, &table, state)
}

fn validate_entries(
    store: &RedbLogStore,
    table: &impl ReadableTable<u64, &'static [u8]>,
    state: RaftLogState,
) -> Result<(), RaftLogError> {
    let mut count = 0;
    let mut bytes = 0;
    let mut previous: Option<LogIdentity> = None;
    let mut committed = state.committed().is_none();
    for row in table
        .iter()
        .map_err(|error| format::storage_error(&error))?
    {
        let (key, value) = row.map_err(|error| format::storage_error(&error))?;
        census(store, &mut count, &mut bytes, value.value().len())?;
        let (id, _) = format::decode_entry(
            value.value(),
            key.value(),
            &store.group,
            &store.recovery,
            store.limits.maximum_entry_bytes(),
        )?;
        if previous.map_or(id.index().get() != 0, |before| !format::follows(before, id)) {
            return Err(RaftLogError::Corrupt);
        }
        if state.committed() == Some(id) {
            committed = true;
        }
        previous = Some(id);
    }
    if previous != state.last_log() || !committed {
        return Err(RaftLogError::Corrupt);
    }
    Ok(())
}
