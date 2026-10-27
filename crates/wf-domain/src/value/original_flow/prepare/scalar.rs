//! Scalar mathematical validation and canonical ETF widths.

use crate::value::original_flow::OriginalValueNode as N;
use crate::value::original_flow::prepare::budget::{Budget, add, wire_length};
use crate::value::original_flow::prepare::{
    OriginalValueEncodeError as E, OriginalValueInput as I,
};

pub(in crate::value::original_flow::prepare) fn integer_size(value: i64) -> usize {
    if (0..=255).contains(&value) {
        2
    } else if i32::try_from(value).is_ok() {
        5
    } else {
        3 + magnitude_length(value.unsigned_abs())
    }
}

pub(in crate::value::original_flow::prepare) fn magnitude_length(value: u64) -> usize {
    value.bit_width().div_ceil(8) as usize
}

pub(in crate::value::original_flow::prepare) fn bytes<'a>(
    input: I<'a>,
    range: &std::ops::Range<usize>,
) -> Result<&'a [u8], E> {
    input.bytes.get(range.clone()).ok_or(E::InvalidInput)
}

pub(in crate::value::original_flow::prepare) fn atom_length(
    bytes: &[u8],
    utf8: bool,
    budget: &mut Budget,
) -> Result<usize, E> {
    budget.spend(add(bytes.len(), 1)?)?;
    if utf8 {
        let name = std::str::from_utf8(bytes).map_err(|_| E::InvalidInput)?;
        if name.chars().count() > 255 {
            return Err(E::InvalidInput);
        }
        Ok(bytes.len())
    } else {
        if bytes.len() > 255 {
            return Err(E::InvalidInput);
        }
        bytes
            .iter()
            .try_fold(0, |total, byte| add(total, usize::from(*byte >= 128) + 1))
    }
}

pub(in crate::value::original_flow::prepare) fn big_valid(
    bytes: &[u8],
    negative: bool,
    budget: &mut Budget,
) -> Result<(), E> {
    budget.spend(add(bytes.len(), 1)?)?;
    if bytes.last().is_none_or(|last| *last == 0) {
        return Err(E::InvalidInput);
    }
    if bytes.len() < 8 {
        return Err(E::InvalidInput);
    }
    if bytes.len() == 8 {
        let magnitude = u64::from_le_bytes(bytes.try_into().map_err(|_| E::InvalidInput)?);
        let boundary = if negative {
            1_u64 << 63
        } else {
            i64::MAX.unsigned_abs()
        };
        if magnitude <= boundary {
            return Err(E::InvalidInput);
        }
    }
    Ok(())
}

pub(in crate::value::original_flow::prepare) fn size(
    input: I<'_>,
    node: &N,
    budget: &mut Budget,
) -> Result<Option<(usize, usize)>, E> {
    let size = match node {
        N::EmptyList => 1,
        N::Integer(value) => integer_size(*value),
        N::FloatBits(bits) => {
            if !f64::from_bits(*bits).is_finite() {
                return Err(E::InvalidInput);
            }
            9
        }
        N::BigInteger { negative, digits } => {
            let digits = bytes(input, digits)?;
            big_valid(digits, *negative, budget)?;
            wire_length(digits.len())?;
            add(if digits.len() <= 255 { 3 } else { 6 }, digits.len())?
        }
        N::Atom { bytes: range, utf8 } => {
            let length = atom_length(bytes(input, range)?, *utf8, budget)?;
            return Ok(Some((
                add(if length <= 255 { 2 } else { 3 }, length)?,
                length,
            )));
        }
        N::Binary { bytes: range } => {
            let length = bytes(input, range)?.len();
            wire_length(length)?;
            add(5, length)?
        }
        N::BitString {
            bytes: range,
            last_bits,
        } => {
            let length = bytes(input, range)?.len();
            if length == 0 || !(1..=8).contains(last_bits) {
                return Err(E::InvalidInput);
            }
            wire_length(length)?;
            add(if *last_bits == 8 { 5 } else { 6 }, length)?
        }
        _ => return Ok(None),
    };
    Ok(Some((size, 0)))
}
