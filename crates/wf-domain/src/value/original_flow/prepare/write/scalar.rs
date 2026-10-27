//! Canonical scalar bytes; only the output owns a copy of payload data.

use crate::value::original_flow::OriginalValueNode as N;
use crate::value::original_flow::prepare::OriginalValueEncodeError as E;
use crate::value::original_flow::prepare::budget::{Budget, wire_length};
use crate::value::original_flow::prepare::layout::TermView;
use crate::value::original_flow::prepare::scalar::{bytes, magnitude_length};
use crate::value::original_flow::prepare::write::Output;

pub(in crate::value::original_flow::prepare) fn integer(
    output: &mut Output,
    value: i64,
    budget: &mut Budget,
) -> Result<(), E> {
    if let Ok(small) = u8::try_from(value) {
        output.put(&[97, small], budget)
    } else if let Ok(word) = i32::try_from(value) {
        output.byte(98, budget)?;
        output.put(&word.to_be_bytes(), budget)
    } else {
        let magnitude = value.unsigned_abs().to_le_bytes();
        let length = magnitude_length(value.unsigned_abs());
        output.put(
            &[
                110,
                u8::try_from(length).map_err(|_| E::Limit)?,
                u8::from(value < 0),
            ],
            budget,
        )?;
        output.put(magnitude.get(..length).ok_or(E::InvalidInput)?, budget)
    }
}

pub(in crate::value::original_flow::prepare) fn atom(
    view: TermView<'_, '_>,
    id: usize,
    output: &mut Output,
    budget: &mut Budget,
) -> Result<(), E> {
    let id = view.canonical(id)?;
    let N::Atom { bytes: range, utf8 } = view.node(id)? else {
        return Err(E::InvalidInput);
    };
    let length = view.plan(id)?.atom_len;
    if let Ok(length) = u8::try_from(length) {
        output.put(&[119, length], budget)?;
    } else {
        output.byte(118, budget)?;
        output.put(
            &u16::try_from(length).map_err(|_| E::Limit)?.to_be_bytes(),
            budget,
        )?;
    }
    let name = bytes(view.input, range)?;
    if *utf8 {
        output.put(name, budget)
    } else {
        for value in name {
            let mut encoded = [0; 4];
            output.put(
                char::from(*value).encode_utf8(&mut encoded).as_bytes(),
                budget,
            )?;
        }
        Ok(())
    }
}

pub(in crate::value::original_flow::prepare) fn raw<'a>(
    view: TermView<'a, '_>,
    id: usize,
) -> Result<Option<&'a [u8]>, E> {
    match view.node(id)? {
        N::Binary { bytes: range }
        | N::BitString {
            bytes: range,
            last_bits: 8,
        } => Ok(Some(bytes(view.input, range)?)),
        _ => Ok(None),
    }
}

pub(in crate::value::original_flow::prepare) fn node(
    view: TermView<'_, '_>,
    id: usize,
    output: &mut Output,
    budget: &mut Budget,
) -> Result<bool, E> {
    match view.node(id)? {
        N::EmptyList => output.byte(106, budget)?,
        N::Integer(value) => integer(output, *value, budget)?,
        N::FloatBits(bits) => {
            output.byte(70, budget)?;
            output.put(&bits.to_be_bytes(), budget)?;
        }
        N::Atom { .. } => atom(view, id, output, budget)?,
        N::BigInteger { negative, digits } => {
            let value = bytes(view.input, digits)?;
            if let Ok(length) = u8::try_from(value.len()) {
                output.put(&[110, length], budget)?;
            } else {
                output.byte(111, budget)?;
                output.u32(wire_length(value.len())?, budget)?;
            }
            output.byte(u8::from(*negative), budget)?;
            output.put(value, budget)?;
        }
        N::Binary { bytes: range } => binary(bytes(view.input, range)?, output, budget)?,
        N::BitString {
            bytes: range,
            last_bits,
        } => bitstring(bytes(view.input, range)?, *last_bits, output, budget)?,
        _ => return Ok(false),
    }
    Ok(true)
}

fn binary(value: &[u8], output: &mut Output, budget: &mut Budget) -> Result<(), E> {
    output.byte(109, budget)?;
    output.u32(wire_length(value.len())?, budget)?;
    output.put(value, budget)
}

fn bitstring(value: &[u8], last: u8, output: &mut Output, budget: &mut Budget) -> Result<(), E> {
    if last == 8 {
        return binary(value, output, budget);
    }
    output.byte(77, budget)?;
    output.u32(wire_length(value.len())?, budget)?;
    output.byte(last, budget)?;
    let end = value.len().checked_sub(1).ok_or(E::InvalidInput)?;
    output.put(value.get(..end).ok_or(E::InvalidInput)?, budget)?;
    output.byte(
        value.get(end).ok_or(E::InvalidInput)? & (u8::MAX << (8 - last)),
        budget,
    )
}
