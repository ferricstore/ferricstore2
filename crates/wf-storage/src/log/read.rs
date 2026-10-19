//! One coherent read snapshot, borrowed validation, and prospective output owners.

use std::ops::Bound;

use redb::{ReadableDatabase, ReadableTable};
use wf_ports::log::{RaftLogEntry, RaftLogError, RaftLogState};

use super::{format, store::RedbLogStore};

pub(super) fn metadata(
    table: &impl ReadableTable<&'static [u8], &'static [u8]>,
    store: &RedbLogStore,
) -> Result<RaftLogState, RaftLogError> {
    let guard = table
        .get(format::STATE)
        .map_err(|error| format::storage_error(&error))?
        .ok_or(RaftLogError::Corrupt)?;
    format::decode_state(guard.value(), &store.group, &store.recovery)
}

pub(super) fn state(store: &RedbLogStore) -> Result<RaftLogState, RaftLogError> {
    let tx = store
        .database
        .begin_read()
        .map_err(format::transaction_error)?;
    let table = tx.open_table(format::META).map_err(format::table_error)?;
    metadata(&table, store)
}

pub(super) fn point(
    store: &RedbLogStore,
    index: u64,
    maximum_owned_bytes: usize,
) -> Result<Option<RaftLogEntry>, RaftLogError> {
    let body_cap = maximum_owned_bytes
        .checked_sub(512)
        .ok_or(RaftLogError::Limit)?
        .min(store.limits.maximum_entry_bytes());
    let tx = store
        .database
        .begin_read()
        .map_err(format::transaction_error)?;
    let table = tx.open_table(format::LOGS).map_err(format::table_error)?;
    let guard = table
        .get(index)
        .map_err(|error| format::storage_error(&error))?;
    let Some(guard) = guard else {
        return Ok(None);
    };
    let (id, body) = format::decode_entry(
        guard.value(),
        index,
        &store.group,
        &store.recovery,
        body_cap,
    )?;
    let bytes = format::sum(&[512, body.len()])?;
    if bytes > maximum_owned_bytes || bytes > store.limits.maximum_read_bytes() {
        return Err(RaftLogError::Limit);
    }
    store.workspace(bytes)?;
    let mut owned = format::allocate(body.len())?;
    owned.extend_from_slice(body);
    Ok(Some(RaftLogEntry::new(id, owned)))
}

fn interval(start: Bound<u64>, end: Bound<u64>) -> Result<(u128, u128), RaftLogError> {
    let lower = match start {
        Bound::Included(value) => u128::from(value),
        Bound::Excluded(value) => u128::from(value) + 1,
        Bound::Unbounded => 0,
    };
    let upper = match end {
        Bound::Included(value) => u128::from(value) + 1,
        Bound::Excluded(value) => u128::from(value),
        Bound::Unbounded => u128::from(u64::MAX) + 1,
    };
    if lower > upper {
        return Err(RaftLogError::InvalidRange);
    }
    Ok((lower, upper))
}

pub(super) fn page(
    store: &RedbLogStore,
    start: Bound<u64>,
    end: Bound<u64>,
) -> Result<Vec<RaftLogEntry>, RaftLogError> {
    let (lower, upper) = interval(start, end)?;
    let mut output = Vec::new();
    if lower == upper {
        return Ok(output);
    }
    let lower = u64::try_from(lower).map_err(|_| RaftLogError::InvalidRange)?;
    let upper = match u64::try_from(upper) {
        Ok(value) => Bound::Excluded(value),
        Err(_) => Bound::Unbounded,
    };
    let tx = store
        .database
        .begin_read()
        .map_err(format::transaction_error)?;
    let table = tx.open_table(format::LOGS).map_err(format::table_error)?;
    let mut returned = 0;
    let mut previous = None;
    let mut rows = table
        .range::<u64>((Bound::Included(lower), upper))
        .map_err(|error| format::storage_error(&error))?;
    while output.len() < store.limits.maximum_read_entries() {
        let Some(row) = rows.next() else {
            break;
        };
        let (key, value) = row.map_err(|error| format::storage_error(&error))?;
        let (id, body) = format::decode_entry(
            value.value(),
            key.value(),
            &store.group,
            &store.recovery,
            store.limits.maximum_entry_bytes(),
        )?;
        if previous.is_some_and(|before| !format::follows(before, id)) {
            return Err(RaftLogError::Corrupt);
        }
        let admitted = format::sum(&[returned, 512, body.len()]).and_then(|prospective| {
            if prospective > store.limits.maximum_read_bytes() {
                return Err(RaftLogError::Limit);
            }
            store.workspace(prospective)?;
            Ok(prospective)
        });
        let prospective = match admitted {
            Ok(bytes) => bytes,
            Err(error) => {
                if output.is_empty() {
                    return Err(error);
                }
                break;
            }
        };
        output
            .try_reserve_exact(1)
            .map_err(|_| RaftLogError::Allocation)?;
        let mut owned = format::allocate(body.len())?;
        owned.extend_from_slice(body);
        output.push(RaftLogEntry::new(id, owned));
        returned = prospective;
        previous = Some(id);
    }
    Ok(output)
}
