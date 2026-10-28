//! Decimal integer conversion with one admitted scratch owner.

use std::fmt::Write as _;

use super::OriginalHistoryMetadataError as E;
use super::budget::{Budget, add, mul};
use super::text::ScalarText;

mod float;
pub(super) use float::render as float;

pub(super) fn integer(value: i64, charge: &mut Budget) -> Result<Vec<u8>, E> {
    charge.spend(1_024)?;
    let mut text = ScalarText::new();
    write!(&mut text, "{value}").map_err(|_| E::Limit)?;
    copy(text.as_bytes(), charge)
}

pub(super) fn big_integer(bytes: &[u8], negative: bool, charge: &mut Budget) -> Result<Vec<u8>, E> {
    charge.spend(mul(bytes.len(), 4)?)?;
    let width = bytes
        .iter()
        .rposition(|byte| *byte != 0)
        .map_or(0, |last| last + 1);
    let digits = add(mul(width, 3)?, 1)?;
    // Every division scans at most width bytes and emits one decimal digit.
    charge.spend(mul(mul(add(width, 1)?, digits)?, 8)?)?;
    let mut scratch = charge.reserve::<u8>(width)?;
    scratch.extend_from_slice(&bytes[..width]);
    let mut output = charge.reserve::<u8>(digits)?;
    if width == 0 {
        output.push(b'0');
    }
    while !scratch.is_empty() {
        output.push(b'0' + divide(&mut scratch)?);
    }
    if negative && width != 0 {
        output.push(b'-');
    }
    output.reverse();
    let scratch_capacity = scratch.capacity();
    drop(scratch);
    charge.release(scratch_capacity)?;
    charge.output(output.len())?;
    Ok(output)
}

fn divide(bytes: &mut Vec<u8>) -> Result<u8, E> {
    let mut remainder = 0_u16;
    for byte in bytes.iter_mut().rev() {
        let value = remainder * 256 + u16::from(*byte);
        *byte = u8::try_from(value / 10).map_err(|_| E::InvalidInput)?;
        remainder = value % 10;
    }
    while bytes.last().is_some_and(|byte| *byte == 0) {
        let _ = bytes.pop();
    }
    u8::try_from(remainder).map_err(|_| E::InvalidInput)
}

pub(super) fn copy(bytes: &[u8], charge: &mut Budget) -> Result<Vec<u8>, E> {
    charge.output(bytes.len())?;
    let mut output = charge.reserve(bytes.len())?;
    output.extend_from_slice(bytes);
    Ok(output)
}
