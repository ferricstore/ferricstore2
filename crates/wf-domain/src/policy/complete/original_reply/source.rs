//! Fresh same-index lookups, reached capture binding and bounded indexed names.

use std::cmp::Ordering;

use super::model::{
    OriginalPolicyReply as Reply, OriginalPolicyReplyContext as Context,
    OriginalPolicyReplyError as ReplyError,
};
use super::{overflow, pay, peak};
use crate::policy::complete::flow_input::value::{PolicyInputCursor, PolicyInputKind as Kind};
use crate::policy::complete::indexed_input::{borrow_attributes, borrow_metadata};
use crate::policy::complete::original_entry::{
    TermPosition as Position, indexed::OriginalPolicyCurrentIndex as Index,
};
use crate::policy::complete::{
    CompletePolicyEncodingError as Error, PolicySchemaCharge, PolicySchemaError, PolicySchemaLimits,
};

/// Measures a freshly decoded portable GET reply without building that reply.
///
/// The returned index/catalog, reply metadata and reached-map rank peak exclude
/// caller query backing. A later presenter performs its own separately paid
/// decode; this arithmetic result grants no retained-index or authority credit.
/// # Errors
/// Preserves malformed envelope, portable profile, schema and allocation errors.
pub fn original_policy_get_reply_projection_bytes(
    encoded: Option<&[u8]>,
    state: Option<&[u8]>,
    schema: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<usize, Error> {
    super::entry(schema, charge)?;
    let mut index = encoded
        .map(|raw| {
            super::super::original_entry::indexed::decode_portable_retained(raw, 0, schema, charge)?
                .ok_or(Error::Malformed)
        })
        .transpose()?;
    let mut bytes = 0_usize;
    if let Some(index) = &mut index {
        bytes = index.owner_bound()?;
        index.build_catalog(bytes, charge)?;
        bytes = bytes
            .checked_add(index.catalog_bound()?)
            .ok_or_else(overflow)?;
    }
    let states = index
        .as_ref()
        .map(|index| field(index, index.cursor().position(), b"states", charge))
        .transpose()?
        .flatten();
    let capacity = super::capacity(index.as_ref(), state, states)?;
    let depth = index.as_ref().map_or(0, Index::depth_bound);
    let metadata = capacity
        .checked_mul(520)
        .and_then(|n| n.checked_add(1024))
        .and_then(|n| depth.checked_mul(64).and_then(|depth| n.checked_add(depth)))
        .ok_or_else(overflow)?;
    bytes = bytes.checked_add(metadata).ok_or_else(overflow)?;
    if state.is_none()
        && let (Some(index), Some(states)) = (&index, states)
    {
        let states = index.at(states);
        if states.kind() == Kind::Map && states.len() > 1 {
            bytes = states
                .len()
                .checked_mul(72)
                .and_then(|n| n.checked_add(64))
                .and_then(|n| bytes.checked_add(n))
                .ok_or_else(overflow)?;
        }
    }
    Ok(bytes)
}

pub(super) fn field(
    index: &Index<'_, '_>,
    map: Position,
    name: &[u8],
    charge: &mut PolicySchemaCharge,
) -> Result<Option<Position>, Error> {
    let map = index.at(map);
    let (mut low, mut high) = (0, map.len());
    while low < high {
        let middle = low + (high - low) / 2;
        pay(charge, 128)?;
        let (key, value) = map.row(middle).ok_or(Error::Malformed)?;
        let order = if let Some(bytes) = key.atom_bytes() {
            pay(charge, fee(bytes.len(), name.len())?)?;
            bytes.cmp(name)
        } else {
            pay(charge, 128)?;
            Ordering::Greater
        };
        match order {
            Ordering::Equal => return Ok(Some(value.position())),
            Ordering::Less => low = middle + 1,
            Ordering::Greater => high = middle,
        }
    }
    Ok(None)
}

pub(super) fn exact(
    index: &Index<'_, '_>,
    map: Position,
    key: Position,
    charge: &mut PolicySchemaCharge,
) -> Result<Option<Position>, Error> {
    let map = index.at(map);
    let (mut low, mut high) = (0, map.len());
    while low < high {
        let middle = low + (high - low) / 2;
        pay(charge, 128)?;
        let (candidate, value) = map.row(middle).ok_or(Error::Malformed)?;
        match index.compare_positions(candidate.position(), key, charge)? {
            Ordering::Equal => return Ok(Some(value.position())),
            Ordering::Less => low = middle + 1,
            Ordering::Greater => high = middle,
        }
    }
    Ok(None)
}

pub(super) fn selected(
    index: &Index<'_, '_>,
    states: Position,
    query: &[u8],
    charge: &mut PolicySchemaCharge,
) -> Result<Option<Position>, Error> {
    pay(charge, 128)?;
    let states = index.at(states);
    if states.kind() != Kind::Map {
        return Ok(None);
    }
    for row in 0..states.len() {
        pay(charge, 128)?;
        let (key, value) = states.row(row).ok_or(Error::Malformed)?;
        let Some(bytes) = key.binary_bytes() else {
            continue;
        };
        pay(charge, 16)?;
        if bytes.len() == query.len() {
            pay(charge, fee(bytes.len(), query.len())?)?;
            if bytes == query {
                return Ok(Some(value.position()));
            }
        }
    }
    Ok(None)
}

pub(super) fn guard_map(
    index: &Index<'_, '_>,
    map: Position,
    charge: &mut PolicySchemaCharge,
) -> Result<(), ReplyError> {
    if let Some(marker) = field(index, map, b"__struct__", charge)?
        && index.at(marker).atom_bytes().is_some()
    {
        return Err(Error::UnsupportedTag.into());
    }
    Ok(())
}

pub(super) fn native_order<'input>(
    index: &Index<'input, 'input>,
    states: Position,
    context: &Context<'input>,
    resident: usize,
    charge: &mut PolicySchemaCharge,
) -> Result<(Option<&'input [u32]>, usize), ReplyError> {
    let states = index.at(states);
    let count = states.len();
    if count <= 1 {
        return Ok((None, 0));
    }
    pay(charge, 128)?;
    let binding = context.encoded_binding.ok_or_else(authority)?;
    let encoded = index.entry().encoded();
    pay(charge, fee(encoded.len(), binding.len())?)?;
    if binding != encoded {
        return Err(authority().into());
    }
    let (id, forward, inverse) = context.states_order.ok_or_else(authority)?;
    pay(charge, 16)?;
    if states.map_id() != Some(id) {
        return Err(authority().into());
    }
    pay(charge, 16)?;
    if forward.len() != count {
        return Err(authority().into());
    }
    pay(charge, 16)?;
    if inverse.len() != count {
        return Err(authority().into());
    }
    let backing = count
        .checked_mul(8)
        .and_then(|n| n.checked_add(64))
        .ok_or_else(overflow)?;
    peak(charge, resident.checked_add(backing).ok_or_else(overflow)?)?;
    for (ordinal, rank_word) in forward.iter().enumerate() {
        pay(charge, 4)?;
        let rank = usize::try_from(*rank_word).map_err(|_| overflow())?;
        pay(charge, 16)?;
        if rank >= count {
            return Err(authority().into());
        }
        pay(charge, 4)?;
        if usize::try_from(inverse[rank]).map_err(|_| overflow())? != ordinal {
            return Err(authority().into());
        }
    }
    Ok((Some(forward), backing))
}

pub(super) fn indexed(
    owner: &mut Reply<'_>,
    charge: &mut PolicySchemaCharge,
) -> Result<(), ReplyError> {
    pay(charge, 128)?;
    let Some(index) = owner.index.as_ref() else {
        pay(charge, 128)?;
        return Ok(());
    };
    let policy = index.cursor().position();
    if let Some(value) = field(index, policy, b"indexed_attributes", charge)? {
        let cursor = index.at(value);
        if matches!(cursor.kind(), Kind::List | Kind::ImproperList) {
            let mut names = [None; 3];
            let normalized =
                borrow_attributes(Some(PolicyInputCursor::Current(cursor)), &mut names, charge)
                    .map_err(indexed_error)?;
            if let Ok(Some(count)) = normalized {
                for (slot, bytes) in names.into_iter().take(count).flatten().enumerate() {
                    copy_name(
                        bytes,
                        slot,
                        &mut owner.inline,
                        &mut owner.coordinates,
                        charge,
                    )?;
                }
                owner.attributes = count;
            }
        }
    }
    pay(charge, 128)?;
    if let Some(value) = field(index, policy, b"indexed_state_meta", charge)?
        && let Ok(Some(bytes)) =
            borrow_metadata(PolicyInputCursor::Current(index.at(value)), 0, charge)
                .map_err(indexed_error)?
    {
        copy_name(bytes, 3, &mut owner.inline, &mut owner.coordinates, charge)?;
    }
    Ok(())
}

fn copy_name(
    bytes: &[u8],
    slot: usize,
    inline: &mut [u8; 256],
    coordinates: &mut [(usize, usize); 4],
    charge: &mut PolicySchemaCharge,
) -> Result<(), Error> {
    let start = slot.checked_mul(64).ok_or_else(overflow)?;
    let end = start.checked_add(bytes.len()).ok_or_else(overflow)?;
    pay(charge, fee(bytes.len(), 0)?)?;
    inline
        .get_mut(start..end)
        .ok_or(Error::Malformed)?
        .copy_from_slice(bytes);
    pay(charge, 128)?;
    *coordinates.get_mut(slot).ok_or(Error::Malformed)? = (start, bytes.len());
    Ok(())
}

fn indexed_error(error: Error) -> ReplyError {
    match error {
        Error::Schema(PolicySchemaError::InvalidAuthority) => ReplyError::FunctionClause,
        other => other.into(),
    }
}

fn fee(left: usize, right: usize) -> Result<u64, Error> {
    left.checked_add(right)
        .and_then(|n| u64::try_from(n).ok())
        .and_then(|n| n.checked_add(128))
        .ok_or_else(overflow)
}

fn authority() -> Error {
    Error::Schema(PolicySchemaError::InvalidAuthority)
}
