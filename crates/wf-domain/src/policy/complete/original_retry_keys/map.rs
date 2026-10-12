//! One paid source-selector algorithm over descriptors and validated raw spans.

use super::{Error, OriginalPolicyMapKey as Key, OriginalPolicyMapValue as Value};
use super::{PolicySchemaCharge, PolicySchemaError, PolicySchemaLimits, charge_schema};

pub(in crate::policy::complete) trait OriginalMapVisit: Copy {
    type Key: Copy;
    fn len(self) -> usize;
    fn key(self, previous: Option<Self::Key>) -> Self::Key;
    fn value_map(self, key: Self::Key) -> Option<Self>;
    fn name_len(self, key: Self::Key) -> Option<usize>;
    fn is_named(self, key: Self::Key) -> bool;
    fn is_atom(self, key: Self::Key) -> bool;
    fn equals_ascii(self, key: Self::Key, target: &[u8]) -> bool;
}

impl<'a> OriginalMapVisit for &'a [(Key<'a>, Value<'a>)] {
    type Key = (usize, Key<'a>);
    fn len(self) -> usize {
        <[(Key<'a>, Value<'a>)]>::len(self)
    }
    fn key(self, previous: Option<Self::Key>) -> Self::Key {
        let index = previous.map_or(0, |(index, _)| index + 1);
        (index, self[index].0)
    }
    fn value_map(self, key: Self::Key) -> Option<Self> {
        if let Value::Map(map) = self[key.0].1 {
            Some(map)
        } else {
            None
        }
    }
    fn name_len(self, key: Self::Key) -> Option<usize> {
        key_name(key.1).map(<[u8]>::len)
    }
    fn is_named(self, key: Self::Key) -> bool {
        key_name(key.1).is_some()
    }
    fn is_atom(self, key: Self::Key) -> bool {
        matches!(key.1, Key::Atom(_))
    }
    fn equals_ascii(self, key: Self::Key, target: &[u8]) -> bool {
        key_name(key.1) == Some(target)
    }
}

pub(in crate::policy::complete) fn run<M: OriginalMapVisit>(
    policy: M,
    limits: PolicySchemaLimits,
    name_cap: u64,
    charge: &mut PolicySchemaCharge,
) -> Result<bool, Error> {
    if charge.profile() != limits {
        return Err(Error::Schema(PolicySchemaError::ProfileMismatch));
    }
    charge_schema(charge, 128, 0).map_err(Error::Schema)?;
    let mut measured = (0_u64, 0_u64);
    visit_descriptor(0, &mut measured.0, charge)?;
    preflight_entries(policy, 0, &mut measured, name_cap, charge)?;
    charge_schema(charge, 0, 128).map_err(Error::Schema)?;
    if let Some(retry) = select_field(policy, b"retry", false, charge)?
        && let Some(map) = policy.value_map(retry)
        && contains_old_key(map, charge)?
    {
        return Ok(true);
    }
    if let Some(states) = select_field(policy, b"states", false, charge)?
        && let Some(states) = policy.value_map(states)
    {
        charge_schema(charge, 128, 0).map_err(Error::Schema)?;
        let mut previous = None;
        for _ in 0..states.len() {
            charge_schema(charge, 128, 0).map_err(Error::Schema)?;
            let key = states.key(previous);
            previous = Some(key);
            charge_schema(charge, 128, 0).map_err(Error::Schema)?;
            if let Some(state) = states.value_map(key)
                && let Some(retry) = select_field(state, b"retry", true, charge)?
                && let Some(map) = state.value_map(retry)
                && contains_old_key(map, charge)?
            {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

fn preflight_entries<M: OriginalMapVisit>(
    entries: M,
    depth: usize,
    measured: &mut (u64, u64),
    name_cap: u64,
    charge: &mut PolicySchemaCharge,
) -> Result<(), Error> {
    let child_depth = depth
        .checked_add(1)
        .ok_or(Error::Schema(PolicySchemaError::LengthOverflow))?;
    let mut queried = 0_u8;
    let mut previous = None;
    for _ in 0..entries.len() {
        visit_descriptor(child_depth, &mut measured.0, charge)?;
        let key = entries.key(previous);
        previous = Some(key);
        if let Some(length) = entries.name_len(key) {
            let length = u64::try_from(length)
                .map_err(|_| Error::Schema(PolicySchemaError::LengthOverflow))?;
            measured.1 = measured
                .1
                .checked_add(length)
                .ok_or(Error::Schema(PolicySchemaError::LengthOverflow))?;
            if measured.1 > name_cap {
                return Err(Error::ByteLimit);
            }
            charge_schema(charge, length, 0).map_err(Error::Schema)?;
            if let Some(bit) = queried_key_bit(entries, key, charge)? {
                if queried & bit != 0 {
                    return Err(Error::Malformed);
                }
                queried |= bit;
            }
        }
        visit_descriptor(child_depth, &mut measured.0, charge)?;
        if let Some(child) = entries.value_map(key) {
            preflight_entries(child, child_depth, measured, name_cap, charge)?;
        }
    }
    Ok(())
}

fn visit_descriptor(
    depth: usize,
    nodes: &mut u64,
    charge: &mut PolicySchemaCharge,
) -> Result<(), Error> {
    charge_schema(charge, 128, 0).map_err(Error::Schema)?;
    *nodes = nodes
        .checked_add(1)
        .ok_or(Error::Schema(PolicySchemaError::LengthOverflow))?;
    if depth > 64 || *nodes > 4_096 {
        return Err(Error::ItemLimit);
    }
    Ok(())
}

fn queried_key_bit<M: OriginalMapVisit>(
    map: M,
    key: M::Key,
    charge: &mut PolicySchemaCharge,
) -> Result<Option<u8>, Error> {
    for (target, bit) in [
        (b"retry".as_slice(), 1_u8),
        (b"states".as_slice(), 2),
        (b"max_attempts".as_slice(), 4),
    ] {
        if compare_name(map, key, target, charge)? {
            return Ok(Some(if map.is_atom(key) { bit } else { bit << 3 }));
        }
    }
    Ok(None)
}

fn select_field<M: OriginalMapVisit>(
    entries: M,
    target: &[u8],
    maps_only: bool,
    charge: &mut PolicySchemaCharge,
) -> Result<Option<M::Key>, Error> {
    charge_schema(charge, 128, 0).map_err(Error::Schema)?;
    let mut binary = None;
    let mut previous = None;
    for _ in 0..entries.len() {
        let key = entries.key(previous);
        previous = Some(key);
        charge_visit(charge, entries.name_len(key).unwrap_or(0))?;
        if !entries.is_named(key) {
            continue;
        }
        if compare_name(entries, key, target, charge)? {
            charge_schema(charge, 128, 0).map_err(Error::Schema)?;
            if maps_only && entries.value_map(key).is_none() {
                continue;
            }
            if entries.is_atom(key) {
                return Ok(Some(key));
            }
            binary = Some(key);
        }
    }
    Ok(binary)
}

fn contains_old_key<M: OriginalMapVisit>(
    entries: M,
    charge: &mut PolicySchemaCharge,
) -> Result<bool, Error> {
    charge_schema(charge, 128, 0).map_err(Error::Schema)?;
    let mut previous = None;
    for _ in 0..entries.len() {
        let key = entries.key(previous);
        previous = Some(key);
        charge_visit(charge, entries.name_len(key).unwrap_or(0))?;
        if entries.is_named(key) && compare_name(entries, key, b"max_attempts", charge)? {
            return Ok(true);
        }
    }
    Ok(false)
}

fn key_name(key: Key<'_>) -> Option<&[u8]> {
    match key {
        Key::Atom(text) => Some(text.as_bytes()),
        Key::Binary(bytes) => Some(bytes),
        Key::Other => None,
    }
}

fn compare_name<M: OriginalMapVisit>(
    map: M,
    key: M::Key,
    target: &[u8],
    charge: &mut PolicySchemaCharge,
) -> Result<bool, Error> {
    let length = map
        .name_len(key)
        .unwrap_or(0)
        .checked_add(target.len())
        .ok_or(Error::Schema(PolicySchemaError::LengthOverflow))?;
    charge_visit(charge, length)?;
    Ok(map.equals_ascii(key, target))
}

fn charge_visit(charge: &mut PolicySchemaCharge, length: usize) -> Result<(), Error> {
    let work = u64::try_from(length)
        .ok()
        .and_then(|n| n.checked_add(128))
        .ok_or(Error::Schema(PolicySchemaError::LengthOverflow))?;
    charge_schema(charge, work, 0).map_err(Error::Schema)
}
