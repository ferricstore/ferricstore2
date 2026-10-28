//! Selected leaf conversion; unsupported compounds are never traversed.

use std::borrow::Cow;
use std::ops::Range;

use super::OriginalHistoryMetadataError as E;
use super::budget::{Budget, add, mul};
use super::numeric;
use crate::value::{OriginalValueInput as I, OriginalValueNode as N};

pub(super) const fn supported_key(node: &N) -> bool {
    matches!(
        node,
        N::Atom { .. } | N::Binary { .. } | N::BitString { last_bits: 8, .. }
    )
}

pub(super) const fn supported_value(node: &N) -> bool {
    supported_key(node) || matches!(node, N::Integer(_) | N::BigInteger { .. } | N::FloatBits(_))
}

pub(super) fn text<'a>(
    input: I<'a>,
    node: &N,
    nil_empty: bool,
    charge: &mut Budget,
) -> Result<Cow<'a, [u8]>, E> {
    match node {
        N::Binary { bytes }
        | N::BitString {
            bytes,
            last_bits: 8,
        } => {
            let bytes = span(input, bytes)?;
            charge.output(bytes.len())?;
            Ok(Cow::Borrowed(bytes))
        }
        N::Atom { bytes, utf8 } => atom(span(input, bytes)?, *utf8, nil_empty, charge),
        N::Integer(value) => Ok(Cow::Owned(numeric::integer(*value, charge)?)),
        N::BigInteger { negative, digits } => Ok(Cow::Owned(numeric::big_integer(
            span(input, digits)?,
            *negative,
            charge,
        )?)),
        N::FloatBits(bits) => Ok(Cow::Owned(numeric::float(*bits, charge)?)),
        _ => Err(E::InvalidInput),
    }
}

fn span<'a>(input: I<'a>, range: &Range<usize>) -> Result<&'a [u8], E> {
    input.bytes.get(range.clone()).ok_or(E::InvalidInput)
}

fn atom<'a>(
    bytes: &'a [u8],
    utf8: bool,
    nil_empty: bool,
    charge: &mut Budget,
) -> Result<Cow<'a, [u8]>, E> {
    charge.spend(mul(bytes.len(), 8)?)?;
    if utf8 {
        if std::str::from_utf8(bytes)
            .map_err(|_| E::InvalidInput)?
            .chars()
            .count()
            > 255
        {
            return Err(E::InvalidInput);
        }
    } else if bytes.len() > 255 {
        return Err(E::InvalidInput);
    }
    if nil_empty && bytes == b"nil" {
        charge.output(0)?;
        return Ok(Cow::Borrowed(b""));
    }
    let extra = if utf8 {
        0
    } else {
        bytes.iter().filter(|byte| **byte >= 128).count()
    };
    charge.output(add(bytes.len(), extra)?)?;
    if extra == 0 {
        return Ok(Cow::Borrowed(bytes));
    }
    let mut output = charge.reserve(add(bytes.len(), extra)?)?;
    for byte in bytes {
        if *byte < 128 {
            output.push(*byte);
        } else {
            output.extend_from_slice(&[0xc0 | (byte >> 6), 0x80 | (byte & 63)]);
        }
    }
    Ok(Cow::Owned(output))
}
