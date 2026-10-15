//! Generic field resolution without policy business admission.

use crate::retention::ResolvedRetention;

use super::super::flow_input::value::PolicyInputKind as Kind;
use super::super::original_entry::TermPosition;
use super::model::{
    OriginalPolicyReply as Reply, PolicyReplyRef as Ref, PolicyReplyState as State,
};
use super::{
    EncodingError as Error, OriginalPolicyReplyError, PolicySchemaCharge, maps, pay, source,
};

pub(super) fn root<'input>(
    owner: &mut Reply<'input>,
    defaults: ResolvedRetention,
    charge: &mut PolicySchemaCharge,
) -> Result<State<'input>, OriginalPolicyReplyError> {
    let policy = owner.index.as_ref().map(|index| index.cursor().position());
    owner.version =
        lookup(owner, policy, b"version", charge)?.map_or(Ref::StaticAtom("nil"), Ref::Source);
    let maximum = lookup(owner, policy, b"max_active_ms", charge)?;
    owner.max_active = maximum
        .filter(|&position| {
            let value = owner.index.as_ref().map(|index| index.at(position));
            value.is_some_and(|value| value.integer().is_some() || value.big_integer().is_some())
        })
        .map_or(Ref::StaticAtom("nil"), Ref::Source);
    let mut result = State::defaults(defaults);
    let input = lookup(owner, policy, b"retry", charge)?;
    if let Some(backoff) = retry(owner, input, &mut result, charge)? {
        result.backoff = maps::base(
            owner.index.as_ref().ok_or(Error::Malformed)?,
            backoff,
            charge,
        )?;
    }
    let input = lookup(owner, policy, b"retention", charge)?;
    retention(owner, input, &mut result, charge)?;
    source::indexed(owner, charge)?;
    let governance = lookup(owner, policy, b"governance", charge)?;
    pay(charge, 128)?;
    result.governance = map_position(owner, governance).map_or(Ref::StaticAtom("nil"), Ref::Source);
    Ok(result)
}

pub(super) fn state<'input>(
    owner: &Reply<'input>,
    config: Option<TermPosition>,
    name: Ref<'input>,
    ordinal: u32,
    charge: &mut PolicySchemaCharge,
) -> Result<State<'input>, OriginalPolicyReplyError> {
    let mut result = *owner.records.first().ok_or(Error::Malformed)?;
    result.name = name;
    result.source_ordinal = ordinal;
    result.governance = Ref::StaticAtom("nil");
    result.mode = false;
    if let Some(config) = map_position(owner, config) {
        let mode = lookup(owner, Some(config), b"mode", charge)?;
        result.mode = mode.is_some_and(|position| {
            owner
                .index
                .as_ref()
                .is_some_and(|index| index.at(position).atom_bytes() == Some(b"fifo"))
        });
        let input = lookup(owner, Some(config), b"retry", charge)?;
        if let Some(input) = map_position(owner, input)
            && let Some(backoff) = retry(owner, Some(input), &mut result, charge)?
        {
            result.backoff = maps::local(
                owner.index.as_ref().ok_or(Error::Malformed)?,
                result.backoff,
                backoff,
                charge,
            )?;
        }
        let input = lookup(owner, Some(config), b"retention", charge)?;
        if let Some(input) = map_position(owner, input) {
            retention(owner, Some(input), &mut result, charge)?;
        }
        result.governance = lookup(owner, Some(config), b"governance", charge)?
            .map_or(Ref::StaticAtom("nil"), Ref::Source);
    }
    Ok(result)
}

fn retry<'input>(
    owner: &Reply<'input>,
    input: Option<TermPosition>,
    result: &mut State<'input>,
    charge: &mut PolicySchemaCharge,
) -> Result<Option<TermPosition>, OriginalPolicyReplyError> {
    pay(charge, 128)?;
    let Some(input) = map_position(owner, input) else {
        return Ok(None);
    };
    let index = owner.index.as_ref().ok_or(Error::Malformed)?;
    source::guard_map(index, input, charge)?;
    if let Some(value) = source::field(index, input, b"max_retries", charge)? {
        result.max_retries = Ref::Source(value);
    }
    let backoff = source::field(index, input, b"backoff", charge)?;
    if let Some(value) = source::field(index, input, b"exhausted_to", charge)? {
        result.exhausted_to = Ref::Source(value);
    }
    Ok(map_position(owner, backoff))
}

fn retention<'input>(
    owner: &Reply<'input>,
    input: Option<TermPosition>,
    result: &mut State<'input>,
    charge: &mut PolicySchemaCharge,
) -> Result<(), OriginalPolicyReplyError> {
    pay(charge, 128)?;
    let Some(input) = map_position(owner, input) else {
        return Ok(());
    };
    let index = owner.index.as_ref().ok_or(Error::Malformed)?;
    source::guard_map(index, input, charge)?;
    if let Some(value) = source::field(index, input, b"ttl_ms", charge)? {
        result.ttl_ms = Ref::Source(value);
    }
    if let Some(value) = source::field(index, input, b"history_max_events", charge)? {
        result.history_max_events = Ref::Source(value);
    }
    Ok(())
}

fn lookup(
    owner: &Reply<'_>,
    map: Option<TermPosition>,
    name: &[u8],
    charge: &mut PolicySchemaCharge,
) -> Result<Option<TermPosition>, Error> {
    if let (Some(index), Some(map)) = (&owner.index, map) {
        source::field(index, map, name, charge)
    } else {
        Ok(None)
    }
}

fn map_position(owner: &Reply<'_>, position: Option<TermPosition>) -> Option<TermPosition> {
    position.filter(|&position| {
        owner
            .index
            .as_ref()
            .is_some_and(|index| index.at(position).kind() == Kind::Map)
    })
}

pub(super) fn false_or_nil(owner: &Reply<'_>, value: Ref<'_>) -> bool {
    match value {
        Ref::StaticAtom("nil" | "false") => true,
        Ref::Source(position) => owner.index.as_ref().is_some_and(|index| {
            matches!(index.at(position).atom_bytes(), Some(b"nil" | b"false"))
        }),
        _ => false,
    }
}
