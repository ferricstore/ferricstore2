//! Eager Map.new callbacks, followed by exact list-LAST index compaction.

use std::cmp::Ordering;

use super::super::flow_input::value::PolicyInputKind as Kind;
use super::super::original_entry::{TermPosition, indexed::OriginalPolicyCurrentCursor};
use super::model::{
    OriginalPolicyReply as Reply, OriginalPolicyReplyContext as Context, PolicyReplyRef as Ref,
};
use super::{
    EncodingError as Error, OriginalPolicyReplyError as ReplyError, PolicySchemaCharge, overflow,
    pay, resolve, source,
};

pub(super) fn all<'input>(
    owner: &mut Reply<'input>,
    input: Option<TermPosition>,
    context: &Context<'input>,
    charge: &mut PolicySchemaCharge,
) -> Result<(), ReplyError> {
    pay(charge, 128)?;
    let mut unique = None;
    if let Some(input) = input {
        let index = owner.index.as_ref().ok_or(Error::Malformed)?;
        match index.at(input).kind() {
            Kind::Map => map(owner, input, context, charge)?,
            Kind::List | Kind::ImproperList => {
                list(owner, input, charge)?;
                sort(owner, charge)?;
                unique = Some(compact(owner, charge)?);
            }
            Kind::Other
                if index.at(input).float().is_none() && index.at(input).bitstring().is_none() =>
            {
                return Err(Error::UnsupportedTag.into());
            }
            _ => return Err(ReplyError::Enumerable),
        }
    }
    pay(charge, 128)?;
    if let Some(length) = unique {
        owner.order.truncate(length);
    }
    Ok(())
}

fn map<'input>(
    owner: &mut Reply<'input>,
    input: TermPosition,
    context: &Context<'input>,
    charge: &mut PolicySchemaCharge,
) -> Result<(), ReplyError> {
    let index = owner.index.as_ref().ok_or(Error::Malformed)?;
    source::guard_map(index, input, charge)?;
    let count = index.at(input).len();
    let (forward, extra) =
        source::native_order(index, input, context, owner.resident_bound, charge)?;
    owner.resident_bound = owner
        .resident_bound
        .checked_add(extra)
        .ok_or_else(overflow)?;
    for ordinal in 0..count {
        pay(charge, 128)?;
        let rank = forward.map_or(Ok(ordinal), |rows| {
            usize::try_from(rows[ordinal]).map_err(|_| overflow())
        })?;
        let index = owner.index.as_ref().ok_or(Error::Malformed)?;
        let (key, value) = index.at(input).row(rank).ok_or(Error::Malformed)?;
        let config = key.binary_bytes().is_some().then_some(value.position());
        let name = key.position();
        pay(charge, 128)?;
        let state = resolve::state(
            owner,
            config,
            Ref::Source(name),
            u32::try_from(ordinal).map_err(|_| overflow())?,
            charge,
        )?;
        append(owner, state, charge)?;
    }
    Ok(())
}

fn list(
    owner: &mut Reply<'_>,
    mut input: TermPosition,
    charge: &mut PolicySchemaCharge,
) -> Result<(), ReplyError> {
    let mut ordinal = 0_u32;
    loop {
        let index = owner.index.as_ref().ok_or(Error::Malformed)?;
        let cursor = index.at(input);
        if !matches!(cursor.kind(), Kind::List | Kind::ImproperList) {
            return Err(ReplyError::FunctionClause);
        }
        let length = cursor.len();
        for row in 0..length {
            pay(charge, 128)?;
            let index = owner.index.as_ref().ok_or(Error::Malformed)?;
            let item = index.at(input).child(row).ok_or(Error::Malformed)?;
            pay(charge, 128)?;
            if item.kind() != Kind::Tuple || item.len() != 2 {
                return Err(ReplyError::FunctionClause);
            }
            let name = item.child(0).ok_or(Error::Malformed)?.position();
            // Source state helpers consult MAP states only; list cfg is ignored.
            let state = resolve::state(owner, None, Ref::Source(name), ordinal, charge)?;
            append(owner, state, charge)?;
            ordinal = ordinal.checked_add(1).ok_or_else(overflow)?;
        }
        let tail = owner
            .index
            .as_ref()
            .ok_or(Error::Malformed)?
            .at(input)
            .tail()
            .map(OriginalPolicyCurrentCursor::position);
        let Some(tail) = tail else {
            return Ok(());
        };
        pay(charge, 128)?;
        input = tail;
    }
}

fn append<'input>(
    owner: &mut Reply<'input>,
    state: super::model::PolicyReplyState<'input>,
    charge: &mut PolicySchemaCharge,
) -> Result<(), Error> {
    pay(charge, 128)?;
    if owner.records.len() >= owner.capacity {
        return Err(Error::ItemLimit);
    }
    let slot = u32::try_from(owner.records.len()).map_err(|_| overflow())?;
    owner.records.push(state);
    pay(charge, 128)?;
    if owner.order.len() >= owner.capacity {
        return Err(Error::ItemLimit);
    }
    owner.order.push(slot);
    Ok(())
}

fn sort(owner: &mut Reply<'_>, charge: &mut PolicySchemaCharge) -> Result<(), Error> {
    let count = owner.order.len();
    for root in (0..count / 2).rev() {
        sift(owner, root, count, charge)?;
    }
    for end in (1..count).rev() {
        pay(charge, 256)?;
        owner.order.swap(0, end);
        sift(owner, 0, end, charge)?;
    }
    Ok(())
}

fn sift(
    owner: &mut Reply<'_>,
    mut root: usize,
    count: usize,
    charge: &mut PolicySchemaCharge,
) -> Result<(), Error> {
    while root * 2 + 1 < count {
        let mut child = root * 2 + 1;
        if child + 1 < count
            && compare(owner, owner.order[child], owner.order[child + 1], charge)? == Ordering::Less
        {
            child += 1;
        }
        if compare(owner, owner.order[root], owner.order[child], charge)? != Ordering::Less {
            break;
        }
        pay(charge, 256)?;
        owner.order.swap(root, child);
        root = child;
    }
    Ok(())
}

fn compare(
    owner: &Reply<'_>,
    left: u32,
    right: u32,
    charge: &mut PolicySchemaCharge,
) -> Result<Ordering, Error> {
    let order = keys(owner, left, right, charge)?;
    if order != Ordering::Equal {
        return Ok(order);
    }
    pay(charge, 16)?;
    Ok(owner.records[left as usize]
        .source_ordinal
        .cmp(&owner.records[right as usize].source_ordinal))
}

fn keys(
    owner: &Reply<'_>,
    left: u32,
    right: u32,
    charge: &mut PolicySchemaCharge,
) -> Result<Ordering, Error> {
    let Ref::Source(left) = owner
        .records
        .get(left as usize)
        .ok_or(Error::Malformed)?
        .name
    else {
        return Err(Error::Malformed);
    };
    let Ref::Source(right) = owner
        .records
        .get(right as usize)
        .ok_or(Error::Malformed)?
        .name
    else {
        return Err(Error::Malformed);
    };
    owner
        .index
        .as_ref()
        .ok_or(Error::Malformed)?
        .compare_positions(left, right, charge)
}

fn compact(owner: &mut Reply<'_>, charge: &mut PolicySchemaCharge) -> Result<usize, Error> {
    let (mut read, mut write) = (0, 0);
    while read < owner.order.len() {
        let mut last = owner.order[read];
        read += 1;
        while read < owner.order.len() {
            if keys(owner, last, owner.order[read], charge)? != Ordering::Equal {
                break;
            }
            last = owner.order[read];
            read += 1;
        }
        if owner.order[write] != last {
            pay(charge, 128)?;
            owner.order[write] = last;
        }
        write += 1;
    }
    Ok(write)
}
