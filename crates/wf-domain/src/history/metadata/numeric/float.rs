//! Source short notation over standard shortest-round-trip finite digits.

use std::fmt::Write as _;

use super::{Budget, E, ScalarText, copy};

pub(in crate::history::metadata) fn render(bits: u64, charge: &mut Budget) -> Result<Vec<u8>, E> {
    let value = f64::from_bits(bits);
    if !value.is_finite() {
        return Err(E::InvalidInput);
    }
    charge.spend(4_096)?;
    let mut fixed = ScalarText::new();
    write!(&mut fixed, "{value}").map_err(|_| E::Limit)?;
    if !fixed.as_bytes().contains(&b'.') {
        fixed.push(b".0").map_err(|_| E::Limit)?;
    }
    let mut raw = ScalarText::new();
    write!(&mut raw, "{value:e}").map_err(|_| E::Limit)?;
    let exponent = raw
        .as_bytes()
        .iter()
        .position(|byte| *byte == b'e')
        .ok_or(E::InvalidInput)?;
    let (mantissa, exponent) = raw.as_bytes().split_at(exponent);
    let mut scientific = ScalarText::new();
    scientific.push(mantissa).map_err(|_| E::Limit)?;
    if !mantissa.contains(&b'.') {
        scientific.push(b".0").map_err(|_| E::Limit)?;
    }
    scientific.push(exponent).map_err(|_| E::Limit)?;
    let selected = if value.abs() < 9_007_199_254_740_992.0
        && fixed.as_bytes().len() <= scientific.as_bytes().len()
    {
        fixed.as_bytes()
    } else {
        scientific.as_bytes()
    };
    copy(selected, charge)
}
