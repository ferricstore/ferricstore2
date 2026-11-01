//! All five original metadata sections, with full-only internal/config fields.
use super::super::{I, budget::Budget, model::OriginalQueryRecordError as E};
use super::{
    cursor::{Cursor, input},
    dynamic,
    frame::Frame,
    model::{
        Configuration, OriginalQueryDecodePlan as Plan, OriginalQueryRecord,
        OriginalQuerySelection as A, OriginalQueryStateSelection as S,
    },
    selection,
};
const BUILTINS: [usize; 14] = [1, 2, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15];
pub(super) fn parse<'a>(
    frame: &Frame<'a>,
    plan: Option<Plan<'_>>,
    b: &mut Budget,
) -> Result<(OriginalQueryRecord<'a>, [&'a [u8]; 5]), E> {
    let mut c = Cursor::new(frame.metadata);
    let flags = c.byte(b)?;
    if flags > 1 {
        return Err(E::InvalidRecord);
    }
    let mut sizes = [0; 5];
    for size in &mut sizes {
        *size = c.u16(b)?;
    }
    if sizes[2] > 512 {
        return Err(E::InvalidRecord);
    }
    let mut sections = [&[][..]; 5];
    for (slot, size) in sections.iter_mut().zip(sizes) {
        *slot = c.take(size, b)?;
    }
    c.finish()?;
    let mut fields = builtin(sections[0], frame, b)?;
    let internals = internal(sections[1], b)?;
    let (indexed_attributes, indexed_state) = configuration(sections[2], plan.is_none(), b)?;
    let selected = plan.unwrap_or(Plan {
        attributes: A::All,
        state_meta: S::All,
    });
    let attributes = dynamic::attributes(sections[3], selected.attributes, b)?;
    let states = dynamic::states(sections[4], selected.state_meta, b)?;
    if flags == 1 {
        fields[14] = Some(I::Binary(frame.id));
    }
    if plan.is_none() {
        fields[16..20].copy_from_slice(&internals);
    }
    let partition = match fields[5] {
        Some(I::Nil) => None,
        Some(I::Binary(bytes)) => Some(bytes),
        _ => return Err(E::InvalidRecord),
    };
    super::super::record::bind(frame.key, frame.id, partition, frame.scope, b)?;
    Ok((
        OriginalQueryRecord {
            fields,
            attributes,
            states,
            attributes_nested: matches!(selected.attributes, A::All),
            states_nested: matches!(selected.state_meta, S::All),
            indexed_attributes,
            indexed_state,
        },
        sections,
    ))
}
fn builtin<'a>(
    bytes: &'a [u8],
    frame: &Frame<'a>,
    b: &mut Budget,
) -> Result<[Option<I<'a, 'a>>; 20], E> {
    if frame.version > 9_007_199_254_740_991 {
        return Err(E::InvalidRecord);
    }
    let mut c = Cursor::new(bytes);
    let bitmap = c.u16(b)?;
    if bitmap & !0x3fff != 0 {
        return Err(E::InvalidRecord);
    }
    let mut fields = [None; 20];
    fields[0] = Some(I::Binary(frame.id));
    fields[3] = Some(I::Integer(i128::from(frame.version)));
    for (bit, index) in BUILTINS.into_iter().enumerate() {
        if bitmap & (1 << bit) == 0 {
            continue;
        }
        let scalar = c.value(32768, false, b)?.scalar.ok_or(E::InvalidRecord)?;
        let value = input(scalar);
        super::super::metadata::validate_builtin(index, value, b)?;
        fields[index] = Some(value);
    }
    c.finish()?;
    match fields[5] {
        Some(I::Nil) => {}
        Some(I::Binary(bytes)) if !bytes.is_empty() => {}
        _ => return Err(E::InvalidRecord),
    }
    Ok(fields)
}
fn internal<'a>(bytes: &'a [u8], b: &mut Budget) -> Result<[Option<I<'a, 'a>>; 4], E> {
    let mut c = Cursor::new(bytes);
    let flags = c.byte(b)?;
    if flags & !15 != 0 {
        return Err(E::InvalidRecord);
    }
    let mut fields = [None; 4];
    for (bit, slot) in fields.iter_mut().enumerate() {
        if flags & (1 << bit) == 0 {
            continue;
        }
        let value = if bit == 3 {
            let scalar = c.value(32768, false, b)?.scalar.ok_or(E::InvalidRecord)?;
            let value = input(scalar);
            if !matches!(value, I::Binary(_)) {
                return Err(E::InvalidRecord);
            }
            value
        } else {
            I::Integer(i128::from(c.varint(b)?))
        };
        *slot = Some(value);
    }
    c.finish()?;
    Ok(fields)
}
fn configuration<'a>(
    bytes: &'a [u8],
    materialize: bool,
    b: &mut Budget,
) -> Result<Configuration<'a>, E> {
    let mut c = Cursor::new(bytes);
    let count = usize::from(c.byte(b)?);
    if count > 3 {
        return Err(E::InvalidRecord);
    }
    let mut names = if materialize {
        b.reserve(count)?
    } else {
        Vec::new()
    };
    let mut previous = None;
    for _ in 0..count {
        let name = dynamic::read_name(&mut c, false, previous, b)?;
        previous = Some(name);
        if materialize {
            names.push(name);
        }
    }
    let length = usize::from(c.byte(b)?);
    let state = c.take(length, b)?;
    c.finish()?;
    if !state.is_empty() {
        selection::name(state, true, 64, b)?;
    }
    Ok((names, (materialize && !state.is_empty()).then_some(state)))
}
