//! Owner-free exact grammar, canonical-child and linear sidecar verification.

use super::value::PolicySetFrameValue as Cursor;
use super::{
    CompletePolicyRequestError, Error, MAX_BYTES, MAX_FRAME, MAX_NODES, MAX_PATCH, PREFIX,
    PolicySchemaCharge, PolicySchemaLimits, PolicySetFrameCursor, ScannedOriginalPolicySetFrame,
    add, canonical, check_scope, value::word, visit,
};
use crate::policy::complete::encoding::reader::Reader;
use std::cmp::Ordering;
use wf_types::version::{CommittedTime, HlcTimestamp};

pub(super) fn frame<'frame>(
    encoded: &'frame [u8],
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<(ScannedOriginalPolicySetFrame<'frame>, &'frame [u8]), CompletePolicyRequestError> {
    let mut reader = Reader::new(encoded, limits, charge);
    reader.node()?;
    if reader.take(4)? != b"FWCP" {
        return Err(Error::Malformed.into());
    }
    if reader.u16()? != 1 {
        return Err(Error::UnsupportedVersion.into());
    }
    if reader.byte()? != 9 {
        return Err(Error::UnsupportedTag.into());
    }
    let tenant = reader.raw(8192)?;
    let group = reader.raw(256)?;
    let recovery = reader.raw(256)?;
    check_scope(tenant, group, recovery)?;
    let request = reader.raw(256)?;
    if request.is_empty() {
        return Err(Error::Malformed.into());
    }
    if reader.u16()? != 4 {
        return Err(Error::UnsupportedVersion.into());
    }
    let digest = reader.take(32)?.try_into().map_err(|_| Error::Malformed)?;
    let hlc = HlcTimestamp::new(reader.u64()?, reader.u16()?).map_err(|_| Error::Malformed)?;
    let stamp = CommittedTime::new(hlc, reader.u64()?).map_err(|_| Error::Malformed)?;
    let ilen = usize::try_from(reader.u32()?).map_err(|_| super::overflow())?;
    let slen = usize::try_from(reader.u32()?).map_err(|_| super::overflow())?;
    if ilen > MAX_FRAME || slen > MAX_FRAME {
        return Err(Error::ByteLimit.into());
    }
    let begin = reader.position();
    let side = add(begin, ilen)?;
    let end = add(side, slen)?;
    if end > encoded.len() {
        return Err(Error::Malformed.into());
    }
    let original_intent = encoded.get(begin..side).ok_or(Error::Malformed)?;
    let sidecar = encoded.get(side..end).ok_or(Error::Malformed)?;
    let maps = sidecar_shape(sidecar, limits, charge)?;
    let empty = PolicySetFrameCursor {
        node: &[],
        intent: original_intent,
        sidecar,
    };
    let mut scanned = ScannedOriginalPolicySetFrame {
        encoded,
        original_intent,
        tenant,
        group,
        recovery,
        request,
        stamp,
        digest,
        policy_key: &[],
        expected: None,
        patch: empty,
        replace: false,
        response_state: None,
        patch_bounds: (0, 0),
    };
    let inner_tenant = intent(&mut scanned, sidecar, maps, limits, charge)?;
    if end < encoded.len() {
        return Err(Error::TrailingBytes.into());
    }
    Ok((scanned, inner_tenant))
}

fn sidecar_shape(
    bytes: &[u8],
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<usize, Error> {
    let mut reader = Reader::new(bytes, limits, charge);
    reader.node()?;
    let maps = usize::try_from(reader.u32()?).map_err(|_| super::overflow())?;
    if maps > MAX_NODES {
        return Err(Error::Malformed);
    }
    let first = add(8, maps.checked_mul(4).ok_or_else(super::overflow)?)?;
    let mut previous = first;
    for index in 0..=maps {
        let offset = usize::try_from(reader.u32()?).map_err(|_| super::overflow())?;
        if (index == 0 && offset != first) || offset < previous || offset > bytes.len() {
            return Err(Error::Malformed);
        }
        previous = offset;
    }
    if previous != bytes.len() {
        return Err(Error::Malformed);
    }
    for id in 0..maps {
        let start = word(bytes, 4 + id * 4).ok_or(Error::Malformed)?;
        let end = word(bytes, 8 + id * 4).ok_or(Error::Malformed)?;
        let record = bytes.get(start..end).ok_or(Error::Malformed)?;
        visit(charge, 128)?;
        let mut position = 0;
        read(record, &mut position, 4, charge)?;
        let length = u32_at(record, &mut position, charge)?;
        if add(8, length.checked_mul(8).ok_or_else(super::overflow)?)? != record.len() {
            return Err(Error::Malformed);
        }
        for source in 0..length {
            let rank = u32_at(record, &mut position, charge)?;
            visit(charge, 16)?;
            if rank >= length {
                return Err(Error::Malformed);
            }
            visit(charge, 4)?;
            if word(record, 8 + length * 4 + rank * 4) != Some(source) {
                return Err(Error::Malformed);
            }
        }
    }
    Ok(maps)
}

fn intent<'frame>(
    scanned: &mut ScannedOriginalPolicySetFrame<'frame>,
    sidecar: &'frame [u8],
    maps: usize,
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<&'frame [u8], Error> {
    let bytes = scanned.original_intent;
    let mut reader = Reader::new(bytes, limits, charge);
    reader.node()?;
    if reader.take(PREFIX.len())? != PREFIX {
        return Err(Error::Malformed);
    }
    let tenant = reader.raw(8192)?;
    if tenant.is_empty() {
        return Err(Error::Malformed);
    }
    let key = reader.raw(65_535)?;
    let replace = match reader.byte()? {
        0 => false,
        1 => true,
        _ => return Err(Error::Malformed),
    };
    let response_state = match reader.byte()? {
        0 => None,
        1 => Some(reader.raw(MAX_BYTES)?),
        _ => return Err(Error::Malformed),
    };
    let expected = match reader.byte()? {
        0 => None,
        1 => Some(reader.u64()?)
            .filter(|&n| n <= 9_007_199_254_740_991)
            .ok_or(Error::Malformed)
            .map(Some)?,
        _ => return Err(Error::Malformed),
    };
    let start = reader.position();
    let mut metrics = (0, 0, 0, 0);
    let patch = node(
        PolicySetFrameCursor {
            node: bytes.get(start..).ok_or(Error::Malformed)?,
            intent: bytes,
            sidecar,
        },
        start,
        0,
        &mut metrics,
        limits,
        charge,
    )?;
    if add(start, patch.node.len())? != bytes.len() {
        return Err(Error::Malformed);
    }
    scanned.replace = replace;
    scanned.response_state = response_state;
    scanned.patch_bounds = (metrics.0, metrics.3);
    if metrics.1 != maps {
        return Err(Error::Malformed);
    }
    scanned.policy_key = key;
    scanned.expected = expected;
    scanned.patch = patch;
    Ok(tenant)
}

fn node<'frame>(
    mut input: PolicySetFrameCursor<'frame>,
    offset: usize,
    depth: usize,
    metrics: &mut (usize, usize, usize, usize),
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<PolicySetFrameCursor<'frame>, Error> {
    visit(charge, 128)?;
    metrics.0 = add(metrics.0, 1)?;
    metrics.3 = metrics.3.max(depth);
    if metrics.0 > MAX_NODES || depth > 64 {
        return Err(Error::ItemLimit);
    }
    let mut position = 0;
    let length = u32_at(input.node, &mut position, charge)?;
    input.node = input.node.get(..length).ok_or(Error::Malformed)?;
    let tag = read(input.node, &mut position, 1, charge)?[0];
    match tag {
        0 => (),
        1 => {
            let sign = read(input.node, &mut position, 1, charge)?[0];
            let count = u32_at(input.node, &mut position, charge)?;
            metrics.2 = add(metrics.2, count)?;
            if metrics.2 > MAX_PATCH {
                return Err(Error::ByteLimit);
            }
            let bytes = read(input.node, &mut position, count, charge)?;
            if sign > 1 || (count == 0 && sign != 0) || bytes.last() == Some(&0) {
                return Err(Error::Malformed);
            }
        }
        2 | 3 => {
            let count = u32_at(input.node, &mut position, charge)?;
            metrics.2 = add(metrics.2, count)?;
            if metrics.2 > MAX_PATCH {
                return Err(Error::ByteLimit);
            }
            let bytes = read(input.node, &mut position, count, charge)?;
            if tag == 2 && (bytes == b"nil" || std::str::from_utf8(bytes).is_err()) {
                return Err(Error::Malformed);
            }
        }
        4..=7 => {
            compound(
                input,
                (offset, depth, tag, position),
                metrics,
                limits,
                charge,
            )?;
            return Ok(input);
        }
        _ => return Err(Error::UnsupportedTag),
    }
    if position != input.node.len() {
        return Err(Error::Malformed);
    }
    Ok(input)
}

fn compound(
    input: PolicySetFrameCursor<'_>,
    fields: (usize, usize, u8, usize),
    metrics: &mut (usize, usize, usize, usize),
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<(), Error> {
    let (offset, depth, tag, mut position) = fields;
    if tag == 4 {
        let id = u32_at(input.node, &mut position, charge)?;
        if id != metrics.1 {
            return Err(Error::Malformed);
        }
        metrics.1 = add(metrics.1, 1)?;
        let record = word(input.sidecar, 4 + id * 4).ok_or(Error::Malformed)?;
        if word(input.sidecar, record) != Some(offset) {
            return Err(Error::Malformed);
        }
    }
    let count = u32_at(input.node, &mut position, charge)?;
    if tag == 4 {
        let id = word(input.node, 5).ok_or(Error::Malformed)?;
        let record = word(input.sidecar, 4 + id * 4).ok_or(Error::Malformed)?;
        if word(input.sidecar, record + 4) != Some(count) {
            return Err(Error::Malformed);
        }
    }
    if count > MAX_NODES || (tag == 7 && count == 0) {
        return Err(Error::Malformed);
    }
    let children = if tag == 4 {
        count.checked_mul(2).ok_or_else(super::overflow)?
    } else {
        add(count, usize::from(tag == 7))?
    };
    let table = position;
    let first = add(
        table,
        add(children, 1)?
            .checked_mul(4)
            .ok_or_else(super::overflow)?,
    )?;
    let mut previous = first;
    for index in 0..=children {
        let current = u32_at(input.node, &mut position, charge)?;
        if (index == 0 && current != first) || current < previous || current > input.node.len() {
            return Err(Error::Malformed);
        }
        previous = current;
    }
    if previous != input.node.len() {
        return Err(Error::Malformed);
    }
    children_walk(
        input,
        (offset, depth),
        metrics,
        (tag, count, children, table),
        limits,
        charge,
    )
}

fn children_walk(
    input: PolicySetFrameCursor<'_>,
    location: (usize, usize),
    metrics: &mut (usize, usize, usize, usize),
    fields: (u8, usize, usize, usize),
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<(), Error> {
    let (offset, depth) = location;
    let (tag, count, children, table) = fields;
    for index in 0..children {
        let begin = word(input.node, table + index * 4).ok_or(Error::Malformed)?;
        let end = word(input.node, table + (index + 1) * 4).ok_or(Error::Malformed)?;
        let child = PolicySetFrameCursor {
            node: input.node.get(begin..end).ok_or(Error::Malformed)?,
            ..input
        };
        let bounded = node(
            child,
            add(offset, begin)?,
            depth + 1,
            metrics,
            limits,
            charge,
        )?;
        if bounded.node.len() != child.node.len() {
            return Err(Error::Malformed);
        }
        if tag == 7 && index == count && matches!(child.node.get(4), Some(5 | 7)) {
            return Err(Error::Malformed);
        }
    }
    if tag == 4 {
        for index in 1..count {
            let left = input.canonical_row(index - 1).ok_or(Error::Malformed)?.0;
            let right = input.canonical_row(index).ok_or(Error::Malformed)?.0;
            if canonical::compare(
                Cursor::Wire(left),
                Cursor::Wire(right),
                &mut Vec::new(),
                charge,
            )? != Ordering::Less
            {
                return Err(Error::Malformed);
            }
        }
    }
    Ok(())
}

fn read<'frame>(
    bytes: &'frame [u8],
    position: &mut usize,
    length: usize,
    charge: &mut PolicySchemaCharge,
) -> Result<&'frame [u8], Error> {
    let end = add(*position, length)?;
    let value = bytes.get(*position..end).ok_or(Error::Malformed)?;
    visit(charge, length)?;
    *position = end;
    Ok(value)
}

fn u32_at(
    bytes: &[u8],
    position: &mut usize,
    charge: &mut PolicySchemaCharge,
) -> Result<usize, Error> {
    usize::try_from(u32::from_be_bytes(
        read(bytes, position, 4, charge)?
            .try_into()
            .map_err(|_| Error::Malformed)?,
    ))
    .map_err(|_| super::overflow())
}
