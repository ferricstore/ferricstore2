//! Persistent default/type/local backoff unions and constant-progress maps.

use super::super::original_entry::TermPosition;
use super::super::original_entry::indexed::OriginalPolicyCurrentIndex;
use super::model::{
    OriginalPolicyReply as Reply, PolicyReplyBackoff as Backoff, PolicyReplyMap as Map,
    PolicyReplyRef as Ref,
};
use super::{EncodingError as Error, PolicySchemaCharge, overflow, pay, source};

const DEFAULTS: [&[u8]; 4] = [b"kind", b"base_ms", b"max_ms", b"jitter_pct"];
const ALL: [&str; 10] = [
    "type",
    "generation",
    "version",
    "max_active_ms",
    "retry",
    "retention",
    "indexed_attributes",
    "indexed_state_meta",
    "governance",
    "states",
];
const SELECTED: [&str; 11] = [
    "type",
    "state",
    "generation",
    "version",
    "mode",
    "max_active_ms",
    "retry",
    "retention",
    "indexed_attributes",
    "indexed_state_meta",
    "governance",
];
const STATE: [&str; 5] = ["mode", "max_active_ms", "retry", "retention", "governance"];
const RETRY: [&str; 3] = ["max_retries", "backoff", "exhausted_to"];
const RETENTION: [&str; 2] = ["ttl_ms", "history_max_events"];

pub(super) fn base(
    index: &OriginalPolicyCurrentIndex<'_, '_>,
    map: TermPosition,
    charge: &mut PolicySchemaCharge,
) -> Result<Backoff, Error> {
    let mut result = Backoff {
        type_map: Some(map),
        local_map: None,
        default_mask: 0,
        len: 4,
    };
    count(index, map, &mut result, charge)?;
    Ok(result)
}

pub(super) fn local(
    index: &OriginalPolicyCurrentIndex<'_, '_>,
    mut result: Backoff,
    map: TermPosition,
    charge: &mut PolicySchemaCharge,
) -> Result<Backoff, Error> {
    result.local_map = Some(map);
    count(index, map, &mut result, charge)?;
    Ok(result)
}

fn count(
    index: &OriginalPolicyCurrentIndex<'_, '_>,
    map: TermPosition,
    result: &mut Backoff,
    charge: &mut PolicySchemaCharge,
) -> Result<(), Error> {
    for row in 0..index.at(map).len() {
        pay(charge, 128)?;
        let (key, _) = index.at(map).row(row).ok_or(Error::Malformed)?;
        if let Some(bit) = default_key(key.atom_bytes(), charge)? {
            if result.default_mask & bit == 0 {
                pay(charge, 128)?;
                result.default_mask |= bit;
            }
        } else {
            let old = if result.local_map.is_some() {
                result
                    .type_map
                    .filter(|&value| index.at(value).len() != 0)
                    .map(|value| source::exact(index, value, key.position(), charge))
                    .transpose()?
                    .flatten()
            } else {
                None
            };
            if old.is_none() {
                pay(charge, 128)?;
                result.len = result.len.checked_add(1).ok_or_else(overflow)?;
            }
        }
    }
    Ok(())
}

fn default_key(bytes: Option<&[u8]>, charge: &mut PolicySchemaCharge) -> Result<Option<u8>, Error> {
    let Some(bytes) = bytes else {
        return Ok(None);
    };
    for (i, key) in DEFAULTS.iter().enumerate() {
        compare(key, bytes, charge)?;
        if *key == bytes {
            return Ok(Some(1 << i));
        }
    }
    Ok(None)
}

pub(super) fn len(owner: &Reply<'_>, map: Map) -> Option<usize> {
    Some(match map {
        Map::Root => {
            if owner.requested_state.is_some() {
                11
            } else {
                10
            }
        }
        Map::States => owner.order.len(),
        Map::State(_) => 5,
        Map::Retry(_) => 3,
        Map::Retention(_) => 2,
        Map::Backoff(slot) => owner.records.get(slot as usize)?.backoff.len,
        Map::Source(position) => owner.index.as_ref()?.at(position).len(),
    })
}

pub(super) fn field<'input>(
    owner: &Reply<'input>,
    map: Map,
    name: &[u8],
    charge: &mut PolicySchemaCharge,
) -> Result<Option<Ref<'input>>, Error> {
    match map {
        Map::Source(position) => {
            let index = owner.index.as_ref().ok_or(Error::Malformed)?;
            Ok(source::field(index, position, name, charge)?.map(Ref::Source))
        }
        Map::Backoff(slot) => {
            let record = owner.records.get(slot as usize).ok_or(Error::Malformed)?;
            let index = owner.index.as_ref();
            for map in [record.backoff.local_map, record.backoff.type_map]
                .into_iter()
                .flatten()
            {
                if let Some(value) =
                    source::field(index.ok_or(Error::Malformed)?, map, name, charge)?
                {
                    return Ok(Some(Ref::Source(value)));
                }
            }
            for (i, key) in DEFAULTS.iter().enumerate() {
                compare(key, name, charge)?;
                if *key == name {
                    return Ok(Some(default_value(i)));
                }
            }
            Ok(None)
        }
        Map::States => {
            for &slot in &owner.order {
                pay(charge, 128)?;
                let index = owner.index.as_ref().ok_or(Error::Malformed)?;
                let state = owner.records.get(slot as usize).ok_or(Error::Malformed)?;
                let Ref::Source(position) = state.name else {
                    return Err(Error::Malformed);
                };
                if let Some(bytes) = index.at(position).atom_bytes() {
                    compare(bytes, name, charge)?;
                    if bytes == name {
                        return Ok(Some(Ref::Map(Map::State(slot))));
                    }
                } else {
                    pay(charge, 128)?;
                }
            }
            Ok(None)
        }
        _ => {
            for i in 0..len(owner, map).ok_or(Error::Malformed)? {
                let (key, value) = fixed(owner, map, i).ok_or(Error::Malformed)?;
                compare(key.as_bytes(), name, charge)?;
                if key.as_bytes() == name {
                    return Ok(Some(value));
                }
            }
            Ok(None)
        }
    }
}

pub(super) fn next<'input>(
    owner: &Reply<'input>,
    map: Map,
    phase: &mut usize,
    position: &mut usize,
    charge: &mut PolicySchemaCharge,
) -> Result<Option<(Ref<'input>, Ref<'input>)>, Error> {
    if let Map::Backoff(slot) = map {
        return backoff_next(owner, slot, phase, position, charge);
    }
    if *position >= len(owner, map).ok_or(Error::Malformed)? {
        return Ok(None);
    }
    pay(charge, 128)?;
    let row = match map {
        Map::States => {
            let id = *owner.order.get(*position).ok_or(Error::Malformed)?;
            let state = owner.records.get(id as usize).ok_or(Error::Malformed)?;
            (state.name, Ref::Map(Map::State(id)))
        }
        Map::Source(source) => {
            let (key, value) = owner
                .index
                .as_ref()
                .ok_or(Error::Malformed)?
                .at(source)
                .row(*position)
                .ok_or(Error::Malformed)?;
            (Ref::Source(key.position()), Ref::Source(value.position()))
        }
        _ => {
            let (key, value) = fixed(owner, map, *position).ok_or(Error::Malformed)?;
            (Ref::StaticAtom(key), value)
        }
    };
    *position += 1;
    Ok(Some(row))
}

fn backoff_next<'input>(
    owner: &Reply<'input>,
    slot: u32,
    phase: &mut usize,
    position: &mut usize,
    charge: &mut PolicySchemaCharge,
) -> Result<Option<(Ref<'input>, Ref<'input>)>, Error> {
    let data = owner
        .records
        .get(slot as usize)
        .ok_or(Error::Malformed)?
        .backoff;
    loop {
        if *phase == 0 {
            while *position < 4 {
                pay(charge, 128)?;
                let id = *position;
                *position += 1;
                if data.default_mask & (1 << id) == 0 {
                    return Ok(Some((Ref::StaticAtom(default_name(id)), default_value(id))));
                }
            }
        } else if *phase <= 2 {
            let map = if *phase == 1 {
                data.type_map
            } else {
                data.local_map
            };
            if let Some(map) = map {
                let index = owner.index.as_ref().ok_or(Error::Malformed)?;
                while *position < index.at(map).len() {
                    pay(charge, 128)?;
                    let (key, value) = index.at(map).row(*position).ok_or(Error::Malformed)?;
                    *position += 1;
                    if *phase == 1
                        && let Some(local) = data.local_map
                        && index.at(local).len() != 0
                        && source::exact(index, local, key.position(), charge)?.is_some()
                    {
                        continue;
                    }
                    return Ok(Some((
                        Ref::Source(key.position()),
                        Ref::Source(value.position()),
                    )));
                }
            }
        } else {
            return Ok(None);
        }
        *phase += 1;
        *position = 0;
    }
}

fn fixed<'input>(
    owner: &Reply<'input>,
    map: Map,
    position: usize,
) -> Option<(&'static str, Ref<'input>)> {
    let slot = match map {
        Map::Root => owner.selected,
        Map::State(n) | Map::Retry(n) | Map::Retention(n) => n,
        _ => return None,
    };
    let state = owner.records.get(slot as usize)?;
    let keys: &[&str] = match map {
        Map::Root if owner.requested_state.is_some() => &SELECTED,
        Map::Root => &ALL,
        Map::State(_) => &STATE,
        Map::Retry(_) => &RETRY,
        Map::Retention(_) => &RETENTION,
        _ => return None,
    };
    let key = *keys.get(position)?;
    let value = match key {
        "type" => Ref::Bytes(owner.requested_type),
        "state" => Ref::Bytes(owner.requested_state?),
        "generation" => Ref::Unsigned(owner.generation()),
        "version" => owner.version,
        "max_active_ms" => owner.max_active,
        "mode" => Ref::StaticAtom(if state.mode { "fifo" } else { "parallel" }),
        "retry" => Ref::Map(Map::Retry(slot)),
        "retention" => Ref::Map(Map::Retention(slot)),
        "governance" => state.governance,
        "states" => Ref::Map(Map::States),
        "indexed_attributes" => Ref::Attributes(0),
        "indexed_state_meta" if owner.coordinates[3].1 != 0 => Ref::Inline(3),
        "indexed_state_meta" => Ref::StaticAtom("nil"),
        "max_retries" => state.max_retries,
        "backoff" => Ref::Map(Map::Backoff(slot)),
        "exhausted_to" => state.exhausted_to,
        "ttl_ms" => state.ttl_ms,
        "history_max_events" => state.history_max_events,
        _ => return None,
    };
    Some((key, value))
}

fn default_name(index: usize) -> &'static str {
    match index {
        0 => "kind",
        1 => "base_ms",
        2 => "max_ms",
        _ => "jitter_pct",
    }
}

fn default_value(index: usize) -> Ref<'static> {
    match index {
        0 => Ref::StaticAtom("exponential"),
        1 => Ref::Unsigned(1_000),
        2 => Ref::Unsigned(30_000),
        _ => Ref::Unsigned(20),
    }
}

fn compare(left: &[u8], right: &[u8], charge: &mut PolicySchemaCharge) -> Result<(), Error> {
    let length = left
        .len()
        .checked_add(right.len())
        .and_then(|n| n.checked_add(128))
        .ok_or_else(overflow)?;
    pay(charge, u64::try_from(length).map_err(|_| overflow())?)
}
