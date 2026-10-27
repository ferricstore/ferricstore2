//! Append-only row chains and lazy bounded open addressing, without key clones.

use super::{
    Error, PolicyMergeArena as Arena, PolicyMergeRef as Ref, PolicyMergeRow, authority, equality,
    overflow, pay, value::guard_pair,
};
use crate::policy::complete::PolicySchemaCharge;

pub(super) fn find(
    arena: &Arena<'_>,
    map: usize,
    key: Ref<'_>,
    hash: u64,
    charge: &mut PolicySchemaCharge,
) -> Result<Option<(usize, usize)>, Error> {
    let map = arena.maps.get(map).ok_or_else(authority)?;
    if map.capacity == 0 {
        return Ok(None);
    }
    let mut slot = initial_slot(hash, map.capacity)?;
    for _ in 0..map.capacity {
        pay(charge, 16)?;
        let value = *arena.lookup.get(map.table + slot).ok_or_else(authority)?;
        if value == 0 {
            return Ok(None);
        }
        if value != u64::MAX {
            let row = usize::try_from(value - 1).map_err(|_| overflow())?;
            let entry = arena.rows.get(row).ok_or_else(authority)?;
            if entry.hash == hash && equality::equal(arena, entry.key, key, charge)? {
                return Ok(Some((row, map.table + slot)));
            }
        }
        slot = (slot + 1) & (map.capacity - 1);
    }
    Ok(None)
}

pub(super) fn insert<'input>(
    arena: &mut Arena<'input>,
    map: usize,
    key: Ref<'input>,
    value: Ref<'input>,
    hash: u64,
    charge: &mut PolicySchemaCharge,
) -> Result<(), Error> {
    let (table, capacity) = grow(arena, map, charge)?;
    pay(charge, 128)?;
    if arena.rows.len() == arena.rows.capacity() {
        return Err(Error::ItemLimit);
    }
    let id = arena.rows.len();
    let previous = arena.maps.get(map).ok_or_else(authority)?.last;
    arena.rows.push(PolicyMergeRow {
        key,
        value,
        hash,
        previous,
        live: true,
    });
    let slot = empty(arena, table, capacity, hash, charge)?;
    pay(charge, 128)?;
    *arena.lookup.get_mut(slot).ok_or_else(authority)? = u64::try_from(id)
        .map_err(|_| overflow())?
        .checked_add(1)
        .ok_or_else(overflow)?;
    pay(charge, 128)?;
    let map = arena.maps.get_mut(map).ok_or_else(authority)?;
    map.last = id;
    map.len = map.len.checked_add(1).ok_or_else(overflow)?;
    map.staged = map.staged.checked_add(1).ok_or_else(overflow)?;
    map.table = table;
    map.capacity = capacity;
    Ok(())
}

pub(super) fn find_text(
    arena: &Arena<'_>,
    map: usize,
    bytes: &[u8],
    atom: bool,
    hash: u64,
    charge: &mut PolicySchemaCharge,
) -> Result<Option<(usize, usize)>, Error> {
    let map = arena.maps.get(map).ok_or_else(authority)?;
    if map.capacity == 0 {
        return Ok(None);
    }
    let mut slot = initial_slot(hash, map.capacity)?;
    for _ in 0..map.capacity {
        pay(charge, 16)?;
        let value = *arena.lookup.get(map.table + slot).ok_or_else(authority)?;
        if value == 0 {
            return Ok(None);
        }
        if value != u64::MAX {
            let id = usize::try_from(value - 1).map_err(|_| overflow())?;
            if text_matches(arena, id, bytes, atom, hash, charge)? {
                return Ok(Some((id, map.table + slot)));
            }
        }
        slot = (slot + 1) & (map.capacity - 1);
    }
    Ok(None)
}

fn text_matches(
    arena: &Arena<'_>,
    id: usize,
    bytes: &[u8],
    atom: bool,
    hash: u64,
    charge: &mut PolicySchemaCharge,
) -> Result<bool, Error> {
    let row = arena.rows.get(id).ok_or_else(authority)?;
    if row.hash != hash {
        return Ok(false);
    }
    pay(charge, 128)?;
    let key = arena.view(row.key);
    let candidate = if atom {
        key.atom_bytes()
    } else {
        key.binary_bytes()
    };
    let Some(candidate) = candidate else {
        return Ok(false);
    };
    pay(
        charge,
        128_usize
            .checked_add(bytes.len())
            .and_then(|n| n.checked_add(candidate.len()))
            .ok_or_else(overflow)?,
    )?;
    Ok(bytes == candidate)
}

pub(super) fn replace<'input>(
    arena: &mut Arena<'input>,
    row: usize,
    value: Ref<'input>,
    charge: &mut PolicySchemaCharge,
) -> Result<(), Error> {
    pay(charge, 128)?;
    arena.rows.get_mut(row).ok_or_else(authority)?.value = value;
    Ok(())
}

pub(super) fn delete(
    arena: &mut Arena<'_>,
    map: usize,
    found: (usize, usize),
    charge: &mut PolicySchemaCharge,
) -> Result<(), Error> {
    pay(charge, 128)?;
    arena.rows.get_mut(found.0).ok_or_else(authority)?.live = false;
    pay(charge, 128)?;
    *arena.lookup.get_mut(found.1).ok_or_else(authority)? = u64::MAX;
    pay(charge, 128)?;
    let map = arena.maps.get_mut(map).ok_or_else(authority)?;
    map.len = map.len.checked_sub(1).ok_or_else(overflow)?;
    Ok(())
}

fn grow(
    arena: &mut Arena<'_>,
    map: usize,
    charge: &mut PolicySchemaCharge,
) -> Result<(usize, usize), Error> {
    let old = arena.maps.get(map).ok_or_else(authority)?;
    if old.staged.checked_add(1).ok_or_else(overflow)? <= old.capacity / 2 {
        return Ok((old.table, old.capacity));
    }
    let old_capacity = old.capacity;
    let mut row = old.last;
    let capacity = if old_capacity == 0 {
        2
    } else {
        old_capacity.checked_mul(2).ok_or_else(overflow)?
    };
    let table = arena.lookup.len();
    for _ in 0..capacity {
        pay(charge, 128)?;
        if arena.lookup.len() == arena.lookup.capacity() {
            return Err(Error::ItemLimit);
        }
        arena.lookup.push(0);
    }
    while row != usize::MAX {
        pay(charge, 16)?;
        let entry = arena.rows.get(row).ok_or_else(authority)?;
        let (previous, hash, live) = (entry.previous, entry.hash, entry.live);
        if live {
            let slot = cached_empty(arena, table, capacity, hash, charge)?;
            pay(charge, 128)?;
            *arena.lookup.get_mut(slot).ok_or_else(authority)? =
                u64::try_from(row).map_err(|_| overflow())? + 1;
        }
        row = previous;
    }
    if old_capacity != 0 {
        pay(charge, 128)?;
        let map = arena.maps.get_mut(map).ok_or_else(authority)?;
        map.table = table;
        map.capacity = capacity;
    }
    Ok((table, capacity))
}

fn empty(
    arena: &Arena<'_>,
    table: usize,
    capacity: usize,
    hash: u64,
    charge: &mut PolicySchemaCharge,
) -> Result<usize, Error> {
    pay(charge, 16)?;
    cached_empty(arena, table, capacity, hash, charge)
}

fn cached_empty(
    arena: &Arena<'_>,
    table: usize,
    capacity: usize,
    hash: u64,
    charge: &mut PolicySchemaCharge,
) -> Result<usize, Error> {
    let mut slot = initial_slot(hash, capacity)?;
    for attempt in 0..capacity {
        if attempt != 0 {
            pay(charge, 16)?;
        }
        let value = *arena.lookup.get(table + slot).ok_or_else(authority)?;
        if value == 0 || value == u64::MAX {
            return Ok(table + slot);
        }
        slot = (slot + 1) & (capacity - 1);
    }
    Err(Error::ItemLimit)
}

pub(super) fn guard(
    arena: &Arena<'_>,
    map: usize,
    charge: &mut PolicySchemaCharge,
) -> Result<(), Error> {
    let mut row = arena.maps.get(map).ok_or_else(authority)?.last;
    while row != usize::MAX {
        pay(charge, 128)?;
        let entry = arena.rows.get(row).ok_or_else(authority)?;
        if entry.live {
            guard_pair(arena.view(entry.key), arena.view(entry.value), charge)?;
        }
        row = entry.previous;
    }
    Ok(())
}

pub(super) fn freeze(
    arena: &mut Arena<'_>,
    map: usize,
    charge: &mut PolicySchemaCharge,
) -> Result<(), Error> {
    if arena.maps.get(map).ok_or_else(authority)?.order != usize::MAX {
        return Ok(());
    }
    let mut row = arena.maps.get(map).ok_or_else(authority)?.last;
    while row != usize::MAX {
        pay(charge, 128)?;
        let entry = arena.rows.get(row).ok_or_else(authority)?;
        let (previous, child) = (
            entry.previous,
            if entry.live { Some(entry.value) } else { None },
        );
        if let Some(Ref::Map(child)) = child {
            freeze(arena, child, charge)?;
        }
        row = previous;
    }
    let start = arena.order.len();
    row = arena.maps.get(map).ok_or_else(authority)?.last;
    while row != usize::MAX {
        pay(charge, 128)?;
        let entry = arena.rows.get(row).ok_or_else(authority)?;
        if entry.live {
            pay(charge, 128)?;
            if arena.order.len() == arena.order.capacity() {
                return Err(Error::ItemLimit);
            }
            arena.order.push(row);
        }
        row = entry.previous;
    }
    let count = arena.order.len() - start;
    for index in 0..count / 2 {
        pay(charge, 256)?;
        arena.order.swap(start + index, start + count - index - 1);
    }
    pay(charge, 128)?;
    arena.maps.get_mut(map).ok_or_else(authority)?.order = start;
    Ok(())
}

fn initial_slot(hash: u64, capacity: usize) -> Result<usize, Error> {
    let mask =
        u64::try_from(capacity.checked_sub(1).ok_or_else(overflow)?).map_err(|_| overflow())?;
    usize::try_from(hash & mask).map_err(|_| overflow())
}
