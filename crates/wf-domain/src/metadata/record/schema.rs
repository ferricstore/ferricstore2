//! Closed envelope only; field values keep the complete shared data grammar.
use super::{OriginalRecordMetadataError as E, OriginalRecordMetadataField as F, budget::spend};
use crate::metadata::{OriginalMetadataInput, integer_i64};
use crate::value::{OriginalValueInput as Input, OriginalValueNode as Node};
use std::ops::Range;

pub(super) fn validate(input: Input<'_>, work: &mut usize) -> Result<[Option<usize>; 12], E> {
    let Node::Tuple { children } = node(input, input.root, work)? else {
        return Err(E::InvalidSchema);
    };
    let children = edges(input, children.clone(), work)?;
    let [tag, version, fields] = children else {
        return Err(E::InvalidSchema);
    };
    let Node::Binary { bytes: tag } = node(input, *tag, work)? else {
        return Err(E::InvalidSchema);
    };
    if bytes(input, tag.clone(), work)? != b"wf.record.metadata" {
        return Err(E::InvalidSchema);
    }
    version_one(input, *version, work)?;
    let Node::Map { entries } = node(input, *fields, work)? else {
        return Err(E::InvalidSchema);
    };
    let rows = edges(input, entries.clone(), work)?;
    if !rows.len().is_multiple_of(2) || rows.len() > 24 {
        return Err(E::InvalidSchema);
    }
    let mut fields = [None; 12];
    for row in rows.as_chunks::<2>().0 {
        let Node::Binary { bytes: key } = node(input, row[0], work)? else {
            return Err(E::InvalidSchema);
        };
        if key.len() > 32 {
            return Err(E::InvalidSchema);
        }
        let key = bytes(input, key.clone(), work)?;
        let field = F::from_bytes(key).ok_or(E::InvalidSchema)?;
        node(input, row[1], work)?;
        if fields[field.index()].replace(row[1]).is_some() {
            return Err(E::InvalidSchema);
        }
    }
    Ok(fields)
}

fn version_one(input: Input<'_>, index: usize, work: &mut usize) -> Result<(), E> {
    let value = match node(input, index, work)? {
        Node::Integer(value) => Some(*value),
        Node::BigInteger { negative, digits } => integer_i64(OriginalMetadataInput::BigInteger {
            negative: *negative,
            little_endian: bytes(input, digits.clone(), work)?,
        }),
        _ => return Err(E::InvalidSchema),
    };
    if value == Some(1) {
        Ok(())
    } else {
        Err(E::UnsupportedVersion)
    }
}
fn node<'a>(input: Input<'a>, index: usize, work: &mut usize) -> Result<&'a Node, E> {
    spend(work, 64)?;
    input.nodes.get(index).ok_or(E::InvalidSchema)
}
fn edges<'a>(input: Input<'a>, range: Range<usize>, work: &mut usize) -> Result<&'a [usize], E> {
    let values = input.edges.get(range).ok_or(E::InvalidSchema)?;
    spend(
        work,
        values
            .len()
            .checked_mul(std::mem::size_of::<usize>())
            .ok_or(E::Limit)?,
    )?;
    Ok(values)
}
fn bytes<'a>(input: Input<'a>, range: Range<usize>, work: &mut usize) -> Result<&'a [u8], E> {
    let values = input.bytes.get(range).ok_or(E::InvalidSchema)?;
    spend(work, values.len().checked_mul(16).ok_or(E::Limit)?)?;
    Ok(values)
}
