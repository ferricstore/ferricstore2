//! Original deterministic ETF for the closed normalized policy-cell subset.
//!
//! Pinned OTP28.3.1 external.c size/emit branches agree on this subset only.
//! No generation, stored-entry admission or payment authority is constructed.

use crate::policy::complete::{
    CompletePolicyEncodingError as Error, PolicySchemaCharge, PolicySchemaError, charge_schema,
};

use super::layout::{FlowPolicyCell, FlowPolicyLayout};

pub(super) fn measure_snapshot(
    layout: &FlowPolicyLayout<'_>,
    root: usize,
    charge: &mut PolicySchemaCharge,
) -> Result<usize, Error> {
    let mut output = None;
    let version = block(&[131], &[], false, &mut output, usize::MAX, charge)?;
    let content = visit(layout, root, charge, &mut output, usize::MAX)?;
    version.checked_add(content).ok_or_else(overflow)
}

pub(super) fn emit_snapshot(
    layout: &FlowPolicyLayout<'_>,
    root: usize,
    length: usize,
    charge: &mut PolicySchemaCharge,
) -> Result<Vec<u8>, Error> {
    let fee = u64::try_from(length)
        .ok()
        .and_then(|n| n.checked_add(128))
        .ok_or_else(overflow)?;
    charge_schema(charge, fee, 0).map_err(Error::Schema)?;
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(length)
        .map_err(|_| Error::AllocationFailure)?;
    let mut output = Some(&mut bytes);
    block(&[131], &[], false, &mut output, length, charge)?;
    visit(layout, root, charge, &mut output, length)?;
    if bytes.len() != length {
        return Err(Error::EncodingMismatch);
    }
    Ok(bytes)
}

fn visit(
    layout: &FlowPolicyLayout<'_>,
    index: usize,
    charge: &mut PolicySchemaCharge,
    output: &mut Option<&mut Vec<u8>>,
    limit: usize,
) -> Result<usize, Error> {
    match *layout.cells.get(index).ok_or(Error::Malformed)? {
        FlowPolicyCell::Atom(value) => {
            let length = u8::try_from(value.len()).map_err(|_| Error::Malformed)?;
            block(
                &[119, length],
                value.as_bytes(),
                true,
                output,
                limit,
                charge,
            )
        }
        FlowPolicyCell::Binary(value) => {
            let length = u32::try_from(value.len()).map_err(|_| overflow())?;
            let mut header = [109, 0, 0, 0, 0];
            header[1..].copy_from_slice(&length.to_be_bytes());
            block(&header, value, true, output, limit, charge)
        }
        FlowPolicyCell::Integer(value) => integer(value, output, limit, charge),
        FlowPolicyCell::BigInteger(magnitude) => big_integer(magnitude, output, limit, charge),
        FlowPolicyCell::List { start, len } => {
            container(layout, start, len, 108, charge, output, limit)
        }
        FlowPolicyCell::Map { start, len } => {
            container(layout, start, len, 116, charge, output, limit)
        }
    }
}

fn integer(
    value: u128,
    output: &mut Option<&mut Vec<u8>>,
    limit: usize,
    charge: &mut PolicySchemaCharge,
) -> Result<usize, Error> {
    if let Ok(value) = u8::try_from(value) {
        return block(&[97, value], &[], true, output, limit, charge);
    }
    if let Ok(value) = i32::try_from(value) {
        let mut header = [98, 0, 0, 0, 0];
        header[1..].copy_from_slice(&value.to_be_bytes());
        return block(&header, &[], true, output, limit, charge);
    }
    let bytes = value.to_le_bytes();
    let length = bytes
        .iter()
        .rposition(|&byte| byte != 0)
        .ok_or(Error::Malformed)?
        + 1;
    big_integer(&bytes[..length], output, limit, charge)
}

fn big_integer(
    magnitude: &[u8],
    output: &mut Option<&mut Vec<u8>>,
    limit: usize,
    charge: &mut PolicySchemaCharge,
) -> Result<usize, Error> {
    if let Ok(length) = u8::try_from(magnitude.len()) {
        return block(&[110, length, 0], magnitude, true, output, limit, charge);
    }
    let length = u32::try_from(magnitude.len()).map_err(|_| overflow())?;
    let mut header = [111, 0, 0, 0, 0, 0];
    header[1..5].copy_from_slice(&length.to_be_bytes());
    block(&header, magnitude, true, output, limit, charge)
}

fn container(
    layout: &FlowPolicyLayout<'_>,
    start: usize,
    len: usize,
    tag: u8,
    charge: &mut PolicySchemaCharge,
    output: &mut Option<&mut Vec<u8>>,
    limit: usize,
) -> Result<usize, Error> {
    if tag == 108 && len == 0 {
        return block(&[106], &[], true, output, limit, charge);
    }
    let count = u32::try_from(len).map_err(|_| overflow())?;
    let mut header = [tag, 0, 0, 0, 0];
    header[1..].copy_from_slice(&count.to_be_bytes());
    let mut length = block(&header, &[], true, output, limit, charge)?;
    let children = if tag == 116 {
        len.checked_mul(2).ok_or_else(overflow)?
    } else {
        len
    };
    let end = start.checked_add(children).ok_or_else(overflow)?;
    for &child in layout.edges.get(start..end).ok_or(Error::Malformed)? {
        length = length
            .checked_add(visit(layout, child, charge, output, limit)?)
            .ok_or_else(overflow)?;
    }
    if tag == 108 {
        length = length
            .checked_add(block(&[106], &[], false, output, limit, charge)?)
            .ok_or_else(overflow)?;
    }
    Ok(length)
}

fn block(
    header: &[u8],
    payload: &[u8],
    node: bool,
    output: &mut Option<&mut Vec<u8>>,
    limit: usize,
    charge: &mut PolicySchemaCharge,
) -> Result<usize, Error> {
    let length = header
        .len()
        .checked_add(payload.len())
        .ok_or_else(overflow)?;
    let fee = u64::try_from(length)
        .ok()
        .and_then(|n| n.checked_add(u64::from(node) * 128))
        .ok_or_else(overflow)?;
    charge_schema(charge, fee, 0).map_err(Error::Schema)?;
    if let Some(bytes) = output {
        let next = bytes.len().checked_add(length).ok_or_else(overflow)?;
        if next > limit {
            return Err(Error::EncodingMismatch);
        }
        bytes.extend_from_slice(header);
        bytes.extend_from_slice(payload);
    }
    Ok(length)
}

fn overflow() -> Error {
    Error::Schema(PolicySchemaError::LengthOverflow)
}
