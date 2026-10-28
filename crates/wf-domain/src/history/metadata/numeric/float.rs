//! Source shortest notation with exact binary/decimal halfway ties to even.

use std::fmt::Write as _;

use super::{Budget, E, ScalarText, copy};

pub(in crate::history::metadata) fn render(bits: u64, charge: &mut Budget) -> Result<Vec<u8>, E> {
    let value = f64::from_bits(bits);
    if !value.is_finite() {
        return Err(E::InvalidInput);
    }
    charge.spend(4_096)?;
    let negative = value.is_sign_negative();
    if value == 0.0 {
        return copy(if negative { b"-0.0" } else { b"0.0" }, charge);
    }
    let (mut mantissa, mut scale) = shortest(value)?;
    mantissa = ties_to_even(bits, mantissa, scale)?;
    while mantissa.is_multiple_of(10) {
        mantissa /= 10;
        scale += 1;
    }
    let mut digits = ScalarText::new();
    write!(&mut digits, "{mantissa}").map_err(|_| E::Limit)?;
    let digits = digits.as_bytes();
    let scientific = scientific(digits, scale, negative)?;
    let point = i32::try_from(digits.len()).map_err(|_| E::Limit)? + scale;
    if value.abs() < 9_007_199_254_740_992.0
        && fixed_length(digits.len(), point, negative)? <= scientific.as_bytes().len()
    {
        copy(fixed(digits, point, negative)?.as_bytes(), charge)
    } else {
        copy(scientific.as_bytes(), charge)
    }
}

fn shortest(value: f64) -> Result<(u64, i32), E> {
    let mut raw = ScalarText::new();
    write!(&mut raw, "{value:e}").map_err(|_| E::Limit)?;
    let raw = std::str::from_utf8(raw.as_bytes()).map_err(|_| E::InvalidInput)?;
    let (mantissa, exponent) = raw.split_once('e').ok_or(E::InvalidInput)?;
    let exponent: i32 = exponent.parse().map_err(|_| E::InvalidInput)?;
    let fraction = mantissa
        .split_once('.')
        .map_or(0, |(_, digits)| digits.len());
    let digits = mantissa
        .bytes()
        .filter(u8::is_ascii_digit)
        .try_fold(0_u64, |value, byte| {
            value
                .checked_mul(10)
                .and_then(|value| value.checked_add(u64::from(byte - b'0')))
                .ok_or(E::Limit)
        })?;
    Ok((
        digits,
        exponent - i32::try_from(fraction).map_err(|_| E::Limit)?,
    ))
}

fn ties_to_even(bits: u64, mantissa: u64, scale: i32) -> Result<u64, E> {
    if mantissa.is_multiple_of(2) {
        return Ok(mantissa);
    }
    let exponent = i32::try_from((bits >> 52) & 0x7ff).map_err(|_| E::InvalidInput)?;
    let fraction = bits & ((1_u64 << 52) - 1);
    let binary = if exponent == 0 {
        fraction
    } else {
        fraction | (1_u64 << 52)
    };
    let zeros = binary.trailing_zeros();
    let power = if exponent == 0 {
        -1074
    } else {
        exponent - 1075
    } + i32::try_from(zeros).map_err(|_| E::InvalidInput)?;
    if power != scale - 1 {
        return Ok(mantissa);
    }
    // Exact midpoint: odd_binary * 2^power = (2*M +/- 1) * 10^scale / 2.
    // Powers that overflow u128 cannot equal the <=65-bit decimal midpoint.
    let Some(factor) = 5_u128.checked_pow(scale.unsigned_abs()) else {
        return Ok(mantissa);
    };
    let odd = u128::from(binary >> zeros);
    let (target, multiplier) = if scale < 0 {
        let Some(target) = odd.checked_mul(factor) else {
            return Ok(mantissa);
        };
        (target, 1)
    } else {
        (odd, factor)
    };
    let middle = u128::from(mantissa) * 2;
    if (middle - 1).checked_mul(multiplier) == Some(target)
        && (fraction != 0 || exponent <= 1 || lower_neighbor_fits(exponent, scale, factor)?)
    {
        mantissa.checked_sub(1).ok_or(E::InvalidInput)
    } else if (middle + 1).checked_mul(multiplier) == Some(target) {
        mantissa.checked_add(1).ok_or(E::InvalidInput)
    } else {
        Ok(mantissa)
    }
}

fn lower_neighbor_fits(exponent: i32, scale: i32, factor: u128) -> Result<bool, E> {
    // A normal power of two has half-sized spacing below it. Require the
    // decimal half-spacing to fit that lower half-ULP, including its even bound.
    if scale < 0 {
        let shift = 1076 - exponent + scale;
        if shift <= 0 {
            return Ok(true);
        }
        Ok(1_u128
            .checked_shl(u32::try_from(shift).map_err(|_| E::InvalidInput)?)
            .is_some_and(|bound| bound <= factor))
    } else {
        let shift = exponent - 1076 - scale;
        if shift < 0 {
            return Ok(false);
        }
        Ok(1_u128
            .checked_shl(u32::try_from(shift).map_err(|_| E::InvalidInput)?)
            .is_none_or(|bound| factor <= bound))
    }
}

fn fixed_length(digits: usize, point: i32, negative: bool) -> Result<usize, E> {
    let length = if point <= 0 {
        2 + usize::try_from(point.unsigned_abs()).map_err(|_| E::Limit)? + digits
    } else if usize::try_from(point).map_err(|_| E::Limit)? >= digits {
        usize::try_from(point).map_err(|_| E::Limit)? + 2
    } else {
        digits + 1
    };
    Ok(length + usize::from(negative))
}

fn fixed(digits: &[u8], point: i32, negative: bool) -> Result<ScalarText, E> {
    let mut output = ScalarText::new();
    if negative {
        output.push(b"-").map_err(|_| E::Limit)?;
    }
    if point <= 0 {
        output.push(b"0.").map_err(|_| E::Limit)?;
        for _ in 0..point.unsigned_abs() {
            output.push(b"0").map_err(|_| E::Limit)?;
        }
        output.push(digits).map_err(|_| E::Limit)?;
    } else {
        let point = usize::try_from(point).map_err(|_| E::Limit)?;
        if point >= digits.len() {
            output.push(digits).map_err(|_| E::Limit)?;
            for _ in digits.len()..point {
                output.push(b"0").map_err(|_| E::Limit)?;
            }
            output.push(b".0").map_err(|_| E::Limit)?;
        } else {
            let (before, after) = digits.split_at(point);
            output.push(before).map_err(|_| E::Limit)?;
            output.push(b".").map_err(|_| E::Limit)?;
            output.push(after).map_err(|_| E::Limit)?;
        }
    }
    Ok(output)
}

fn scientific(digits: &[u8], scale: i32, negative: bool) -> Result<ScalarText, E> {
    let mut output = ScalarText::new();
    if negative {
        output.push(b"-").map_err(|_| E::Limit)?;
    }
    output.push(&digits[..1]).map_err(|_| E::Limit)?;
    output.push(b".").map_err(|_| E::Limit)?;
    output
        .push(if digits.len() == 1 {
            b"0"
        } else {
            &digits[1..]
        })
        .map_err(|_| E::Limit)?;
    let exponent = scale + i32::try_from(digits.len()).map_err(|_| E::Limit)? - 1;
    write!(&mut output, "e{exponent}").map_err(|_| E::Limit)?;
    Ok(output)
}
