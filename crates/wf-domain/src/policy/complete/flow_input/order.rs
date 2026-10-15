//! Paid source-map traversal and deterministic output-key ordering.

use std::cmp::Ordering;

use crate::policy::complete::{
    CompletePolicyEncodingError as Error, PolicySchemaCharge, PolicySchemaError, charge_schema,
};

use super::layout::{FlowPolicyCell, FlowPolicyLayout, FlowPolicyWorkSlot};
use super::value::{PolicyInputCursor, PolicyInputMap};

pub(super) mod hash;

pub(super) fn prepare_map_order<'input>(
    map: PolicyInputMap<'input>,
    layout: &mut FlowPolicyLayout<'input>,
    charge: &mut PolicySchemaCharge,
) -> Result<(usize, usize), Error> {
    charge_schema(charge, 128, 0).map_err(Error::Schema)?;
    prepare_map_order_rows(map, layout, charge)
}

pub(super) fn prepare_map_order_rows<'input>(
    map: PolicyInputMap<'input>,
    layout: &mut FlowPolicyLayout<'input>,
    charge: &mut PolicySchemaCharge,
) -> Result<(usize, usize), Error> {
    let source = map.bind_source_order(charge)?;
    let start = layout.work.len();
    for source_index in 0..source.len(charge)? {
        charge_schema(charge, 128, 0).map_err(Error::Schema)?;
        let (index, key, _) = source.source_row_at(source_index, charge)?;
        let bytes =
            key_bytes(key).filter(|(atom, _)| !*atom || matches!(&map, PolicyInputMap::Keyword(_)));
        let fingerprint = bytes.map_or(Ok(0), |(_, bytes)| hash::binary_hash(bytes, charge))?;
        let duplicate = if bytes.is_some() {
            duplicate_slot(&map, layout, start, key, fingerprint, charge)?
        } else {
            None
        };
        if let Some(slot) = duplicate {
            if matches!(&map, PolicyInputMap::Direct(_)) {
                return Err(Error::Malformed);
            }
            charge_schema(charge, 128, 0).map_err(Error::Schema)?;
            layout.work.get_mut(slot).ok_or(Error::Malformed)?.index = index;
        } else {
            layout.push_work(
                FlowPolicyWorkSlot {
                    hash: fingerprint,
                    index,
                    cell: 0,
                    order: swizzle(fingerprint),
                },
                charge,
            )?;
        }
    }
    let len = layout.work.len() - start;
    if matches!(&map, PolicyInputMap::StatePairs(_)) {
        sort_slots(layout, &map, start, len, charge)?;
    }
    Ok((start, len))
}

fn duplicate_slot<'input>(
    map: &PolicyInputMap<'input>,
    layout: &FlowPolicyLayout<'input>,
    start: usize,
    key: PolicyInputCursor<'input>,
    fingerprint: u64,
    charge: &mut PolicySchemaCharge,
) -> Result<Option<usize>, Error> {
    let (kind, bytes) = key_bytes(key).ok_or(Error::Malformed)?;
    for index in start..layout.work.len() {
        let slot = layout.work.get(index).ok_or(Error::Malformed)?;
        charge_schema(charge, 16, 0).map_err(Error::Schema)?;
        if slot.hash != fingerprint {
            continue;
        }
        let (prior, _) = map.row_at(slot.index, charge)?;
        if let Some((prior_kind, prior_bytes)) = key_bytes(prior)
            && kind == prior_kind
        {
            pay_compare(bytes, prior_bytes, charge)?;
            if bytes == prior_bytes {
                return Ok(Some(index));
            }
        }
    }
    Ok(None)
}

fn key_bytes(key: PolicyInputCursor<'_>) -> Option<(bool, &[u8])> {
    key.binary_bytes()
        .map(|bytes| (false, bytes))
        .or_else(|| key.atom_bytes().map(|bytes| (true, bytes)))
}

fn swizzle(hash: u64) -> u64 {
    let bytes = hash.swap_bytes();
    ((bytes & 0xf0f0_f0f0_f0f0_f0f0) >> 4) | ((bytes & 0x0f0f_0f0f_0f0f_0f0f) << 4)
}

fn sort_slots<'input>(
    layout: &mut FlowPolicyLayout<'input>,
    map: &PolicyInputMap<'input>,
    start: usize,
    len: usize,
    charge: &mut PolicySchemaCharge,
) -> Result<(), Error> {
    let large = len > 32;
    for root in (0..len / 2).rev() {
        sift_slots(layout, map, start, root, len, large, charge)?;
    }
    for end in (1..len).rev() {
        charge_schema(charge, 256, 0).map_err(Error::Schema)?;
        layout.work.swap(start, start + end);
        sift_slots(layout, map, start, 0, end, large, charge)?;
    }
    Ok(())
}

fn sift_slots<'input>(
    layout: &mut FlowPolicyLayout<'input>,
    map: &PolicyInputMap<'input>,
    start: usize,
    mut root: usize,
    end: usize,
    large: bool,
    charge: &mut PolicySchemaCharge,
) -> Result<(), Error> {
    loop {
        let mut child = root
            .checked_mul(2)
            .and_then(|n| n.checked_add(1))
            .ok_or_else(overflow)?;
        if child >= end {
            return Ok(());
        }
        if child + 1 < end
            && compare_slots(layout, map, start + child, start + child + 1, large, charge)?
                == Ordering::Less
        {
            child += 1;
        }
        if compare_slots(layout, map, start + root, start + child, large, charge)? != Ordering::Less
        {
            return Ok(());
        }
        charge_schema(charge, 256, 0).map_err(Error::Schema)?;
        layout.work.swap(start + root, start + child);
        root = child;
    }
}

fn compare_slots<'input>(
    layout: &FlowPolicyLayout<'input>,
    map: &PolicyInputMap<'input>,
    left: usize,
    right: usize,
    large: bool,
    charge: &mut PolicySchemaCharge,
) -> Result<Ordering, Error> {
    let left = layout.work.get(left).ok_or(Error::Malformed)?;
    let right = layout.work.get(right).ok_or(Error::Malformed)?;
    if large {
        charge_schema(charge, 16, 0).map_err(Error::Schema)?;
        let order = right.order.cmp(&left.order);
        if order != Ordering::Equal {
            return Ok(order);
        }
    }
    let (left, _) = map.row_at(left.index, charge)?;
    let (right, _) = map.row_at(right.index, charge)?;
    let left = left.binary_bytes().ok_or(Error::Malformed)?;
    let right = right.binary_bytes().ok_or(Error::Malformed)?;
    pay_compare(left, right, charge)?;
    Ok(if large {
        right.cmp(left)
    } else {
        left.cmp(right)
    })
}

pub(super) fn sort_output_map(
    layout: &mut FlowPolicyLayout<'_>,
    start: usize,
    len: usize,
    charge: &mut PolicySchemaCharge,
) -> Result<(), Error> {
    let end = len
        .checked_mul(2)
        .and_then(|n| start.checked_add(n))
        .ok_or_else(overflow)?;
    if end > layout.edges.len() {
        return Err(Error::Malformed);
    }
    for root in (0..len / 2).rev() {
        sift_output(layout, start, root, len, charge)?;
    }
    for end in (1..len).rev() {
        swap_pairs(layout, start, 0, end, charge)?;
        sift_output(layout, start, 0, end, charge)?;
    }
    Ok(())
}

fn sift_output(
    layout: &mut FlowPolicyLayout<'_>,
    start: usize,
    mut root: usize,
    end: usize,
    charge: &mut PolicySchemaCharge,
) -> Result<(), Error> {
    loop {
        let mut child = root
            .checked_mul(2)
            .and_then(|n| n.checked_add(1))
            .ok_or_else(overflow)?;
        if child >= end {
            return Ok(());
        }
        if child + 1 < end
            && compare_output(layout, start, child, child + 1, charge)? == Ordering::Less
        {
            child += 1;
        }
        if compare_output(layout, start, root, child, charge)? != Ordering::Less {
            return Ok(());
        }
        swap_pairs(layout, start, root, child, charge)?;
        root = child;
    }
}

fn compare_output(
    layout: &FlowPolicyLayout<'_>,
    start: usize,
    left: usize,
    right: usize,
    charge: &mut PolicySchemaCharge,
) -> Result<Ordering, Error> {
    let (left_kind, left_bytes) = output_key(layout, start + left * 2)?;
    let (right_kind, right_bytes) = output_key(layout, start + right * 2)?;
    pay_compare(left_bytes, right_bytes, charge)?;
    Ok(left_kind
        .cmp(&right_kind)
        .then_with(|| left_bytes.cmp(right_bytes)))
}

fn output_key<'input>(
    layout: &FlowPolicyLayout<'input>,
    edge: usize,
) -> Result<(u8, &'input [u8]), Error> {
    let index = *layout.edges.get(edge).ok_or(Error::Malformed)?;
    match *layout.cells.get(index).ok_or(Error::Malformed)? {
        FlowPolicyCell::Atom(name) => Ok((0, name.as_bytes())),
        FlowPolicyCell::Binary(name) => Ok((1, name)),
        _ => Err(Error::Malformed),
    }
}

fn swap_pairs(
    layout: &mut FlowPolicyLayout<'_>,
    start: usize,
    left: usize,
    right: usize,
    charge: &mut PolicySchemaCharge,
) -> Result<(), Error> {
    charge_schema(charge, 512, 0).map_err(Error::Schema)?;
    let left = start + left * 2;
    let right = start + right * 2;
    layout.edges.swap(left, right);
    layout.edges.swap(left + 1, right + 1);
    Ok(())
}

fn pay_compare(left: &[u8], right: &[u8], charge: &mut PolicySchemaCharge) -> Result<(), Error> {
    let length = left.len().checked_add(right.len()).ok_or_else(overflow)?;
    let fee = u64::try_from(length)
        .ok()
        .and_then(|n| n.checked_add(128))
        .ok_or_else(overflow)?;
    charge_schema(charge, fee, 0).map_err(Error::Schema)
}

fn overflow() -> Error {
    Error::Schema(PolicySchemaError::LengthOverflow)
}
