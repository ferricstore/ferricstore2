//! Trusted received-boundary publication with one prospectively charged scan.

use std::ops::Bound;

use redb::ReadableTable;
use sha2::{Digest, Sha256};
use wf_ports::log::{RaftLogError, RaftLogState};
use wf_types::version::LogIdentity;

use super::{
    format, recovery,
    store::{self, RedbLogStore},
    write,
};

fn admit(
    state: RaftLogState,
    expected: RaftLogState,
    position: LogIdentity,
) -> Result<(), RaftLogError> {
    if state != expected {
        return Err(RaftLogError::Conflict);
    }
    for protected in [state.last_purged(), state.committed()]
        .into_iter()
        .flatten()
    {
        if position.index() <= protected.index()
            || (position.term(), position.leader_node())
                < (protected.term(), protected.leader_node())
        {
            return Err(RaftLogError::Conflict);
        }
    }
    Ok(())
}

fn scan(
    store: &RedbLogStore,
    table: &impl ReadableTable<u64, &'static [u8]>,
    state: RaftLogState,
    position: LogIdentity,
    prepared: [u8; 32],
) -> Result<bool, RaftLogError> {
    let mut count = 0;
    let mut bytes = 0;
    let mut previous = state.last_purged();
    let mut committed = state.committed().is_none() || state.committed() == state.last_purged();
    let mut matched = false;
    for row in table
        .iter()
        .map_err(|error| format::storage_error(&error))?
    {
        let (key, value) = row.map_err(|error| format::storage_error(&error))?;
        recovery::census(store, &mut count, &mut bytes, value.value().len())?;
        let (id, body) = format::decode_entry(
            value.value(),
            key.value(),
            &store.group,
            &store.recovery,
            store.limits.maximum_entry_bytes(),
        )?;
        if previous.map_or(id.index().get() != 0, |before| !format::follows(before, id)) {
            return Err(RaftLogError::Corrupt);
        }
        if matched {
            if !committed
                || state.last_log().is_none_or(|tail| {
                    id.index() > tail.index() || (id.index() == tail.index() && id != tail)
                })
            {
                return Err(RaftLogError::Corrupt);
            }
            return Ok(true);
        }
        if state.committed() == Some(id) {
            committed = true;
        }
        if id == position {
            let actual: [u8; 32] = Sha256::digest(body).into();
            if actual != prepared {
                return Err(RaftLogError::Conflict);
            }
            matched = true;
        }
        previous = Some(id);
    }
    if !committed || previous != state.last_log() {
        return Err(RaftLogError::Corrupt);
    }
    Ok(matched)
}

pub(super) fn boundary(
    store: &RedbLogStore,
    expected: RaftLogState,
    position: LogIdentity,
    prepared: [u8; 32],
    snapshot_id: [u8; 32],
) -> Result<(), RaftLogError> {
    let tx = store
        .database
        .begin_write()
        .map_err(format::transaction_error)?;
    let state = write::load(&tx, store)?;
    if state.last_purged() == Some(position) && state.purged_snapshot_id() == Some(snapshot_id) {
        return Ok(());
    }
    admit(state, expected, position)?;
    let keep_suffix = {
        let mut table = tx.open_table(format::LOGS).map_err(format::table_error)?;
        let keep = scan(store, &table, state, position, prepared)?;
        let end = if keep {
            Bound::Included(position.index().get())
        } else {
            Bound::Unbounded
        };
        table
            .retain_in::<u64, _>((Bound::Unbounded, end), |_, _| false)
            .map_err(|error| format::storage_error(&error))?;
        keep
    };
    write::replace(
        &tx,
        store,
        RaftLogState::new_with_anchor(
            state.vote(),
            Some(position),
            if keep_suffix {
                state.last_log()
            } else {
                Some(position)
            },
            Some((position, snapshot_id)),
        ),
    )?;
    store::commit(tx)
}
