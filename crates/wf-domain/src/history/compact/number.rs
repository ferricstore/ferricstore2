//! Mathematical scalar equality and arbitrary magnitude emission without owners.

use crate::metadata::OriginalMetadataInput as M;

use super::{
    OriginalCompactHistoryError as E,
    budget::{Budget, add, product},
    primitives::Output,
};

pub(super) fn is_integer(value: M<'_, '_>) -> bool {
    matches!(value, M::Integer(_) | M::BigInteger { .. })
}

pub(super) fn input(value: M<'_, '_>, budget: &mut Budget) -> Result<(), E> {
    budget.spend(1)?;
    if let M::BigInteger { little_endian, .. } = value {
        budget.input(little_endian.len())?;
    }
    Ok(())
}

fn magnitude<'a>(
    value: M<'_, 'a>,
    scratch: &'a mut [u8; 16],
    budget: &mut Budget,
) -> Result<Option<(bool, &'a [u8])>, E> {
    budget.spend(17)?;
    let (negative, bytes) = match value {
        M::Integer(value) => {
            *scratch = value.unsigned_abs().to_le_bytes();
            (value < 0, scratch.as_slice())
        }
        M::BigInteger {
            negative,
            little_endian,
        } => (negative, little_endian),
        _ => return Ok(None),
    };
    budget.spend(add(bytes.len(), 1)?)?;
    let mut end = bytes.len();
    while end > 0 && bytes[end - 1] == 0 {
        end -= 1;
    }
    Ok(Some((negative && end != 0, &bytes[..end])))
}

pub(super) fn equal(a: M<'_, '_>, b: M<'_, '_>, budget: &mut Budget) -> Result<bool, E> {
    let mut left = [0; 16];
    let mut right = [0; 16];
    let a = magnitude(a, &mut left, budget)?;
    let b = magnitude(b, &mut right, budget)?;
    let (Some((an, ab)), Some((bn, bb))) = (a, b) else {
        return Ok(false);
    };
    budget.spend(add(add(ab.len(), bb.len())?, 1)?)?;
    Ok(an == bn && ab == bb)
}

pub(super) fn write(
    value: M<'_, '_>,
    output: &mut Output<'_>,
    budget: &mut Budget,
) -> Result<(), E> {
    let mut scratch = [0; 16];
    let Some((false, bytes)) = magnitude(value, &mut scratch, budget)? else {
        return output.byte(0, budget);
    };
    let bits = match bytes.last() {
        Some(last) => add(
            product(bytes.len() - 1, 8)?,
            (8 - last.leading_zeros()) as usize,
        )?,
        None => 0,
    };
    let groups = bits.div_ceil(7).max(1);
    let mut carry = 1_u16;
    for group in 0..groups {
        budget.spend(8)?;
        let bit = product(group, 7)?;
        let index = bit / 8;
        let low = u16::from(bytes.get(index).copied().unwrap_or(0));
        let high = u16::from(bytes.get(add(index, 1)?).copied().unwrap_or(0));
        let chunk = (((low | (high << 8)) >> (bit % 8)) & 127) + carry;
        carry = chunk >> 7;
        let continuation = u8::from(group + 1 < groups || carry != 0) << 7;
        output.byte(
            u8::try_from(chunk & 127).map_err(|_| E::InvalidInput)? | continuation,
            budget,
        )?;
    }
    if carry != 0 {
        output.byte(1, budget)?;
    }
    Ok(())
}
