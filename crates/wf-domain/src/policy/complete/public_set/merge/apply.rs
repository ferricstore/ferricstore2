//! Source exact-key-first deep merge and resolved atom-states deletion.

use super::{
    Error, PolicyMergeArena as Arena, PolicyMergeRef as Ref, authority, equality, overflow, pay,
    rows, value::guard_pair,
};
use crate::policy::complete::{PolicySchemaCharge, flow_input::value::PolicyInputKind as Kind};

pub(super) fn merge<'input>(
    arena: &mut Arena<'input>,
    left: Option<Ref<'input>>,
    right: Ref<'input>,
    states: bool,
    charge: &mut PolicySchemaCharge,
) -> Result<usize, Error> {
    pay(charge, 128)?;
    let order = source_order(arena, right, charge)?;
    let map = match left {
        Some(Ref::Map(id)) => id,
        other => {
            let id = arena.map(charge)?;
            if let Some(left) = other {
                copy(arena, id, left, charge)?;
            }
            id
        }
    };
    let count = arena.view(right).len();
    for source in 0..count {
        pay(charge, 128)?;
        let logical = match order {
            Some(p) => {
                usize::try_from(*p.get(source).ok_or_else(authority)?).map_err(|_| overflow())?
            }
            None => source,
        };
        let (key, value) = row(arena, right, logical).ok_or_else(authority)?;
        update(arena, map, key, value, states, charge)?;
    }
    Ok(map)
}

fn copy<'input>(
    arena: &mut Arena<'input>,
    map: usize,
    source: Ref<'input>,
    charge: &mut PolicySchemaCharge,
) -> Result<(), Error> {
    for index in 0..arena.view(source).len() {
        pay(charge, 128)?;
        let (key, value) = row(arena, source, index).ok_or_else(authority)?;
        let hash = equality::hash(arena, key, charge)?;
        rows::insert(arena, map, key, value, hash, charge)?;
    }
    Ok(())
}

fn update<'input>(
    arena: &mut Arena<'input>,
    map: usize,
    key: Ref<'input>,
    value: Ref<'input>,
    states: bool,
    charge: &mut PolicySchemaCharge,
) -> Result<(), Error> {
    let hash = equality::hash(arena, key, charge)?;
    let exact = rows::find(arena, map, key, hash, charge)?;
    let found = if states || exact.is_some() {
        exact
    } else {
        equivalent(arena, map, key, hash, charge)?
    };
    if states && arena.view(value).atom_bytes() == Some(b"nil") {
        if let Some(found) = found {
            rows::delete(arena, map, found, charge)?;
        }
        return Ok(());
    }
    let value = if let Some((id, _)) = found {
        let entry = arena.rows.get(id).ok_or_else(authority)?;
        let (old, resolved) = (entry.value, entry.key);
        if arena.view(old).kind() == Kind::Map && arena.view(value).kind() == Kind::Map {
            let special = !states && arena.view(resolved).atom_bytes() == Some(b"states");
            Ref::Map(merge(arena, Some(old), value, special, charge)?)
        } else {
            value
        }
    } else {
        value
    };
    if let Some((row, _)) = found {
        rows::replace(arena, row, value, charge)
    } else {
        rows::insert(arena, map, key, value, hash, charge)
    }
}

fn equivalent(
    arena: &Arena<'_>,
    map: usize,
    key: Ref<'_>,
    hash: u64,
    charge: &mut PolicySchemaCharge,
) -> Result<Option<(usize, usize)>, Error> {
    let key = arena.view(key);
    if let Some(bytes) = key.binary_bytes() {
        pay(charge, 128)?;
        rows::guard(arena, map, charge)?;
        rows::find_text(arena, map, bytes, true, hash, charge)
    } else if let Some(bytes) = key.atom_bytes() {
        rows::find_text(arena, map, bytes, false, hash, charge)
    } else {
        Ok(None)
    }
}

pub(super) fn source_order<'input>(
    arena: &Arena<'input>,
    value: Ref<'input>,
    charge: &mut PolicySchemaCharge,
) -> Result<Option<&'input [u32]>, Error> {
    let count = arena.view(value).len();
    for index in 0..count {
        pay(charge, 128)?;
        let (key, child) = row(arena, value, index).ok_or_else(authority)?;
        guard_pair(arena.view(key), arena.view(child), charge)?;
    }
    if count <= 1 {
        return Ok(None);
    }
    let Some(orders) = arena.orders else {
        return Ok(None);
    };
    match value {
        Ref::Current(value) => Ok(Some(
            orders
                .current_record(value.map_id().ok_or_else(authority)?, count, charge)?
                .0,
        )),
        Ref::Map(id) => Ok(Some(
            orders
                .merged_record(u32::try_from(id).map_err(|_| overflow())?, count, charge)?
                .0,
        )),
        Ref::Frame(_) => Ok(None),
    }
}

pub(super) fn row<'input>(
    arena: &Arena<'input>,
    value: Ref<'input>,
    index: usize,
) -> Option<(Ref<'input>, Ref<'input>)> {
    match value {
        Ref::Current(value) => {
            let (k, v) = value.row(index)?;
            Some((Ref::Current(k), Ref::Current(v)))
        }
        Ref::Frame(value) => {
            let (k, v) = value.row(index)?;
            Some((Ref::Frame(k), Ref::Frame(v)))
        }
        Ref::Map(id) => {
            let map = arena.maps.get(id)?;
            if index >= map.len {
                return None;
            }
            let id = *arena.order.get(map.order.checked_add(index)?)?;
            let row = arena.rows.get(id)?;
            Some((row.key, row.value))
        }
    }
}
