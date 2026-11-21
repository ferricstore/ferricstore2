//! The image of a Flow's fields beyond its lifecycle record (ADR-014).
//!
//! Layout: lineage ids as optional binaries, retention, the active-time
//! limit (`0` none, `1` milliseconds, `2` infinity), history caps,
//! attributes and state metadata as [`Term`] maps, named values (count, then
//! name, version and bytes), the payload, result and error versions, then
//! the terminal retention override and removal time as optional integers,
//! then the signal idempotency keys (count, then key and digest), then the
//! child groups as a [`Term`] map of each group's reply form, the error
//! kind, the rewind target, the policy-indexed attribute names (count,
//! then names) and state metadata key, and the effects and governance
//! ledger as an optional binary of their own encoding.

use std::collections::BTreeMap;

use wf_application::commands::flow::Term;
use wf_application::commands::flow::children::ChildGroup;
use wf_application::commands::flow::create::MaxActive;
use wf_application::commands::flow::effect::Governance;
use wf_application::commands::flow::meta::{Attributes, StateMeta};
use wf_application::commands::flow::record::{FlowMeta, NamedValue, Retained, RetainedMap};

use super::super::command::{Malformed, Reader, bin, optional_bin, optional_u64};

fn term(out: &mut Vec<u8>, value: &Term) -> Option<()> {
    value.encode(out)
}

pub(super) fn encode(out: &mut Vec<u8>, meta: &FlowMeta) -> Option<()> {
    for lineage in [
        &meta.parent_flow_id,
        &meta.parent_partition_key,
        &meta.root_flow_id,
        &meta.correlation_id,
    ] {
        optional_bin(out, lineage.as_deref())?;
    }
    out.extend_from_slice(&meta.retention_ttl_ms.to_be_bytes());
    match meta.max_active_ms {
        None => out.push(0),
        Some(MaxActive::Ms(value)) => {
            out.push(1);
            out.extend_from_slice(&value.to_be_bytes());
        }
        Some(MaxActive::Infinity) => out.push(2),
    }
    out.extend_from_slice(&meta.history_hot_max_events.to_be_bytes());
    out.extend_from_slice(&meta.history_max_events.to_be_bytes());
    let attributes = meta
        .attributes
        .iter()
        .map(|(name, value)| (name.as_bytes().to_vec(), value.clone()))
        .collect();
    term(out, &Term::Map(attributes))?;
    let state_meta = meta
        .state_meta
        .iter()
        .map(|(state, entries)| {
            let entries = entries
                .iter()
                .map(|(name, value)| (name.as_bytes().to_vec(), value.clone()))
                .collect();
            (state.as_bytes().to_vec(), Term::Map(entries))
        })
        .collect();
    term(out, &Term::Map(state_meta))?;
    out.extend_from_slice(&u32::try_from(meta.values.len()).ok()?.to_be_bytes());
    for (name, value) in &meta.values {
        bin(out, name)?;
        out.extend_from_slice(&value.version.to_be_bytes());
        bin(out, &value.bytes)?;
    }
    for version in [
        meta.payload_version,
        meta.result_version,
        meta.error_version,
    ] {
        out.extend_from_slice(&version.to_be_bytes());
    }
    optional_u64(out, meta.ttl_ms);
    optional_u64(out, meta.terminal_until);
    out.extend_from_slice(&u32::try_from(meta.signal_keys.len()).ok()?.to_be_bytes());
    for (key, digest) in &meta.signal_keys {
        bin(out, key)?;
        bin(out, digest)?;
    }
    let groups = meta
        .child_groups
        .iter()
        .map(|(id, group)| (id.clone(), group.term()))
        .collect();
    term(out, &Term::Map(groups))?;
    optional_bin(out, meta.rewound_to.as_deref())?;
    out.extend_from_slice(
        &u32::try_from(meta.indexed_attributes.len())
            .ok()?
            .to_be_bytes(),
    );
    for name in &meta.indexed_attributes {
        bin(out, name.as_bytes())?;
    }
    optional_bin(out, meta.indexed_state_meta.as_deref().map(str::as_bytes))?;
    let governance = match meta.governance.as_deref() {
        Some(governance) => Some(governance.encode()?),
        None => None,
    };
    optional_bin(out, governance.as_deref())?;
    // Attached references (audit gap G9): name and reference each.
    out.extend_from_slice(&u32::try_from(meta.value_refs.len()).ok()?.to_be_bytes());
    for (name, reference) in &meta.value_refs {
        bin(out, name)?;
        bin(out, reference)?;
    }
    // Kept value versions (ADR-016 phase C): kind, name, version, term
    // flag and bytes each.
    out.extend_from_slice(&u32::try_from(meta.retained.len()).ok()?.to_be_bytes());
    for ((kind, name, version), kept) in &meta.retained {
        out.push(*kind);
        bin(out, name)?;
        out.extend_from_slice(&version.to_be_bytes());
        bin(out, &kept.bytes)?;
    }
    Some(())
}

fn read_term(reader: &mut Reader<'_>) -> Result<Term, Malformed> {
    let (value, rest) = Term::decode(reader.0).ok_or(Malformed)?;
    reader.0 = rest;
    Ok(value)
}

fn text_map(value: Term) -> Result<Vec<(String, Term)>, Malformed> {
    let Term::Map(entries) = value else {
        return Err(Malformed);
    };
    entries
        .into_iter()
        .map(|(key, value)| Ok((String::from_utf8(key).map_err(|_| Malformed)?, value)))
        .collect()
}

pub(super) fn decode(reader: &mut Reader<'_>) -> Result<FlowMeta, Malformed> {
    let parent_flow_id = reader.optional_bin()?;
    let parent_partition_key = reader.optional_bin()?;
    let root_flow_id = reader.optional_bin()?;
    let correlation_id = reader.optional_bin()?;
    let retention_ttl_ms = reader.u64()?;
    let max_active_ms = match reader.u8()? {
        0 => None,
        1 => Some(MaxActive::Ms(reader.u64()?)),
        2 => Some(MaxActive::Infinity),
        _ => return Err(Malformed),
    };
    let history_hot_max_events = reader.u64()?;
    let history_max_events = reader.u64()?;
    let attributes: Attributes = text_map(read_term(reader)?)?.into_iter().collect();
    let mut state_meta = StateMeta::new();
    for (state, entries) in text_map(read_term(reader)?)? {
        state_meta.insert(state, text_map(entries)?.into_iter().collect());
    }
    let count = u32::from_be_bytes(reader.array()?);
    let mut values = BTreeMap::new();
    for _ in 0..count {
        let name = reader.bin()?.to_vec();
        let version = reader.u64()?;
        let bytes = reader.bin()?.to_vec();
        values.insert(name, NamedValue { version, bytes });
    }
    Ok(FlowMeta {
        parent_flow_id,
        parent_partition_key,
        root_flow_id,
        correlation_id,
        retention_ttl_ms,
        max_active_ms,
        history_hot_max_events,
        history_max_events,
        attributes,
        state_meta,
        values,
        payload_version: reader.u64()?,
        result_version: reader.u64()?,
        error_version: reader.u64()?,
        ttl_ms: reader.optional_u64()?,
        terminal_until: reader.optional_u64()?,
        signal_keys: {
            let count = u32::from_be_bytes(reader.array()?);
            let mut keys = BTreeMap::new();
            for _ in 0..count {
                let key = reader.bin()?.to_vec();
                keys.insert(key, reader.bin()?.to_vec());
            }
            keys
        },
        child_groups: match read_term(reader)? {
            Term::Map(entries) => entries
                .into_iter()
                .map(|(id, group)| Ok((id, ChildGroup::from_term(&group).ok_or(Malformed)?)))
                .collect::<Result<BTreeMap<_, _>, Malformed>>()?,
            _ => return Err(Malformed),
        },
        rewound_to: reader.optional_bin()?,
        indexed_attributes: {
            let count = u32::from_be_bytes(reader.array()?);
            (0..count)
                .map(|_| String::from_utf8(reader.bin()?.to_vec()).map_err(|_| Malformed))
                .collect::<Result<Vec<_>, Malformed>>()?
        },
        indexed_state_meta: reader
            .optional_bin()?
            .map(|key| String::from_utf8(key).map_err(|_| Malformed))
            .transpose()?,
        governance: reader
            .optional_bin()?
            .map(|bytes| Governance::decode(&bytes).map(Box::new).ok_or(Malformed))
            .transpose()?,
        value_refs: {
            let count = u32::from_be_bytes(reader.array()?);
            let mut refs = BTreeMap::new();
            for _ in 0..count {
                let name = reader.bin()?.to_vec();
                refs.insert(name, reader.bin()?.to_vec());
            }
            refs
        },
        retained: decode_retained(reader)?,
    })
}

/// Kept value versions (ADR-016 phase C).
fn decode_retained(reader: &mut Reader<'_>) -> Result<RetainedMap, Malformed> {
    let count = u32::from_be_bytes(reader.array()?);
    let mut retained = BTreeMap::new();
    for _ in 0..count {
        let kind = reader.u8()?;
        let name = reader.bin()?.to_vec();
        let version = reader.u64()?;
        let bytes = reader.bin()?.to_vec();
        retained.insert((kind, name, version), Retained { bytes });
    }
    Ok(retained)
}
