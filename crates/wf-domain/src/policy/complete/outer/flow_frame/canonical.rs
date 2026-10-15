//! Paid mathematical comparison and one bounded stack of compact map regions.

use super::{Error, PolicySchemaCharge, add, overflow, visit};
use crate::policy::complete::flow_input::value::{
    PolicyInputCursor as Cursor, PolicyInputKind as Kind,
};
use std::cmp::Ordering;

#[derive(Clone, Copy)]
pub(super) struct FlowFrameCanonicalSlot {
    pub(super) source: usize,
}

pub(super) fn prepare(
    map: Cursor<'_>,
    slots: &mut Vec<FlowFrameCanonicalSlot>,
    charge: &mut PolicySchemaCharge,
) -> Result<usize, Error> {
    visit(charge, 128)?;
    let start = slots.len();
    for source in 0..map.len() {
        row(map, source, charge)?;
        visit(charge, 128)?;
        if slots.len() == slots.capacity() {
            return Err(overflow());
        }
        slots.push(FlowFrameCanonicalSlot { source });
    }
    let length = map.len();
    for root in (0..length / 2).rev() {
        sift(map, start, root, length, slots, charge)?;
    }
    for end in (1..length).rev() {
        swap(slots, start, start + end, charge)?;
        sift(map, start, 0, end, slots, charge)?;
    }
    for index in 1..length {
        if keys(
            map,
            slots[start + index - 1].source,
            slots[start + index].source,
            slots,
            charge,
        )? == Ordering::Equal
        {
            return Err(Error::Malformed);
        }
    }
    Ok(start)
}

fn sift(
    map: Cursor<'_>,
    start: usize,
    mut root: usize,
    length: usize,
    slots: &mut Vec<FlowFrameCanonicalSlot>,
    charge: &mut PolicySchemaCharge,
) -> Result<(), Error> {
    loop {
        let mut child = root * 2 + 1;
        if child >= length {
            return Ok(());
        }
        if child + 1 < length
            && keys(
                map,
                slots[start + child].source,
                slots[start + child + 1].source,
                slots,
                charge,
            )? == Ordering::Less
        {
            child += 1;
        }
        if keys(
            map,
            slots[start + root].source,
            slots[start + child].source,
            slots,
            charge,
        )? != Ordering::Less
        {
            return Ok(());
        }
        swap(slots, start + root, start + child, charge)?;
        root = child;
    }
}

fn swap(
    slots: &mut [FlowFrameCanonicalSlot],
    left: usize,
    right: usize,
    charge: &mut PolicySchemaCharge,
) -> Result<(), Error> {
    visit(charge, 256)?;
    slots.swap(left, right);
    Ok(())
}

fn keys(
    map: Cursor<'_>,
    left: usize,
    right: usize,
    slots: &mut Vec<FlowFrameCanonicalSlot>,
    charge: &mut PolicySchemaCharge,
) -> Result<Ordering, Error> {
    let a = row(map, left, charge)?.0;
    let b = row(map, right, charge)?.0;
    compare(a, b, slots, charge)
}

pub(super) fn row<'input>(
    map: Cursor<'input>,
    index: usize,
    charge: &mut PolicySchemaCharge,
) -> Result<(Cursor<'input>, Cursor<'input>), Error> {
    if let Cursor::Frame(frame) = map {
        return frame
            .canonical_row(index)
            .map(|(k, v)| (Cursor::Frame(k), Cursor::Frame(v)))
            .ok_or(Error::Malformed);
    }
    visit(charge, 128)?;
    map.row(index).ok_or(Error::Malformed)
}

fn rank(value: Cursor<'_>) -> u8 {
    if value.is_atom() {
        return 1;
    }
    match value.kind() {
        Kind::Integer => 0,
        Kind::Binary => 2,
        Kind::Tuple => 3,
        Kind::List | Kind::ImproperList => 4,
        Kind::Map => 5,
        _ => 6,
    }
}

pub(super) fn compare(
    left: Cursor<'_>,
    right: Cursor<'_>,
    slots: &mut Vec<FlowFrameCanonicalSlot>,
    charge: &mut PolicySchemaCharge,
) -> Result<Ordering, Error> {
    visit(charge, 128)?;
    let classes = rank(left).cmp(&rank(right));
    if classes != Ordering::Equal {
        return Ok(classes);
    }
    match rank(left) {
        0 => integers(left, right, charge),
        1 => bytes(
            left.atom_bytes().ok_or(Error::Malformed)?,
            right.atom_bytes().ok_or(Error::Malformed)?,
            charge,
        ),
        2 => bytes(
            left.binary_bytes().ok_or(Error::Malformed)?,
            right.binary_bytes().ok_or(Error::Malformed)?,
            charge,
        ),
        3 => {
            let arity = left.len().cmp(&right.len());
            if arity != Ordering::Equal {
                return Ok(arity);
            }
            for index in 0..left.len() {
                let result = compare(
                    left.child(index).ok_or(Error::Malformed)?,
                    right.child(index).ok_or(Error::Malformed)?,
                    slots,
                    charge,
                )?;
                if result != Ordering::Equal {
                    return Ok(result);
                }
            }
            Ok(Ordering::Equal)
        }
        4 => lists(left, right, slots, charge),
        5 => maps(left, right, slots, charge),
        _ => Err(Error::UnsupportedTag),
    }
}

fn bytes(left: &[u8], right: &[u8], charge: &mut PolicySchemaCharge) -> Result<Ordering, Error> {
    visit(charge, add(128, add(left.len(), right.len())?)?)?;
    Ok(left.cmp(right))
}

fn integers(
    left: Cursor<'_>,
    right: Cursor<'_>,
    charge: &mut PolicySchemaCharge,
) -> Result<Ordering, Error> {
    let (mut a, mut b) = ([0; 16], [0; 16]);
    let (an, ab) = magnitude(left, &mut a, charge)?;
    let (bn, bb) = magnitude(right, &mut b, charge)?;
    visit(charge, add(128, add(ab.len(), bb.len())?)?)?;
    if an != bn {
        return Ok(bn.cmp(&an));
    }
    let result = ab
        .len()
        .cmp(&bb.len())
        .then_with(|| ab.iter().rev().cmp(bb.iter().rev()));
    Ok(if an { result.reverse() } else { result })
}

pub(super) fn magnitude<'input>(
    input: Cursor<'input>,
    small: &'input mut [u8; 16],
    charge: &mut PolicySchemaCharge,
) -> Result<(bool, &'input [u8]), Error> {
    if let Some((negative, bytes)) = input.big_integer() {
        let length = if matches!(input, Cursor::Frame(_)) {
            bytes.len()
        } else {
            visit(charge, add(128, bytes.len())?)?;
            bytes.iter().rposition(|&n| n != 0).map_or(0, |n| n + 1)
        };
        return Ok((negative && length != 0, &bytes[..length]));
    }
    let value = input.integer().ok_or(Error::Malformed)?;
    *small = value.unsigned_abs().to_le_bytes();
    let length = small.iter().rposition(|&n| n != 0).map_or(0, |n| n + 1);
    Ok((value < 0, &small[..length]))
}

fn maps(
    left: Cursor<'_>,
    right: Cursor<'_>,
    slots: &mut Vec<FlowFrameCanonicalSlot>,
    charge: &mut PolicySchemaCharge,
) -> Result<Ordering, Error> {
    if left.len() != right.len() {
        return Ok(left.len().cmp(&right.len()));
    }
    let base = slots.len();
    let a = if matches!(left, Cursor::Frame(_)) {
        None
    } else {
        Some(prepare(left, slots, charge)?)
    };
    let b = if matches!(right, Cursor::Frame(_)) {
        None
    } else {
        Some(prepare(right, slots, charge)?)
    };
    let mut result = Ordering::Equal;
    for index in 0..left.len() {
        let ai = a.map_or(index, |start| slots[start + index].source);
        let bi = b.map_or(index, |start| slots[start + index].source);
        let (ak, av) = row(left, ai, charge)?;
        let (bk, bv) = row(right, bi, charge)?;
        result = compare(ak, bk, slots, charge)?;
        if result == Ordering::Equal {
            result = compare(av, bv, slots, charge)?;
        }
        if result != Ordering::Equal {
            break;
        }
    }
    slots.truncate(base);
    Ok(result)
}

fn lists(
    mut left: Cursor<'_>,
    mut right: Cursor<'_>,
    slots: &mut Vec<FlowFrameCanonicalSlot>,
    charge: &mut PolicySchemaCharge,
) -> Result<Ordering, Error> {
    let (mut a, mut b) = (0, 0);
    loop {
        let ac = next(&mut left, &mut a, charge)?;
        let bc = next(&mut right, &mut b, charge)?;
        match (ac, bc) {
            (Some(ac), Some(bc)) => {
                let result = compare(ac, bc, slots, charge)?;
                if result != Ordering::Equal {
                    return Ok(result);
                }
            }
            (None, None) => {
                if left.kind() == Kind::List && right.kind() == Kind::List {
                    return Ok(Ordering::Equal);
                }
                return compare(left, right, slots, charge);
            }
            (None, Some(_)) => return terminal_order(left, false, charge),
            (Some(_), None) => return terminal_order(right, true, charge),
        }
    }
}

fn terminal_order(
    terminal: Cursor<'_>,
    reverse: bool,
    charge: &mut PolicySchemaCharge,
) -> Result<Ordering, Error> {
    visit(charge, 128)?;
    let result = if terminal.kind() == Kind::List {
        Ordering::Less
    } else {
        rank(terminal).cmp(&4)
    };
    Ok(if reverse { result.reverse() } else { result })
}

pub(super) fn next<'input>(
    input: &mut Cursor<'input>,
    index: &mut usize,
    charge: &mut PolicySchemaCharge,
) -> Result<Option<Cursor<'input>>, Error> {
    loop {
        if !matches!(input.kind(), Kind::List | Kind::ImproperList) {
            return Ok(None);
        }
        if *index < input.len() {
            let child = input.child(*index).ok_or(Error::Malformed)?;
            *index += 1;
            return Ok(Some(child));
        }
        let Some(tail) = input.tail() else {
            return Ok(None);
        };
        visit(charge, 128)?;
        *input = tail;
        *index = 0;
    }
}

pub(super) fn list_shape(
    mut input: Cursor<'_>,
    charge: &mut PolicySchemaCharge,
) -> Result<(usize, bool), Error> {
    let mut length = 0;
    loop {
        if !matches!(input.kind(), Kind::List | Kind::ImproperList) {
            return Ok((length, false));
        }
        length = add(length, input.len())?;
        let Some(tail) = input.tail() else {
            return Ok((length, true));
        };
        visit(charge, 128)?;
        input = tail;
    }
}
