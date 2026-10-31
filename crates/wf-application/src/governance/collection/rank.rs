//! Fallibly reserved result slots and move-only globally ranked original values.
use super::{
    GovernanceCatalogCollection as Collection, GovernanceCatalogCollectionError as E,
    GovernanceCatalogCollectionLimits as Limits,
};
use std::cmp::Ordering;
use wf_ports::state::LoadedGovernanceRecord;

pub(super) fn start<T>(limit: usize, limits: Limits) -> Result<Collection<T>, E> {
    if limit == 0
        || [
            limits.page.maximum_steps,
            limits.page.maximum_bytes,
            limits.page.maximum_temporary_bytes,
            limits.load.maximum_steps,
            limits.load.maximum_bytes,
            limits.load.maximum_temporary_bytes,
            limits.maximum_result_bytes,
            limits.workspace_bytes,
        ]
        .contains(&0)
    {
        return Err(E::InvalidOptions);
    }
    let slots = limit
        .checked_mul(std::mem::size_of::<LoadedGovernanceRecord<T>>())
        .ok_or(E::Limit)?;
    let total = [
        slots,
        128,
        limits.maximum_result_bytes,
        limits.page.maximum_bytes,
        limits.page.maximum_bytes,
        limits.page.maximum_temporary_bytes,
        limits.load.maximum_bytes,
        limits.load.maximum_temporary_bytes,
    ]
    .into_iter()
    .try_fold(0_usize, usize::checked_add)
    .ok_or(E::Limit)?;
    if total > limits.workspace_bytes {
        return Err(E::Limit);
    }
    let mut entries = Vec::new();
    entries
        .try_reserve_exact(limit)
        .map_err(|_| E::Allocation)?;
    Ok(Collection {
        entries,
        owned_bytes: 0,
    })
}

pub(super) fn retain<T>(
    result: &mut Collection<T>,
    candidate: LoadedGovernanceRecord<T>,
    limit: usize,
    compare: &impl Fn(&T, &T) -> Ordering,
    limits: Limits,
) -> Result<(), E> {
    if candidate.owned_bytes < std::mem::size_of::<T>()
        || candidate.owned_bytes > limits.load.maximum_bytes
    {
        return Err(E::InvalidLoader);
    }
    let index = result
        .entries
        .partition_point(|entry| compare(&entry.record, &candidate.record).is_lt());
    let full = result.entries.len() == limit;
    if full && index == result.entries.len() {
        return Ok(());
    }
    let removed = if full {
        result.entries.last().map_or(0, |entry| entry.owned_bytes)
    } else {
        0
    };
    let bytes = result
        .owned_bytes
        .checked_sub(removed)
        .and_then(|n| n.checked_add(candidate.owned_bytes))
        .ok_or(E::Limit)?;
    if bytes > limits.maximum_result_bytes {
        return Err(E::Limit);
    }
    if full {
        drop(result.entries.pop());
    }
    result.entries.insert(index, candidate);
    result.owned_bytes = bytes;
    Ok(())
}
