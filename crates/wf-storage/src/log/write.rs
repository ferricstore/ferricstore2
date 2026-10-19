//! Prevalidated atomic updates; one entry wire at a time and no suffix key list.

use std::ops::Bound;

use redb::{ReadableTable, WriteTransaction};
use wf_ports::log::{RaftLogEntry, RaftLogError, RaftLogState, RaftVote};
use wf_types::version::LogIdentity;

use super::{
    format, read, recovery,
    store::{self, RedbLogStore},
};

fn load(tx: &WriteTransaction, store: &RedbLogStore) -> Result<RaftLogState, RaftLogError> {
    let table = tx.open_table(format::META).map_err(format::table_error)?;
    read::metadata(&table, store)
}

fn replace(
    tx: &WriteTransaction,
    store: &RedbLogStore,
    state: RaftLogState,
) -> Result<(), RaftLogError> {
    let wire = format::state(state, &store.group, &store.recovery)?;
    tx.open_table(format::META)
        .map_err(format::table_error)?
        .insert(format::STATE, wire.as_slice())
        .map_err(|error| format::storage_error(&error))?;
    Ok(())
}

pub(super) fn save_vote(store: &RedbLogStore, vote: RaftVote) -> Result<(), RaftLogError> {
    let tx = store
        .database
        .begin_write()
        .map_err(format::transaction_error)?;
    let state = load(&tx, store)?;
    if let Some(before) = state.vote() {
        if (before.term(), before.node(), before.committed())
            > (vote.term(), vote.node(), vote.committed())
        {
            return Err(RaftLogError::Conflict);
        }
        if before == vote {
            return Ok(());
        }
    }
    replace(
        &tx,
        store,
        RaftLogState::new(Some(vote), state.committed(), state.last_log()),
    )?;
    store::commit(tx)
}

fn identity(
    table: &impl ReadableTable<u64, &'static [u8]>,
    store: &RedbLogStore,
    index: u64,
) -> Result<LogIdentity, RaftLogError> {
    let guard = table
        .get(index)
        .map_err(|error| format::storage_error(&error))?
        .ok_or(RaftLogError::Corrupt)?;
    Ok(format::decode_entry(
        guard.value(),
        index,
        &store.group,
        &store.recovery,
        store.limits.maximum_entry_bytes(),
    )?
    .0)
}

pub(super) fn save_committed(store: &RedbLogStore, id: LogIdentity) -> Result<(), RaftLogError> {
    let tx = store
        .database
        .begin_write()
        .map_err(format::transaction_error)?;
    let state = load(&tx, store)?;
    if state.committed().is_some_and(|before| {
        before.index() > id.index() || (before.index() == id.index() && before != id)
    }) || state
        .last_log()
        .is_none_or(|last| id.index() > last.index())
    {
        return Err(RaftLogError::Conflict);
    }
    if state.committed() == Some(id) {
        return Ok(());
    }
    {
        let table = tx.open_table(format::LOGS).map_err(format::table_error)?;
        if identity(&table, store, id.index().get())? != id {
            return Err(RaftLogError::Conflict);
        }
    }
    replace(
        &tx,
        store,
        RaftLogState::new(state.vote(), Some(id), state.last_log()),
    )?;
    store::commit(tx)
}

fn admission(store: &RedbLogStore, entries: &[RaftLogEntry]) -> Result<(), RaftLogError> {
    if entries.len() > store.limits.maximum_batch_entries() {
        return Err(RaftLogError::Limit);
    }
    let mut bytes = 0;
    let mut largest = 0;
    for entry in entries {
        if entry.body().len() > store.limits.maximum_entry_bytes() {
            return Err(RaftLogError::Limit);
        }
        bytes = format::sum(&[bytes, 512, entry.body().len()])?;
        largest = largest.max(entry.body().len());
    }
    if bytes > store.limits.maximum_append_bytes() {
        return Err(RaftLogError::Limit);
    }
    store.workspace(format::sum(&[bytes, 70, largest])?)?;
    if entries
        .windows(2)
        .any(|pair| !format::follows(pair[0].id(), pair[1].id()))
    {
        return Err(RaftLogError::Conflict);
    }
    Ok(())
}

fn validate_batch(
    table: &impl ReadableTable<u64, &'static [u8]>,
    store: &RedbLogStore,
    entries: &[RaftLogEntry],
    before: Option<LogIdentity>,
) -> Result<Option<LogIdentity>, RaftLogError> {
    let mut tail = before;
    for entry in entries {
        let index = entry.id().index().get();
        if before.is_some_and(|last| index <= last.index().get()) {
            let guard = table
                .get(index)
                .map_err(|error| format::storage_error(&error))?
                .ok_or(RaftLogError::Corrupt)?;
            // Reject differing lengths before hashing an unrelated larger old body.
            if guard.value().len() != format::sum(&[70, entry.body().len()])? {
                return Err(RaftLogError::Conflict);
            }
            let (id, body) = format::decode_entry(
                guard.value(),
                index,
                &store.group,
                &store.recovery,
                store.limits.maximum_entry_bytes(),
            )?;
            if id != entry.id() || body != entry.body() {
                return Err(RaftLogError::Conflict);
            }
        } else {
            if tail.map_or(index != 0, |last| !format::follows(last, entry.id())) {
                return Err(RaftLogError::Conflict);
            }
            tail = Some(entry.id());
        }
    }
    Ok(tail)
}

pub(super) fn append(store: &RedbLogStore, entries: &[RaftLogEntry]) -> Result<(), RaftLogError> {
    if entries.is_empty() {
        return Ok(());
    }
    admission(store, entries)?;
    let tx = store
        .database
        .begin_write()
        .map_err(format::transaction_error)?;
    let state = load(&tx, store)?;
    let last = {
        let mut table = tx.open_table(format::LOGS).map_err(format::table_error)?;
        let last = validate_batch(&table, store, entries, state.last_log())?;
        if last == state.last_log() {
            return Ok(());
        }
        for entry in entries {
            if state
                .last_log()
                .is_some_and(|before| entry.id().index() <= before.index())
            {
                continue;
            }
            let wire = format::entry(entry.id(), entry.body(), &store.group, &store.recovery)?;
            table
                .insert(entry.id().index().get(), wire.as_slice())
                .map_err(|error| format::storage_error(&error))?;
        }
        last
    };
    replace(
        &tx,
        store,
        RaftLogState::new(state.vote(), state.committed(), last),
    )?;
    store::commit(tx)
}

fn suffix(
    table: &impl ReadableTable<u64, &'static [u8]>,
    store: &RedbLogStore,
    index: u64,
    remaining: Option<LogIdentity>,
    expected_last: Option<LogIdentity>,
) -> Result<(), RaftLogError> {
    let mut count = 0;
    let mut bytes = 0;
    let mut previous = remaining;
    for row in table
        .range::<u64>((Bound::Included(index), Bound::Unbounded))
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
    if previous != expected_last {
        return Err(RaftLogError::Corrupt);
    }
    Ok(())
}

pub(super) fn truncate(store: &RedbLogStore, index: u64) -> Result<(), RaftLogError> {
    let tx = store
        .database
        .begin_write()
        .map_err(format::transaction_error)?;
    let state = load(&tx, store)?;
    if state
        .committed()
        .is_some_and(|id| index <= id.index().get())
    {
        return Err(RaftLogError::Conflict);
    }
    if state
        .last_log()
        .is_none_or(|last| index > last.index().get())
    {
        return Ok(());
    }
    let remaining = {
        let mut table = tx.open_table(format::LOGS).map_err(format::table_error)?;
        let remaining = index
            .checked_sub(1)
            .map(|before| identity(&table, store, before))
            .transpose()?;
        suffix(&table, store, index, remaining, state.last_log())?;
        // The validated affected-row census is paid once; redb deletes by range.
        table
            .retain_in::<u64, _>((Bound::Included(index), Bound::Unbounded), |_, _| false)
            .map_err(|error| format::storage_error(&error))?;
        remaining
    };
    replace(
        &tx,
        store,
        RaftLogState::new(state.vote(), state.committed(), remaining),
    )?;
    store::commit(tx)
}
