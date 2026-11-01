//! Bounded flat descriptor views reuse the existing original dynamic validators.
use super::super::{
    I, OriginalQueryRecordError as E,
    budget::{Budget, add},
    writer::Writer,
};
use std::{mem::size_of, ops::Range};
use wf_domain::{
    metadata::record::{OriginalRecordMetadata as M, OriginalRecordMetadataField as F},
    value::{OriginalValueInput as Input, OriginalValueNode as N},
};

pub(super) fn scalar<'a>(input: Input<'a>, id: usize, b: &mut Budget) -> Result<I<'a, 'a>, E> {
    b.spend(1)?;
    Ok(match input.nodes.get(id).ok_or(E::InvalidRecord)? {
        N::Integer(n) => I::Integer(i128::from(*n)),
        N::BigInteger { negative, digits } => I::BigInteger {
            negative: *negative,
            little_endian: input.bytes.get(digits.clone()).ok_or(E::InvalidRecord)?,
        },
        N::FloatBits(bits) => I::Float(f64::from_bits(*bits)),
        N::Binary { bytes } => I::Binary(input.bytes.get(bytes.clone()).ok_or(E::InvalidRecord)?),
        N::Atom { bytes, .. } => match input.bytes.get(bytes.clone()).ok_or(E::InvalidRecord)? {
            b"nil" => I::Nil,
            b"true" => I::Boolean(true),
            b"false" => I::Boolean(false),
            _ => I::Other,
        },
        _ => I::Other,
    })
}
fn rows<'a>(input: Input<'a>, id: usize, maximum: usize, b: &mut Budget) -> Result<&'a [usize], E> {
    b.spend(1)?;
    let N::Map { entries } = input.nodes.get(id).ok_or(E::InvalidRecord)? else {
        return Err(E::InvalidRecord);
    };
    let rows = input.edges.get(entries.clone()).ok_or(E::InvalidRecord)?;
    if !rows.len().is_multiple_of(2) || rows.len() / 2 > maximum {
        return Err(E::InvalidRecord);
    }
    b.spend(rows.len())?;
    Ok(rows)
}
fn list<'a>(
    input: Input<'a>,
    mut id: usize,
    b: &mut Budget,
) -> Result<([I<'a, 'a>; 16], usize), E> {
    let mut values = [I::Nil; 16];
    let mut length = 0;
    loop {
        b.spend(1)?;
        match input.nodes.get(id).ok_or(E::InvalidRecord)? {
            N::EmptyList => return Ok((values, length)),
            N::List { heads, tail } => {
                for child in input.edges.get(heads.clone()).ok_or(E::InvalidRecord)? {
                    *values.get_mut(length).ok_or(E::InvalidRecord)? = scalar(input, *child, b)?;
                    length += 1;
                }
                id = *tail;
            }
            _ => return Err(E::InvalidRecord),
        }
    }
}
fn is_list(input: Input<'_>, id: usize) -> bool {
    matches!(input.nodes.get(id), Some(N::EmptyList | N::List { .. }))
}

pub(super) fn validate(m: &M, b: &mut Budget) -> Result<(), E> {
    let before = b.live();
    b.admit(
        size_of::<[(I<'_, '_>, I<'_, '_>); 64]>()
            + size_of::<[(I<'_, '_>, I<'_, '_>); 16]>()
            + size_of::<[I<'_, '_>; 16]>()
            + size_of::<[Range<usize>; 64]>()
            + size_of::<[Option<Range<usize>>; 16]>()
            + 4096,
    )?;
    if let Some(input) = m.field_input(F::Attributes) {
        attributes(input, b)?;
    }
    if let Some(input) = m.field_input(F::StateMeta) {
        states(input, b)?;
    }
    b.release_to(before);
    Ok(())
}
fn attributes(input: Input<'_>, b: &mut Budget) -> Result<(), E> {
    let source = rows(input, input.root, 16, b)?;
    let mut ranges: [Option<Range<usize>>; 16] = std::array::from_fn(|_| None);
    let mut count = 0;
    for (index, pair) in source.as_chunks::<2>().0.iter().enumerate() {
        if is_list(input, pair[1]) {
            let (_, length) = list(input, pair[1], b)?;
            ranges[index] = Some(count..add(count, length)?);
            count = add(count, length)?;
        }
    }
    let mut flat = b.reserve::<I<'_, '_>>(count)?;
    for pair in source.as_chunks::<2>().0 {
        if is_list(input, pair[1]) {
            let (values, length) = list(input, pair[1], b)?;
            flat.extend_from_slice(values.get(..length).ok_or(E::Limit)?);
        }
    }
    let mut target = [(I::Nil, I::Nil); 16];
    for (index, pair) in source.as_chunks::<2>().0.iter().enumerate() {
        let value = match &ranges[index] {
            Some(range) => I::List(flat.get(range.clone()).ok_or(E::Limit)?),
            None => scalar(input, pair[1], b)?,
        };
        target[index] = (scalar(input, pair[0], b)?, value);
    }
    super::super::dynamic::attributes(
        I::Map(target.get(..source.len() / 2).ok_or(E::Limit)?),
        &mut Writer::measure(),
        b,
    )
}
fn states(input: Input<'_>, b: &mut Budget) -> Result<(), E> {
    let source = rows(input, input.root, 64, b)?;
    let mut count = 0;
    for pair in source.as_chunks::<2>().0 {
        count = add(count, rows(input, pair[1], 16, b)?.len() / 2)?;
    }
    let mut flat = b.reserve::<(I<'_, '_>, I<'_, '_>)>(count)?;
    let mut ranges: [Range<usize>; 64] = std::array::from_fn(|_| 0..0);
    for (index, pair) in source.as_chunks::<2>().0.iter().enumerate() {
        let start = flat.len();
        for inner in rows(input, pair[1], 16, b)?.as_chunks::<2>().0 {
            flat.push((scalar(input, inner[0], b)?, scalar(input, inner[1], b)?));
        }
        ranges[index] = start..flat.len();
    }
    let mut target = [(I::Nil, I::Nil); 64];
    for (index, pair) in source.as_chunks::<2>().0.iter().enumerate() {
        target[index] = (
            scalar(input, pair[0], b)?,
            I::Map(flat.get(ranges[index].clone()).ok_or(E::Limit)?),
        );
    }
    super::super::dynamic::states(
        I::Map(target.get(..source.len() / 2).ok_or(E::Limit)?),
        &mut Writer::measure(),
        b,
    )
}
