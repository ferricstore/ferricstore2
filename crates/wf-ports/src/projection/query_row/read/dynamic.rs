//! Sorted dynamic decoding; skipped values validate without result allocation.
use super::super::{
    budget::{Budget, add},
    model::OriginalQueryRecordError as E,
};
use super::{
    cursor::Cursor,
    model::{Entries, OriginalQuerySelection as A, OriginalQueryStateSelection as S, States},
    selection,
};
pub(super) fn attributes<'a>(
    bytes: &'a [u8],
    selected: A<'_>,
    b: &mut Budget,
) -> Result<Entries<'a>, E> {
    let mut c = Cursor::new(bytes);
    let count = usize::from(c.byte(b)?);
    if count > 16 {
        return Err(E::InvalidRecord);
    }
    let (rows, semantic) = entries(&mut c, count, true, selected, b)?;
    c.finish()?;
    if semantic > 2048 {
        return Err(E::InvalidRecord);
    }
    Ok(rows)
}
pub(super) fn states<'a>(
    bytes: &'a [u8],
    selected: S<'_>,
    b: &mut Budget,
) -> Result<States<'a>, E> {
    let mut c = Cursor::new(bytes);
    let count = usize::from(c.byte(b)?);
    if count > 64 {
        return Err(E::InvalidRecord);
    }
    let mut previous = None;
    let mut result = Vec::new();
    let mut semantic = 0;
    for _ in 0..count {
        let name = read_name(&mut c, true, previous, b)?;
        previous = Some(name);
        let count = usize::from(c.byte(b)?);
        if count > 16 {
            return Err(E::InvalidRecord);
        }
        let chosen = selection::state(selected, name, b)?;
        let (values, bytes) = entries(&mut c, count, false, chosen, b)?;
        semantic = add(semantic, add(name.len(), bytes)?)?;
        if semantic > 16384 {
            return Err(E::InvalidRecord);
        }
        if matches!(selected, S::All) || !values.is_empty() {
            b.grow(&mut result)?;
            result.push((name, values));
        }
    }
    c.finish()?;
    Ok(result)
}
fn entries<'a>(
    c: &mut Cursor<'a>,
    count: usize,
    lists: bool,
    selected: A<'_>,
    b: &mut Budget,
) -> Result<(Entries<'a>, usize), E> {
    let mut result = Vec::new();
    let mut previous = None;
    let mut semantic = 0;
    for _ in 0..count {
        let name = read_name(c, false, previous, b)?;
        previous = Some(name);
        let value = c.value(256, lists, b)?;
        semantic = add(semantic, add(name.len(), value.semantic)?)?;
        if semantic > 16384 {
            return Err(E::InvalidRecord);
        }
        if selection::attribute(selected, name, b)? {
            b.grow(&mut result)?;
            result.push((name, value.materialize(b)?));
        }
    }
    Ok((result, semantic))
}
pub(super) fn read_name<'a>(
    c: &mut Cursor<'a>,
    reserved: bool,
    previous: Option<&[u8]>,
    b: &mut Budget,
) -> Result<&'a [u8], E> {
    let length = usize::from(c.byte(b)?);
    let name = c.take(length, b)?;
    selection::name(name, reserved, 64, b)?;
    b.spend(64)?;
    if previous.is_some_and(|p| name <= p) {
        return Err(E::InvalidRecord);
    }
    Ok(name)
}
