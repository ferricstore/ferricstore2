//! Mathematical cross-source equality; coarse fingerprints never prove identity.

use super::value::PolicyMergeCursor as Cursor;
use super::{Error, PolicyMergeArena, PolicyMergeRef as Ref, authority, overflow, pay};
use crate::policy::complete::PolicySchemaCharge;
use crate::policy::complete::flow_input::value::PolicyInputKind as Kind;

pub(super) fn hash(
    arena: &PolicyMergeArena<'_>,
    value: Ref<'_>,
    charge: &mut PolicySchemaCharge,
) -> Result<u64, Error> {
    let value = arena.view(value);
    if let Some(bytes) = value.atom_bytes().or_else(|| value.binary_bytes()) {
        pay(
            charge,
            128_usize.checked_add(bytes.len()).ok_or_else(overflow)?,
        )?;
        return Ok(fingerprint(bytes, 0xcbf2_9ce4_8422_2325));
    }
    if value.kind() == Kind::Integer {
        let mut buffer = [0; 16];
        let (negative, bytes) = magnitude(value, &mut buffer).ok_or_else(authority)?;
        pay(
            charge,
            128_usize.checked_add(bytes.len()).ok_or_else(overflow)?,
        )?;
        return Ok(fingerprint(
            bytes,
            fingerprint(&[u8::from(negative)], 0xcbf2_9ce4_8422_2325),
        ));
    }
    pay(charge, 128)?;
    Ok(match value.kind() {
        Kind::List | Kind::ImproperList => 1,
        Kind::Tuple => 2,
        Kind::Map => 3,
        _ => 4,
    })
}

fn fingerprint(bytes: &[u8], mut hash: u64) -> u64 {
    for &byte in bytes {
        hash = (hash ^ u64::from(byte)).wrapping_mul(0x100_0000_01b3);
    }
    hash
}

pub(super) fn equal(
    arena: &PolicyMergeArena<'_>,
    left: Ref<'_>,
    right: Ref<'_>,
    charge: &mut PolicySchemaCharge,
) -> Result<bool, Error> {
    pair(arena.view(left), arena.view(right), 0, charge)
}

fn pair(
    left: Cursor<'_>,
    right: Cursor<'_>,
    depth: usize,
    charge: &mut PolicySchemaCharge,
) -> Result<bool, Error> {
    if let (Ref::Current(a), Ref::Current(b)) = (left.value, right.value)
        && let Some(equal) = a.equal(b, charge)?
    {
        return Ok(equal);
    }
    pay(charge, 128)?;
    if depth > 64 {
        return Err(Error::ItemLimit);
    }
    if let (Some(a), Some(b)) = (left.atom_bytes(), right.atom_bytes()) {
        return bytes(a, b, charge);
    }
    if left.atom_bytes().is_some() || right.atom_bytes().is_some() {
        return Ok(false);
    }
    let (a, b) = (left.kind(), right.kind());
    if matches!(a, Kind::List | Kind::ImproperList) && matches!(b, Kind::List | Kind::ImproperList)
    {
        return lists(left, right, depth, charge);
    }
    if a != b {
        return Ok(false);
    }
    match a {
        Kind::Binary => bytes(
            left.binary_bytes().ok_or_else(authority)?,
            right.binary_bytes().ok_or_else(authority)?,
            charge,
        ),
        Kind::Integer => {
            let (mut a, mut b) = ([0; 16], [0; 16]);
            let (na, a) = magnitude(left, &mut a).ok_or_else(authority)?;
            let (nb, b) = magnitude(right, &mut b).ok_or_else(authority)?;
            let equal = bytes(a, b, charge)?;
            Ok(na == nb && equal)
        }
        Kind::Tuple => {
            if left.len() != right.len() {
                return Ok(false);
            }
            for index in 0..left.len() {
                if !pair(
                    left.child(index).ok_or_else(authority)?,
                    right.child(index).ok_or_else(authority)?,
                    depth + 1,
                    charge,
                )? {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        Kind::Map => maps(left, right, depth, charge),
        _ => Ok(false),
    }
}

fn maps(
    left: Cursor<'_>,
    right: Cursor<'_>,
    depth: usize,
    charge: &mut PolicySchemaCharge,
) -> Result<bool, Error> {
    if left.len() != right.len() {
        return Ok(false);
    }
    for index in 0..left.len() {
        pay(charge, 128)?;
        let (key, value) = left.row(index).ok_or_else(authority)?;
        let mut found = false;
        for candidate in 0..right.len() {
            pay(charge, 128)?;
            let (other, nested) = right.row(candidate).ok_or_else(authority)?;
            if !pair(key, other, depth + 1, charge)? {
                continue;
            }
            if !pair(value, nested, depth + 1, charge)? {
                return Ok(false);
            }
            found = true;
            break;
        }
        if !found {
            return Ok(false);
        }
    }
    Ok(true)
}

fn lists(
    mut left: Cursor<'_>,
    mut right: Cursor<'_>,
    depth: usize,
    charge: &mut PolicySchemaCharge,
) -> Result<bool, Error> {
    let (mut a, mut b) = (0, 0);
    loop {
        while a == left.len() && left.kind() == Kind::ImproperList {
            pay(charge, 128)?;
            left = left.tail().ok_or_else(authority)?;
            a = 0;
        }
        while b == right.len() && right.kind() == Kind::ImproperList {
            pay(charge, 128)?;
            right = right.tail().ok_or_else(authority)?;
            b = 0;
        }
        let alist = matches!(left.kind(), Kind::List | Kind::ImproperList);
        let blist = matches!(right.kind(), Kind::List | Kind::ImproperList);
        if !alist || !blist {
            return if !alist && !blist {
                pair(left, right, depth + 1, charge)
            } else {
                Ok(false)
            };
        }
        if a == left.len() || b == right.len() {
            return Ok(a == left.len() && b == right.len());
        }
        if !pair(
            left.child(a).ok_or_else(authority)?,
            right.child(b).ok_or_else(authority)?,
            depth + 1,
            charge,
        )? {
            return Ok(false);
        }
        a += 1;
        b += 1;
    }
}

fn magnitude<'a>(value: Cursor<'a>, buffer: &'a mut [u8; 16]) -> Option<(bool, &'a [u8])> {
    if let Some(value) = value.integer() {
        *buffer = value.unsigned_abs().to_le_bytes();
        let len = buffer.iter().rposition(|&n| n != 0).map_or(0, |n| n + 1);
        return Some((value < 0, &buffer[..len]));
    }
    value.big_integer()
}

fn bytes(a: &[u8], b: &[u8], charge: &mut PolicySchemaCharge) -> Result<bool, Error> {
    pay(
        charge,
        128_usize
            .checked_add(a.len())
            .and_then(|n| n.checked_add(b.len()))
            .ok_or_else(overflow)?,
    )?;
    Ok(a == b)
}
