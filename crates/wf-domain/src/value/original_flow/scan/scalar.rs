//! Scalar wire families and the bounded source `FLOAT_EXT` grammar.

use std::ops::Range;

use crate::value::original_flow::context::check_atom;
use crate::value::original_flow::scan::{Scanner, SummaryKind, TermSummary};
use crate::value::original_flow::{OriginalFlowValueError as E, OriginalValueNode as N};

pub(in crate::value::original_flow) fn parse(
    scan: &mut Scanner<'_, '_, '_, '_>,
    tag: u8,
    offset: usize,
) -> Result<Option<TermSummary>, E> {
    match tag {
        97 | 98 => {
            let range = scan.take(if tag == 97 { 1 } else { 4 })?;
            let bytes = scan.input.get(range).ok_or(E::InvalidFlowValue)?;
            let value = if tag == 97 {
                i64::from(*bytes.first().ok_or(E::InvalidFlowValue)?)
            } else {
                i64::from(i32::from_be_bytes(
                    bytes.try_into().map_err(|_| E::InvalidFlowValue)?,
                ))
            };
            Ok(Some(scan.emit(
                N::Integer(value),
                SummaryKind::Integer(value),
                (tag, offset),
            )?))
        }
        110 | 111 => big_integer(scan, tag, offset),
        70 => {
            let range = scan.take(8)?;
            let bits = u64::from_be_bytes(
                scan.input
                    .get(range)
                    .ok_or(E::InvalidFlowValue)?
                    .try_into()
                    .map_err(|_| E::InvalidFlowValue)?,
            );
            if !f64::from_bits(bits).is_finite() {
                return Err(E::InvalidFlowValue);
            }
            Ok(Some(scan.emit(
                N::FloatBits(bits),
                SummaryKind::Other,
                (tag, offset),
            )?))
        }
        99 => legacy_float(scan, tag, offset),
        100 | 115 | 118 | 119 => atom(scan, tag, offset),
        109 | 77 => binary(scan, tag, offset),
        73 | 75 | 121 => {
            scan.take(match tag {
                73 => 2,
                75 => 3,
                _ => 4,
            })?;
            if tag == 121 && scan.cursor == scan.input.len() {
                return Err(E::InvalidFlowValue);
            }
            Err(E::UnsupportedContext { tag, offset })
        }
        _ => Err(E::InvalidFlowValue),
    }
}

pub(in crate::value::original_flow) fn big_integer(
    scan: &mut Scanner<'_, '_, '_, '_>,
    tag: u8,
    offset: usize,
) -> Result<Option<TermSummary>, E> {
    let count = scan.take(if tag == 110 { 1 } else { 4 })?;
    let bytes = scan.input.get(count).ok_or(E::InvalidFlowValue)?;
    let length = if tag == 110 {
        usize::from(*bytes.first().ok_or(E::InvalidFlowValue)?)
    } else {
        usize::try_from(u32::from_be_bytes(
            bytes.try_into().map_err(|_| E::InvalidFlowValue)?,
        ))
        .map_err(|_| E::Limit)?
    };
    let sign = scan.take(1)?;
    let negative = *scan.input.get(sign.start).ok_or(E::InvalidFlowValue)? != 0;
    let digits = scan.take(length)?;
    normalize_integer(scan, negative, digits, (tag, offset))
}

pub(in crate::value::original_flow) fn normalize_integer(
    scan: &mut Scanner<'_, '_, '_, '_>,
    negative: bool,
    mut digits: Range<usize>,
    site: (u8, usize),
) -> Result<Option<TermSummary>, E> {
    while digits.end > digits.start {
        scan.budget.spend(1)?;
        if *scan.input.get(digits.end - 1).ok_or(E::InvalidFlowValue)? != 0 {
            break;
        }
        digits.end -= 1;
    }
    let mut small = None;
    if digits.len() <= 8 {
        scan.budget.spend(digits.len())?;
        let mut magnitude = 0_u64;
        for byte in scan
            .input
            .get(digits.clone())
            .ok_or(E::InvalidFlowValue)?
            .iter()
            .rev()
        {
            magnitude = magnitude
                .checked_mul(256)
                .and_then(|n| n.checked_add(u64::from(*byte)))
                .ok_or(E::Limit)?;
        }
        if !negative || magnitude == 0 {
            small = i64::try_from(magnitude).ok();
        } else if magnitude == 1_u64 << 63 {
            small = Some(i64::MIN);
        } else if let Ok(value) = i64::try_from(magnitude) {
            small = Some(-value);
        }
    }
    if let Some(value) = small {
        Ok(Some(scan.emit(
            N::Integer(value),
            SummaryKind::Integer(value),
            site,
        )?))
    } else {
        Ok(Some(scan.emit(
            N::BigInteger { negative, digits },
            SummaryKind::Other,
            site,
        )?))
    }
}

pub(in crate::value::original_flow) fn legacy_float(
    scan: &mut Scanner<'_, '_, '_, '_>,
    tag: u8,
    offset: usize,
) -> Result<Option<TermSummary>, E> {
    let field = scan.take(31)?;
    let bytes = scan.input.get(field).ok_or(E::InvalidFlowValue)?;
    let mut end = None;
    for (index, byte) in bytes.iter().enumerate() {
        scan.budget.spend(1)?;
        if *byte == 0 {
            end = Some(index);
            break;
        }
    }
    let end = end.ok_or(E::InvalidFlowValue)?;
    scan.budget.spend(31)?;
    let mut normalized = [0_u8; 31];
    scan.budget.spend(end)?;
    normalized[..end].copy_from_slice(&bytes[..end]);
    let (mut phase, mut integer, mut fraction, mut exponent, mut sign) =
        (0, false, false, false, true);
    for (index, byte) in normalized[..end].iter_mut().enumerate() {
        scan.budget.spend(1)?;
        if index == 0 && matches!(*byte, b'+' | b'-') {
            continue;
        }
        match phase {
            0 if byte.is_ascii_digit() => integer = true,
            0 if integer && matches!(*byte, b'.' | b',') => {
                phase = 1;
                *byte = b'.';
            }
            1 if byte.is_ascii_digit() => fraction = true,
            1 if fraction && matches!(*byte, b'e' | b'E') => phase = 2,
            2 if byte.is_ascii_digit() => {
                exponent = true;
                sign = false;
            }
            2 if sign && matches!(*byte, b'+' | b'-') => sign = false,
            _ => return Err(E::InvalidFlowValue),
        }
    }
    if !integer || !fraction || (phase == 2 && !exponent) {
        return Err(E::InvalidFlowValue);
    }
    scan.budget.spend(end)?;
    let text = std::str::from_utf8(&normalized[..end]).map_err(|_| E::InvalidFlowValue)?;
    scan.budget.spend(end)?;
    let value = text.parse::<f64>().map_err(|_| E::InvalidFlowValue)?;
    if !value.is_finite() {
        return Err(E::InvalidFlowValue);
    }
    Ok(Some(scan.emit(
        N::FloatBits(value.to_bits()),
        SummaryKind::Other,
        (tag, offset),
    )?))
}

pub(in crate::value::original_flow) fn atom(
    scan: &mut Scanner<'_, '_, '_, '_>,
    tag: u8,
    offset: usize,
) -> Result<Option<TermSummary>, E> {
    let header = scan.take(if matches!(tag, 115 | 119) { 1 } else { 2 })?;
    let bytes = scan.input.get(header).ok_or(E::InvalidFlowValue)?;
    let length = if matches!(tag, 115 | 119) {
        usize::from(*bytes.first().ok_or(E::InvalidFlowValue)?)
    } else {
        usize::from(u16::from_be_bytes(
            bytes.try_into().map_err(|_| E::InvalidFlowValue)?,
        ))
    };
    let utf8 = matches!(tag, 118 | 119);
    if length > if utf8 { 1020 } else { 255 } {
        return Err(E::InvalidFlowValue);
    }
    let bytes = scan.take(length)?;
    if utf8 {
        scan.budget.spend(length)?;
        let text = std::str::from_utf8(scan.input.get(bytes.clone()).ok_or(E::InvalidFlowValue)?)
            .map_err(|_| E::InvalidFlowValue)?;
        scan.budget.spend(length)?;
        if text.chars().count() > 255 {
            return Err(E::InvalidFlowValue);
        }
    }
    let kind = SummaryKind::Atom {
        start: bytes.start,
        end: bytes.end,
        utf8,
    };
    let summary = TermSummary {
        id: scan.counts.finished,
        tag,
        offset,
        kind,
    };
    check_atom(scan.context, scan.state, scan.input, summary, scan.budget)?;
    Ok(Some(scan.emit(
        N::Atom { bytes, utf8 },
        kind,
        (tag, offset),
    )?))
}

pub(in crate::value::original_flow) fn binary(
    scan: &mut Scanner<'_, '_, '_, '_>,
    tag: u8,
    offset: usize,
) -> Result<Option<TermSummary>, E> {
    let header = scan.take(4)?;
    let length = usize::try_from(u32::from_be_bytes(
        scan.input
            .get(header)
            .ok_or(E::InvalidFlowValue)?
            .try_into()
            .map_err(|_| E::InvalidFlowValue)?,
    ))
    .map_err(|_| E::Limit)?;
    let bits = if tag == 77 {
        let range = scan.take(1)?;
        *scan.input.get(range.start).ok_or(E::InvalidFlowValue)?
    } else {
        8
    };
    if tag == 77 && length == 0 && bits == 0 {
        return Err(E::UnsupportedProfile { tag, offset });
    }
    if bits == 0 || bits > 8 || (tag == 77 && length == 0) {
        return Err(E::InvalidFlowValue);
    }
    let bytes = scan.take(length)?;
    let node = if bits == 8 {
        N::Binary { bytes }
    } else {
        N::BitString {
            bytes,
            last_bits: bits,
        }
    };
    Ok(Some(scan.emit(node, SummaryKind::Other, (tag, offset))?))
}
