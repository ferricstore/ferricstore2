//! Visible descriptor admission and paid borrowed map lookups.

use super::{Error, OriginalPolicyMapKey as Key, OriginalPolicyMapValue as Value};
use super::{PolicySchemaCharge, PolicySchemaError, charge_schema};

pub(super) fn preflight(
    policy: &[(Key<'_>, Value<'_>)],
    charge: &mut PolicySchemaCharge,
) -> Result<(), Error> {
    let mut measured = (0_u64, 0_u64);
    visit_descriptor(0, &mut measured.0, charge)?;
    preflight_entries(policy, 0, &mut measured, charge)
}

fn preflight_entries(
    entries: &[(Key<'_>, Value<'_>)],
    depth: usize,
    measured: &mut (u64, u64),
    charge: &mut PolicySchemaCharge,
) -> Result<(), Error> {
    let child_depth = depth
        .checked_add(1)
        .ok_or(Error::Schema(PolicySchemaError::LengthOverflow))?;
    let mut queried = 0_u8;
    for &(key, value) in entries {
        visit_descriptor(child_depth, &mut measured.0, charge)?;
        if let Some(name) = key_name(key) {
            let length = u64::try_from(name.len())
                .map_err(|_| Error::Schema(PolicySchemaError::LengthOverflow))?;
            measured.1 = measured
                .1
                .checked_add(length)
                .ok_or(Error::Schema(PolicySchemaError::LengthOverflow))?;
            if measured.1 > 262_144 {
                return Err(Error::ByteLimit);
            }
            charge_schema(charge, length, 0).map_err(Error::Schema)?;
            if let Some(bit) = queried_key_bit(key, name, charge)? {
                if queried & bit != 0 {
                    return Err(Error::Malformed);
                }
                queried |= bit;
            }
        }
        visit_descriptor(child_depth, &mut measured.0, charge)?;
        if let Value::Map(child) = value {
            // The map is this value's already-paid descriptor, not a new root.
            preflight_entries(child, child_depth, measured, charge)?;
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

fn queried_key_bit(
    key: Key<'_>,
    name: &[u8],
    charge: &mut PolicySchemaCharge,
) -> Result<Option<u8>, Error> {
    for (target, bit) in [
        (b"retry".as_slice(), 1_u8),
        (b"states".as_slice(), 2),
        (b"max_attempts".as_slice(), 4),
    ] {
        if compare_name(name, target, charge)? {
            return Ok(Some(if matches!(key, Key::Atom(_)) {
                bit
            } else {
                bit << 3
            }));
        }
    }
    Ok(None)
}

pub(super) fn select_top_field<'input>(
    entries: &[(Key<'input>, Value<'input>)],
    target: &[u8],
    charge: &mut PolicySchemaCharge,
) -> Result<Option<Value<'input>>, Error> {
    select_field(entries, target, false, charge)
}

pub(super) fn select_state_retry_map<'input>(
    entries: &[(Key<'input>, Value<'input>)],
    charge: &mut PolicySchemaCharge,
) -> Result<Option<&'input [(Key<'input>, Value<'input>)]>, Error> {
    Ok(match select_field(entries, b"retry", true, charge)? {
        Some(Value::Map(map)) => Some(map),
        _ => None,
    })
}

fn select_field<'input>(
    entries: &[(Key<'input>, Value<'input>)],
    target: &[u8],
    maps_only: bool,
    charge: &mut PolicySchemaCharge,
) -> Result<Option<Value<'input>>, Error> {
    charge_schema(charge, 128, 0).map_err(Error::Schema)?;
    let mut binary = None;
    for &(key, value) in entries {
        charge_visit(charge, key_name(key).map_or(0, <[u8]>::len))?;
        let Some(name) = key_name(key) else {
            continue;
        };
        if compare_name(name, target, charge)? {
            charge_schema(charge, 128, 0).map_err(Error::Schema)?;
            if maps_only && !matches!(value, Value::Map(_)) {
                continue;
            }
            if matches!(key, Key::Atom(_)) {
                return Ok(Some(value));
            }
            binary = Some(value);
        }
    }
    Ok(binary)
}

pub(super) fn contains_old_key(
    entries: &[(Key<'_>, Value<'_>)],
    charge: &mut PolicySchemaCharge,
) -> Result<bool, Error> {
    charge_schema(charge, 128, 0).map_err(Error::Schema)?;
    for &(key, _) in entries {
        charge_visit(charge, key_name(key).map_or(0, <[u8]>::len))?;
        if let Some(name) = key_name(key)
            && compare_name(name, b"max_attempts", charge)?
        {
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

fn compare_name(
    name: &[u8],
    target: &[u8],
    charge: &mut PolicySchemaCharge,
) -> Result<bool, Error> {
    let length = name
        .len()
        .checked_add(target.len())
        .ok_or(Error::Schema(PolicySchemaError::LengthOverflow))?;
    charge_visit(charge, length)?;
    Ok(name == target)
}

fn charge_visit(charge: &mut PolicySchemaCharge, length: usize) -> Result<(), Error> {
    let work = u64::try_from(length)
        .ok()
        .and_then(|n| n.checked_add(128))
        .ok_or(Error::Schema(PolicySchemaError::LengthOverflow))?;
    charge_schema(charge, work, 0).map_err(Error::Schema)
}
