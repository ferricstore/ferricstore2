//! Source finite scalar grammar and cached mathematical normalization facts.

use std::cmp::Ordering;

use super::{
    Error, OriginalEtfSafety, PolicySchemaCharge, TermPosition as Position, TermSpan, overflow,
    safety, scan,
};

pub(super) fn parse(
    raw: &[u8],
    context: &OriginalEtfSafety<'_>,
    cursor: &mut usize,
    span: &mut TermSpan,
    charge: &mut PolicySchemaCharge,
) -> Result<(), Error> {
    match span.tag() {
        97 => span.keys = u32::from(scan::byte(raw, cursor, charge)?),
        98 => span.keys = scan::word(raw, cursor, charge)?,
        110 | 111 => big(raw, cursor, span, charge)?,
        70 => {
            let bits = u64::from_be_bytes(
                scan::take(raw, cursor, 8, charge)?
                    .try_into()
                    .map_err(|_| Error::Malformed)?,
            );
            if !f64::from_bits(bits).is_finite() {
                return Err(Error::Malformed);
            }
            span.keys = u32::try_from(bits & u64::from(u32::MAX)).map_err(|_| overflow())?;
            span.canonical = u32::try_from(bits >> 32).map_err(|_| overflow())?;
        }
        99 => {
            let bytes = scan::take(raw, cursor, 31, charge)?;
            let bits = legacy_float(bytes)?.to_bits();
            span.keys = u32::try_from(bits & u64::from(u32::MAX)).map_err(|_| overflow())?;
            span.canonical = u32::try_from(bits >> 32).map_err(|_| overflow())?;
        }
        100 | 115 | 118 | 119 => {
            let length = if matches!(span.tag(), 100 | 118) {
                usize::from(scan::short(raw, cursor, charge)?)
            } else {
                usize::from(scan::byte(raw, cursor, charge)?)
            };
            let latin = matches!(span.tag(), 100 | 115);
            let bytes = scan::take(raw, cursor, length, charge)?;
            span.set_datum(safety::atom(context, bytes, latin, charge)?);
        }
        109 | 77 => {
            let length =
                usize::try_from(scan::word(raw, cursor, charge)?).map_err(|_| overflow())?;
            let bits = if span.tag() == 77 {
                scan::byte(raw, cursor, charge)?
            } else {
                8
            };
            if span.tag() == 77 && length == 0 {
                // Pinned OTP28.3.1 underflows its native size for len0/bits0.
                return Err(if bits == 0 {
                    Error::UnsupportedTag
                } else {
                    Error::Malformed
                });
            }
            if (length != 0 && !(1..=8).contains(&bits)) || (length == 0 && bits != 0 && bits != 8)
            {
                return Err(Error::Malformed);
            }
            span.canonical = u32::try_from(*cursor).map_err(|_| overflow())?;
            scan::take(raw, cursor, length, charge)?;
            span.keys = u32::try_from(
                length
                    .checked_mul(8)
                    .and_then(|n| {
                        n.checked_sub(if length == 0 {
                            0
                        } else {
                            usize::from(8 - bits)
                        })
                    })
                    .ok_or_else(overflow)?,
            )
            .map_err(|_| overflow())?;
        }
        _ => return Err(Error::Malformed),
    }
    Ok(())
}

fn big(
    raw: &[u8],
    cursor: &mut usize,
    span: &mut TermSpan,
    charge: &mut PolicySchemaCharge,
) -> Result<(), Error> {
    let length = if span.tag() == 110 {
        usize::from(scan::byte(raw, cursor, charge)?)
    } else {
        usize::try_from(scan::word(raw, cursor, charge)?).map_err(|_| overflow())?
    };
    let sign = scan::byte(raw, cursor, charge)?;
    if sign > 1 {
        return Err(Error::Malformed);
    }
    let start = *cursor;
    let bytes = scan::take(raw, cursor, length, charge)?;
    let significant = bytes
        .iter()
        .rposition(|&byte| byte != 0)
        .map_or(0, |index| index + 1);
    span.canonical = u32::try_from(start).map_err(|_| overflow())?;
    span.keys = u32::try_from(significant).map_err(|_| overflow())?;
    span.set_datum(u32::from(significant != 0 && sign == 1));
    Ok(())
}

pub(super) fn signed(raw: &[u8], span: TermSpan) -> Option<i64> {
    match span.tag() {
        97 => Some(i64::from(span.keys)),
        98 => Some(i64::from(i32::from_be_bytes(span.keys.to_be_bytes()))),
        110 | 111 if span.keys <= 8 => {
            let bytes = raw.get(
                usize::try_from(span.canonical).ok()?
                    ..usize::try_from(span.canonical.checked_add(span.keys)?).ok()?,
            )?;
            let mut magnitude = [0_u8; 8];
            magnitude[..bytes.len()].copy_from_slice(bytes);
            let magnitude = u64::from_le_bytes(magnitude);
            if span.datum() == 1 {
                i64::try_from(magnitude).ok()?.checked_neg()
            } else {
                i64::try_from(magnitude).ok()
            }
        }
        _ => None,
    }
}

pub(super) fn small(raw: &[u8], span: TermSpan) -> Result<i64, Error> {
    let value = signed(raw, span).ok_or(Error::Malformed)?;
    if !(-(1_i64 << 59)..(1_i64 << 59)).contains(&value) {
        return Err(Error::Malformed);
    }
    Ok(value)
}

fn legacy_float(bytes: &[u8]) -> Result<f64, Error> {
    let end = bytes.iter().position(|&b| b == 0).ok_or(Error::Malformed)?;
    let text = &bytes[..end];
    let mut index = usize::from(matches!(text.first(), Some(b'+' | b'-')));
    let start = index;
    while text.get(index).is_some_and(u8::is_ascii_digit) {
        index += 1;
    }
    if index == start || !matches!(text.get(index), Some(b'.' | b',')) {
        return Err(Error::Malformed);
    }
    index += 1;
    let start = index;
    while text.get(index).is_some_and(u8::is_ascii_digit) {
        index += 1;
    }
    if index == start {
        return Err(Error::Malformed);
    }
    if matches!(text.get(index), Some(b'e' | b'E')) {
        index += 1;
        index += usize::from(matches!(text.get(index), Some(b'+' | b'-')));
        let start = index;
        while text.get(index).is_some_and(u8::is_ascii_digit) {
            index += 1;
        }
        if index == start {
            return Err(Error::Malformed);
        }
    }
    if index != text.len() {
        return Err(Error::Malformed);
    }
    let mut normalized = [0_u8; 31];
    normalized[..end].copy_from_slice(text);
    for byte in &mut normalized[..end] {
        if *byte == b',' {
            *byte = b'.';
        }
    }
    let value = std::str::from_utf8(&normalized[..end])
        .map_err(|_| Error::Malformed)?
        .parse::<f64>()
        .map_err(|_| Error::Malformed)?;
    if !value.is_finite() {
        return Err(Error::Malformed);
    }
    Ok(value)
}

pub(super) fn binary_cmp(raw: &[u8], left: TermSpan, right: TermSpan) -> Ordering {
    let bits = left.keys.min(right.keys);
    let count = (bits / 8) as usize;
    let (a, b) = (left.canonical as usize, right.canonical as usize);
    let order = raw[a..a + count].cmp(&raw[b..b + count]);
    if order != Ordering::Equal {
        return order;
    }
    let partial = bits % 8;
    if partial != 0 {
        let mask = 255_u8 << (8 - partial);
        let order = (raw[a + count] & mask).cmp(&(raw[b + count] & mask));
        if order != Ordering::Equal {
            return order;
        }
    }
    left.keys.cmp(&right.keys)
}

pub(super) fn integer_length(raw: &[u8], span: TermSpan) -> usize {
    if matches!(span.tag(), 110 | 111) {
        (span.keys as usize).max(1)
    } else {
        ((64 - signed(raw, span)
            .unwrap_or(0)
            .unsigned_abs()
            .leading_zeros())
        .div_ceil(8) as usize)
            .max(1)
    }
}

fn integer_byte(raw: &[u8], span: TermSpan, index: usize) -> u8 {
    if matches!(span.tag(), 110 | 111) {
        if index < span.keys as usize {
            raw[span.canonical as usize + index]
        } else {
            0
        }
    } else {
        signed(raw, span).unwrap_or(0).unsigned_abs().to_le_bytes()[index]
    }
}

pub(super) fn integer_cmp(
    raw: &[u8],
    spans: &[TermSpan],
    left: Position,
    right: Position,
) -> Ordering {
    let facts = |position| match position {
        Position::Byte(value) => (false, 1_usize, None, value),
        Position::Encoded(index) => {
            let span = spans[index as usize];
            let negative = if matches!(span.tag(), 110 | 111) {
                span.datum() == 1
            } else {
                signed(raw, span).is_some_and(|n| n < 0)
            };
            (negative, integer_length(raw, span), Some(span), 0)
        }
        _ => (false, 1, None, 0),
    };
    let (negative_a, length_a, span_a, byte_a) = facts(left);
    let (negative_b, length_b, span_b, byte_b) = facts(right);
    if negative_a != negative_b {
        return negative_b.cmp(&negative_a);
    }
    let mut order = length_a.cmp(&length_b);
    if order == Ordering::Equal {
        for index in (0..length_a).rev() {
            let a = span_a.map_or(byte_a, |s| integer_byte(raw, s, index));
            let b = span_b.map_or(byte_b, |s| integer_byte(raw, s, index));
            order = a.cmp(&b);
            if order != Ordering::Equal {
                break;
            }
        }
    }
    if negative_a { order.reverse() } else { order }
}

pub(super) fn kind(spans: &[TermSpan], position: Position) -> u8 {
    match position {
        Position::Byte(_) => 1,
        Position::Nil => 4,
        Position::StringCell { .. } => 5,
        Position::Encoded(index) => match spans[index as usize].tag() {
            100 | 115 | 118 | 119 => 0,
            97 | 98 | 110 | 111 => 1,
            70 | 99 => 2,
            109 | 77 => 3,
            104 | 105 => 6,
            116 => 7,
            103 | 88 => 8,
            102 | 89 | 120 => 9,
            101 | 114 | 90 => 10,
            112 => 11,
            113 => 12,
            _ => 255,
        },
    }
}
