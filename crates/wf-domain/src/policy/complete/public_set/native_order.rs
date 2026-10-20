//! Reached pinned OTP28 binary-map order, independent of host hash tables.

use super::{Error, PolicySchemaCharge, PolicySchemaError, charge_schema};
use crate::policy::complete::flow_input::{order::hash, value::PolicyInputMap};
use std::cmp::Ordering;

pub(in crate::policy::complete) fn ranks(
    map: PolicyInputMap<'_>,
    live: usize,
    charge: &mut PolicySchemaCharge,
) -> Result<Vec<u32>, Error> {
    let count = map.len(charge)?;
    let scratch = count
        .checked_mul(72)
        .and_then(|n| n.checked_add(64))
        .ok_or_else(overflow)?;
    charge_schema(
        charge,
        0,
        u64::try_from(live.checked_add(scratch).ok_or_else(overflow)?).map_err(|_| overflow())?,
    )
    .map_err(Error::Schema)?;
    pay(
        charge,
        count
            .checked_mul(64)
            .and_then(|n| n.checked_add(128))
            .ok_or_else(overflow)?,
    )?;
    let mut slots = Vec::new();
    slots
        .try_reserve_exact(count)
        .map_err(|_| Error::AllocationFailure)?;
    for index in 0..count {
        pay(charge, 128)?;
        let (key, _) = map.row_at(index, charge)?;
        let key = key.binary_bytes().ok_or(Error::UnsupportedSourceOrder)?;
        slots.push((
            swizzle(hash::binary_hash(key, charge)?),
            u32::try_from(index).map_err(|_| overflow())?,
        ));
    }
    for root in (0..count / 2).rev() {
        sift(&mut slots, map, root, count, count > 32, charge)?;
    }
    for end in (1..count).rev() {
        pay(charge, 256)?;
        slots.swap(0, end);
        sift(&mut slots, map, 0, end, count > 32, charge)?;
    }
    pay(
        charge,
        count
            .checked_mul(8)
            .and_then(|n| n.checked_add(128))
            .ok_or_else(overflow)?,
    )?;
    let mut result = Vec::new();
    result
        .try_reserve_exact(count)
        .map_err(|_| Error::AllocationFailure)?;
    for (_, index) in slots {
        result.push(index);
    }
    Ok(result)
}

fn sift(
    slots: &mut [(u64, u32)],
    map: PolicyInputMap<'_>,
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
            && compare(slots[child], slots[child + 1], map, large, charge)? == Ordering::Less
        {
            child += 1;
        }
        if compare(slots[root], slots[child], map, large, charge)? != Ordering::Less {
            return Ok(());
        }
        pay(charge, 256)?;
        slots.swap(root, child);
        root = child;
    }
}

fn compare(
    left: (u64, u32),
    right: (u64, u32),
    map: PolicyInputMap<'_>,
    large: bool,
    charge: &mut PolicySchemaCharge,
) -> Result<Ordering, Error> {
    if large {
        pay(charge, 16)?;
        let order = right.0.cmp(&left.0);
        if order != Ordering::Equal {
            return Ok(order);
        }
    }
    let (left, _) = map.row_at(usize::try_from(left.1).map_err(|_| overflow())?, charge)?;
    let (right, _) = map.row_at(usize::try_from(right.1).map_err(|_| overflow())?, charge)?;
    let left = left.binary_bytes().ok_or(Error::UnsupportedSourceOrder)?;
    let right = right.binary_bytes().ok_or(Error::UnsupportedSourceOrder)?;
    pay(
        charge,
        left.len()
            .checked_add(right.len())
            .and_then(|n| n.checked_add(128))
            .ok_or_else(overflow)?,
    )?;
    Ok(if large {
        right.cmp(left)
    } else {
        left.cmp(right)
    })
}
fn swizzle(hash: u64) -> u64 {
    let bytes = hash.swap_bytes();
    ((bytes & 0xf0f0_f0f0_f0f0_f0f0) >> 4) | ((bytes & 0x0f0f_0f0f_0f0f_0f0f) << 4)
}
fn pay(charge: &mut PolicySchemaCharge, bytes: usize) -> Result<(), Error> {
    charge_schema(charge, u64::try_from(bytes).map_err(|_| overflow())?, 0).map_err(Error::Schema)
}
fn overflow() -> Error {
    Error::Schema(PolicySchemaError::LengthOverflow)
}
