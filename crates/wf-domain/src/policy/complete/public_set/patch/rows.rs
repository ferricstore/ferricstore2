//! One active ordinal region for paid synthesized-map sorting and LAST groups.

use super::super::{
    Error, PolicySchemaCharge, PolicySetPatchSlot, Raw, atom, control_name, debit, item, row,
};
use std::cmp::Ordering;

pub(super) fn keyword(
    value: Raw<'_>,
    top: bool,
    work: &mut Vec<PolicySetPatchSlot>,
    charge: &mut PolicySchemaCharge,
) -> Result<(usize, usize), Error> {
    debit(charge, 128)?;
    let start = work.len();
    let mut index = 0;
    loop {
        let (entry, proper) = item(value, index, charge)?;
        let Some(Raw::Tuple(pair)) = entry else {
            if entry.is_some() || !proper {
                return Err(Error::Malformed);
            }
            break;
        };
        if pair.len() != 2 {
            return Err(Error::Malformed);
        }
        let key = atom(pair[0]).ok_or(Error::Malformed)?;
        if !top || control_name(key, charge)?.is_none() {
            let hash = fingerprint(key, charge)?;
            push(
                work,
                PolicySetPatchSlot {
                    source: index,
                    cell: usize::MAX,
                    hash,
                },
                charge,
            )?;
        }
        index += 1;
    }
    let len = work.len() - start;
    compact(value, work, start, len, charge).map(|len| (start, len))
}

pub(super) fn fingerprint(bytes: &[u8], charge: &mut PolicySchemaCharge) -> Result<u64, Error> {
    debit(charge, 128 + bytes.len())?;
    let mut hash = 14_695_981_039_346_656_037_u64;
    for &byte in bytes {
        hash = (hash ^ u64::from(byte)).wrapping_mul(1_099_511_628_211);
    }
    Ok(hash)
}

pub(super) fn push(
    work: &mut Vec<PolicySetPatchSlot>,
    slot: PolicySetPatchSlot,
    charge: &mut PolicySchemaCharge,
) -> Result<(), Error> {
    debit(charge, 128)?;
    if work.len() == work.capacity() {
        return Err(Error::ItemLimit);
    }
    work.push(slot);
    Ok(())
}

pub(super) fn compact(
    value: Raw<'_>,
    work: &mut Vec<PolicySetPatchSlot>,
    start: usize,
    len: usize,
    charge: &mut PolicySchemaCharge,
) -> Result<usize, Error> {
    for root in (0..len / 2).rev() {
        sift(value, work, start, len, root, charge)?;
    }
    for end in (1..len).rev() {
        swap(work, start, start + end, charge)?;
        sift(value, work, start, end, 0, charge)?;
    }
    let mut read = 0;
    let mut write = 0;
    while read < len {
        let mut last = read;
        while last + 1 < len {
            debit(charge, 16)?;
            let a = work[start + last];
            let b = work[start + last + 1];
            if a.hash != b.hash {
                break;
            }
            let (a, _) = row(value, a.source, charge)?;
            let (b, _) = row(value, b.source, charge)?;
            let a = key(a)?;
            let b = key(b)?;
            debit(charge, 128 + a.len() + b.len())?;
            if a != b {
                break;
            }
            last += 1;
        }
        if write != last {
            debit(charge, 128)?;
            work[start + write] = work[start + last];
        }
        write += 1;
        read = last + 1;
    }
    work.truncate(start + write);
    Ok(write)
}

fn sift(
    value: Raw<'_>,
    work: &mut [PolicySetPatchSlot],
    start: usize,
    len: usize,
    mut parent: usize,
    charge: &mut PolicySchemaCharge,
) -> Result<(), Error> {
    loop {
        let mut child = parent * 2 + 1;
        if child >= len {
            return Ok(());
        }
        if child + 1 < len
            && compare(value, work[start + child], work[start + child + 1], charge)?
                == Ordering::Less
        {
            child += 1;
        }
        if compare(value, work[start + parent], work[start + child], charge)? != Ordering::Less {
            return Ok(());
        }
        swap(work, start + parent, start + child, charge)?;
        parent = child;
    }
}

fn compare(
    value: Raw<'_>,
    a: PolicySetPatchSlot,
    b: PolicySetPatchSlot,
    charge: &mut PolicySchemaCharge,
) -> Result<Ordering, Error> {
    let (a_key, _) = row(value, a.source, charge)?;
    let (b_key, _) = row(value, b.source, charge)?;
    let a_key = key(a_key)?;
    let b_key = key(b_key)?;
    debit(charge, 128 + a_key.len() + b_key.len())?;
    let compared = a_key.cmp(b_key);
    if compared != Ordering::Equal {
        return Ok(compared);
    }
    debit(charge, 16)?;
    Ok(a.source.cmp(&b.source))
}

fn key(value: Raw<'_>) -> Result<&[u8], Error> {
    match value {
        Raw::Binary(bytes) => Ok(bytes),
        _ => atom(value).ok_or(Error::Malformed),
    }
}

fn swap(
    work: &mut [PolicySetPatchSlot],
    a: usize,
    b: usize,
    charge: &mut PolicySchemaCharge,
) -> Result<(), Error> {
    debit(charge, 256)?;
    work.swap(a, b);
    Ok(())
}
