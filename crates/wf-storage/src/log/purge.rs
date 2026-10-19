//! Bounded borrowed prefix proof, followed by atomic anchor and ranged deletion.

use redb::ReadableTable;
use std::ops::Bound;
use wf_ports::log::{RaftLogError, RaftLogState};
use wf_types::version::LogIdentity;

use super::{
    format, recovery,
    store::{self, RedbLogStore},
    write,
};

fn validate(
    table: &impl ReadableTable<u64, &'static [u8]>,
    store: &RedbLogStore,
    state: RaftLogState,
    position: LogIdentity,
) -> Result<(), RaftLogError> {
    let mut count = 0;
    let mut bytes = 0;
    let mut previous = state.last_purged();
    for row in table
        .range::<u64>((Bound::Unbounded, Bound::Included(position.index().get())))
        .map_err(|error| format::storage_error(&error))?
    {
        let (key, value) = row.map_err(|error| format::storage_error(&error))?;
        recovery::census(store, &mut count, &mut bytes, value.value().len())?;
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
        previous = Some(id);
    }
    if previous.is_none_or(|id| id.index() != position.index()) {
        return Err(RaftLogError::Corrupt);
    }
    if previous != Some(position) {
        return Err(RaftLogError::Conflict);
    }
    let mut next = table
        .range::<u64>((Bound::Excluded(position.index().get()), Bound::Unbounded))
        .map_err(|error| format::storage_error(&error))?;
    if let Some(row) = next.next() {
        let (key, value) = row.map_err(|error| format::storage_error(&error))?;
        recovery::census(store, &mut count, &mut bytes, value.value().len())?;
        let (id, _) = format::decode_entry(
            value.value(),
            key.value(),
            &store.group,
            &store.recovery,
            store.limits.maximum_entry_bytes(),
        )?;
        if !format::follows(position, id) {
            return Err(RaftLogError::Corrupt);
        }
    } else if state.last_log() != Some(position) {
        return Err(RaftLogError::Corrupt);
    }
    Ok(())
}

pub(super) fn through(
    store: &RedbLogStore,
    position: LogIdentity,
    snapshot_id: [u8; 32],
) -> Result<(), RaftLogError> {
    let tx = store
        .database
        .begin_write()
        .map_err(format::transaction_error)?;
    let state = write::load(&tx, store)?;
    if let Some(before) = state.last_purged() {
        if position.index() < before.index() {
            return Err(RaftLogError::Conflict);
        }
        if position.index() == before.index() {
            return if position == before && state.purged_snapshot_id() == Some(snapshot_id) {
                Ok(())
            } else {
                Err(RaftLogError::Conflict)
            };
        }
    }
    if state.committed().is_none_or(|id| {
        position.index() > id.index() || (position.index() == id.index() && position != id)
    }) {
        return Err(RaftLogError::Conflict);
    }
    {
        let mut table = tx.open_table(format::LOGS).map_err(format::table_error)?;
        validate(&table, store, state, position)?;
        table
            .retain_in::<u64, _>(
                (Bound::Unbounded, Bound::Included(position.index().get())),
                |_, _| false,
            )
            .map_err(|error| format::storage_error(&error))?;
    }
    write::replace(
        &tx,
        store,
        RaftLogState::new_with_anchor(
            state.vote(),
            state.committed(),
            state.last_log(),
            Some((position, snapshot_id)),
        ),
    )?;
    store::commit(tx)
}
