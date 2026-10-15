//! Pinned raw external-size metric, independent of business normalization.

use super::{
    Error, PolicySchemaCharge, PolicySetPatchArena, PolicySetPatchCell, Raw, debit, item, overflow,
    significant,
};

pub(super) fn measure(
    arena: &PolicySetPatchArena<'_>,
    root: usize,
    charge: &mut PolicySchemaCharge,
) -> Result<usize, Error> {
    debit(charge, 1)?;
    add(1, cell(arena, root, charge)?)
}

fn cell(
    arena: &PolicySetPatchArena<'_>,
    index: usize,
    charge: &mut PolicySchemaCharge,
) -> Result<usize, Error> {
    debit(charge, 128)?;
    match *arena.cells.get(index).ok_or(Error::Malformed)? {
        PolicySetPatchCell::Raw(value) => payload(value, charge),
        PolicySetPatchCell::Map { start, pairs } => {
            debit(charge, 5)?;
            let mut size = 5;
            for offset in 0..pairs.checked_mul(2).ok_or_else(overflow)? {
                let child = *arena
                    .edges
                    .get(start.checked_add(offset).ok_or_else(overflow)?)
                    .ok_or(Error::Malformed)?;
                size = add(size, cell(arena, child, charge)?)?;
            }
            Ok(size)
        }
        PolicySetPatchCell::List { start, len } => list_cells(arena, start, len, charge),
    }
}

fn raw(value: Raw<'_>, charge: &mut PolicySchemaCharge) -> Result<usize, Error> {
    debit(charge, 128)?;
    payload(value, charge)
}

fn payload(value: Raw<'_>, charge: &mut PolicySchemaCharge) -> Result<usize, Error> {
    match value {
        Raw::Nil => width(5, charge),
        Raw::Atom(text) => width(
            add(if text.len() <= 255 { 2 } else { 3 }, text.len())?,
            charge,
        ),
        Raw::Binary(bytes) => width(add(5, bytes.len())?, charge),
        Raw::Integer(number) => width(integer_width(number), charge),
        Raw::BigInteger {
            negative,
            little_endian,
        } => {
            let bytes = significant(little_endian, charge)?;
            width(big_width(negative, bytes)?, charge)
        }
        Raw::Tuple(items) => {
            let mut size = width(if items.len() <= 255 { 2 } else { 5 }, charge)?;
            for &item in items {
                size = add(size, raw(item, charge)?)?;
            }
            Ok(size)
        }
        Raw::Map(rows) => {
            let mut size = width(5, charge)?;
            for pair in rows {
                // This key's term visit also precedes the actual row access.
                debit(charge, 128)?;
                let (key, value) = *pair;
                size = add(size, payload(key, charge)?)?;
                size = add(size, raw(value, charge)?)?;
            }
            Ok(size)
        }
        Raw::List(_) | Raw::ImproperList { .. } => list_raw(value, charge),
        Raw::Other => Err(Error::UnsupportedTag),
    }
}

fn integer_width(number: i128) -> usize {
    if u8::try_from(number).is_ok() {
        2
    } else if i32::try_from(number).is_ok() {
        5
    } else {
        3 + usize::try_from(number.unsigned_abs().bit_width().div_ceil(8)).unwrap_or(16)
    }
}

fn big_width(negative: bool, bytes: &[u8]) -> Result<usize, Error> {
    if bytes.is_empty() || (!negative && bytes.len() == 1) {
        return Ok(2);
    }
    if bytes.len() <= 4 {
        let mut number = 0_u32;
        for (index, &byte) in bytes.iter().enumerate() {
            number |= u32::from(byte) << (index * 8);
        }
        if number
            <= if negative {
                2_147_483_648
            } else {
                2_147_483_647
            }
        {
            return Ok(5);
        }
    }
    add(if bytes.len() < 256 { 3 } else { 6 }, bytes.len())
}

fn list_cells(
    arena: &PolicySetPatchArena<'_>,
    start: usize,
    len: usize,
    charge: &mut PolicySchemaCharge,
) -> Result<usize, Error> {
    if len == 0 {
        return width(1, charge);
    }
    let mut packed = true;
    for index in 0..len {
        debit(charge, 128)?;
        let child = *arena
            .edges
            .get(start.checked_add(index).ok_or_else(overflow)?)
            .ok_or(Error::Malformed)?;
        let byte = match *arena.cells.get(child).ok_or(Error::Malformed)? {
            PolicySetPatchCell::Raw(value) => is_byte(value, charge)?,
            _ => false,
        };
        if !byte || index == 65_535 {
            packed = false;
            break;
        }
    }
    if packed {
        return width(add(3, len)?, charge);
    }
    let mut size = width(5, charge)?;
    for index in 0..len {
        let child = *arena
            .edges
            .get(start.checked_add(index).ok_or_else(overflow)?)
            .ok_or(Error::Malformed)?;
        size = add(size, cell(arena, child, charge)?)?;
    }
    add(size, width(1, charge)?)
}

fn list_raw(input: Raw<'_>, charge: &mut PolicySchemaCharge) -> Result<usize, Error> {
    if matches!(input, Raw::List(items) if items.is_empty()) {
        return width(1, charge);
    }
    let mut count = 0;
    loop {
        let (head, proper) = item(input, count, charge)?;
        let Some(head) = head else {
            if proper {
                return width(add(3, count)?, charge);
            }
            break;
        };
        if !is_byte(head, charge)? || count == 65_535 {
            break;
        }
        count += 1;
    }
    let size = width(5, charge)?;
    add(size, raw_list_contents(input, charge)?)
}

fn raw_list_contents(input: Raw<'_>, charge: &mut PolicySchemaCharge) -> Result<usize, Error> {
    let (heads, tail) = match input {
        Raw::List(heads) => (heads, None),
        Raw::ImproperList { prefix, tail } => (prefix, Some(*tail)),
        value => return raw(value, charge),
    };
    let mut size = 0;
    for &head in heads {
        size = add(size, raw(head, charge)?)?;
    }
    match tail {
        None => add(size, width(1, charge)?),
        Some(tail) => {
            debit(charge, 128)?;
            add(size, raw_list_contents(tail, charge)?)
        }
    }
}

fn is_byte(value: Raw<'_>, charge: &mut PolicySchemaCharge) -> Result<bool, Error> {
    match value {
        Raw::Integer(value) => Ok(u8::try_from(value).is_ok()),
        Raw::BigInteger {
            negative,
            little_endian,
        } => {
            let bytes = significant(little_endian, charge)?;
            Ok(bytes.is_empty() || (!negative && bytes.len() == 1))
        }
        _ => Ok(false),
    }
}

fn width(size: usize, charge: &mut PolicySchemaCharge) -> Result<usize, Error> {
    debit(charge, size)?;
    Ok(size)
}

fn add(a: usize, b: usize) -> Result<usize, Error> {
    a.checked_add(b).ok_or_else(overflow)
}
