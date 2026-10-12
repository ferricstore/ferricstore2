//! Two freshly charged lexical passes; pass1 never constructs an index owner.

use super::{
    Error, OriginalEtfSafety, PolicySchemaCharge, TermSpan, VirtualLocalFun, compound, overflow,
    pay, runtime, scalar,
};

pub(super) type Output<'a> = Option<(&'a mut Vec<TermSpan>, &'a mut Vec<VirtualLocalFun>)>;
pub(super) type State = (usize, usize, usize, usize);

pub(super) fn run(
    raw: &[u8],
    safety: &OriginalEtfSafety<'_>,
    measured: &mut (usize, usize, usize),
    output: &mut Output<'_>,
    charge: &mut PolicySchemaCharge,
) -> Result<(), Error> {
    let mut state = (0, 0, 0, 0);
    if byte(raw, &mut state.0, charge)? != 131 {
        return Err(Error::Malformed);
    }
    term(raw, safety, &mut state, output, 0, charge)?;
    if state.0 != raw.len() {
        return Err(Error::TrailingBytes);
    }
    *measured = (state.1, state.2, state.3);
    Ok(())
}

pub(super) fn term(
    raw: &[u8],
    safety: &OriginalEtfSafety<'_>,
    state: &mut State,
    output: &mut Output<'_>,
    depth: usize,
    charge: &mut PolicySchemaCharge,
) -> Result<(u32, TermSpan), Error> {
    pay(charge, 128)?;
    let index = u32::try_from(state.1).map_err(|_| overflow())?;
    state.1 = state.1.checked_add(1).ok_or_else(overflow)?;
    if depth > 64 || state.1 > 4_096 {
        return Err(Error::ItemLimit);
    }
    let start = u32::try_from(state.0).map_err(|_| overflow())?;
    let tag = byte(raw, &mut state.0, charge)?;
    let mut span = TermSpan {
        start,
        symbol: u32::from(tag) << 24,
        ..TermSpan::default()
    };
    if let Some((spans, _)) = output {
        spans.push(span);
    }
    let alias = match tag {
        97 | 98 | 110 | 111 | 70 | 99 | 100 | 115 | 118 | 119 | 109 | 77 => {
            scalar::parse(raw, safety, &mut state.0, &mut span, charge)?;
            None
        }
        104 | 105 | 106 | 107 | 108 | 116 => {
            compound::parse(raw, safety, state, output, (depth, &mut span), charge)?
        }
        101 | 114 | 90 | 103 | 88 | 102 | 89 | 120 | 113 | 112 => {
            runtime::parse(raw, safety, state, output, (depth, &mut span), charge)?;
            None
        }
        73 | 75 | 121 => return Err(Error::UnsupportedTag),
        _ => return Err(Error::Malformed),
    };
    span.end = u32::try_from(state.0).map_err(|_| overflow())?;
    span.next = u32::try_from(state.1).map_err(|_| overflow())?;
    if let Some((spans, _)) = output {
        spans[usize::try_from(index).map_err(|_| overflow())?] = span;
    }
    Ok((index, alias.unwrap_or(span)))
}

pub(super) fn take<'a>(
    raw: &'a [u8],
    cursor: &mut usize,
    length: usize,
    charge: &mut PolicySchemaCharge,
) -> Result<&'a [u8], Error> {
    let end = cursor.checked_add(length).ok_or_else(overflow)?;
    let bytes = raw.get(*cursor..end).ok_or(Error::Malformed)?;
    pay(charge, u64::try_from(length).map_err(|_| overflow())?)?;
    *cursor = end;
    Ok(bytes)
}

pub(super) fn byte(
    raw: &[u8],
    cursor: &mut usize,
    charge: &mut PolicySchemaCharge,
) -> Result<u8, Error> {
    Ok(take(raw, cursor, 1, charge)?[0])
}

pub(super) fn word(
    raw: &[u8],
    cursor: &mut usize,
    charge: &mut PolicySchemaCharge,
) -> Result<u32, Error> {
    Ok(u32::from_be_bytes(
        take(raw, cursor, 4, charge)?
            .try_into()
            .map_err(|_| Error::Malformed)?,
    ))
}

pub(super) fn short(
    raw: &[u8],
    cursor: &mut usize,
    charge: &mut PolicySchemaCharge,
) -> Result<u16, Error> {
    Ok(u16::from_be_bytes(
        take(raw, cursor, 2, charge)?
            .try_into()
            .map_err(|_| Error::Malformed)?,
    ))
}
