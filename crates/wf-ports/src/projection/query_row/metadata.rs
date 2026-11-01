//! Five original sections, measured before a single final write.
use super::{
    I, budget::Budget, config, dynamic, model::OriginalQueryRecordError as E, primitive,
    record::RecordPlan, writer::Writer,
};
const BUILTINS: [usize; 14] = [1, 2, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15];

pub(super) fn measure(plan: &RecordPlan<'_, '_>, budget: &mut Budget) -> Result<[usize; 5], E> {
    let mut sizes = [0; 5];
    for (section, size) in sizes.iter_mut().enumerate() {
        let mut out = Writer::measure();
        emit_section(section, plan, &mut out, budget)?;
        *size = out.length;
    }
    Ok(sizes)
}
pub(super) fn emit(
    plan: &RecordPlan<'_, '_>,
    out: &mut Writer,
    budget: &mut Budget,
) -> Result<(), E> {
    let start = out.length;
    out.byte(u8::from(plan.root_is_id), budget)?;
    for size in plan.sections {
        out.u16(size, budget)?;
    }
    for section in 0..5 {
        emit_section(section, plan, out, budget)?;
    }
    if out.length - start != plan.metadata_bytes {
        return Err(E::Limit);
    }
    Ok(())
}
fn emit_section(
    section: usize,
    plan: &RecordPlan<'_, '_>,
    out: &mut Writer,
    budget: &mut Budget,
) -> Result<(), E> {
    match section {
        0 => builtin(plan, out, budget),
        1 => internal(plan, out, budget),
        2 => config::emit(
            plan.fields.get(22).unwrap_or(I::List(&[])),
            plan.fields.get(23).unwrap_or(I::Nil),
            out,
            budget,
        ),
        3 => dynamic::attributes(plan.fields.get(20).unwrap_or(I::Map(&[])), out, budget),
        4 => dynamic::states(plan.fields.get(21).unwrap_or(I::Map(&[])), out, budget),
        _ => Err(E::Limit),
    }
}
fn value<'a>(plan: &RecordPlan<'_, 'a>, index: usize) -> Option<I<'a, 'a>> {
    // Scalar-only builtin views never borrow descriptor spines.
    if index == 5 {
        return Some(plan.partition.map_or(I::Nil, I::Binary));
    }
    match plan.fields.get(index)? {
        I::Nil => Some(I::Nil),
        I::Boolean(v) => Some(I::Boolean(v)),
        I::Integer(v) => Some(I::Integer(v)),
        I::BigInteger {
            negative,
            little_endian,
        } => Some(I::BigInteger {
            negative,
            little_endian,
        }),
        I::Atom(v) => Some(I::Atom(v)),
        I::Binary(v) => Some(I::Binary(v)),
        I::Float(v) => Some(I::Float(v)),
        _ => Some(I::Other),
    }
}
fn builtin(plan: &RecordPlan<'_, '_>, out: &mut Writer, budget: &mut Budget) -> Result<(), E> {
    let mut bitmap = 0_u16;
    for (bit, index) in BUILTINS.into_iter().enumerate() {
        if !(index == 14 && plan.root_is_id) && value(plan, index).is_some() {
            bitmap |= 1 << bit;
        }
    }
    out.put(&bitmap.to_be_bytes(), budget)?;
    for (bit, index) in BUILTINS.into_iter().enumerate() {
        if bitmap & (1 << bit) == 0 {
            continue;
        }
        let value = value(plan, index).ok_or(E::InvalidRecord)?;
        validate_builtin(index, value, budget)?;
        primitive::emit(value, 32768, false, out, budget)?;
    }
    Ok(())
}
fn internal(plan: &RecordPlan<'_, '_>, out: &mut Writer, budget: &mut Budget) -> Result<(), E> {
    let mut flags = 0;
    for i in 0..4 {
        if plan.fields.get(16 + i).is_some_and(|v| !primitive::nil(v)) {
            flags |= 1 << i;
        }
    }
    out.byte(flags, budget)?;
    for i in 0..4 {
        if flags & (1 << i) == 0 {
            continue;
        }
        let value = plan.fields.get(16 + i).ok_or(E::InvalidRecord)?;
        if i == 3 {
            if !matches!(value, I::Binary(_)) {
                return Err(E::InvalidRecord);
            }
            primitive::emit(value, 32768, false, out, budget)?;
        } else {
            out.varint(primitive::uint(value, budget)?, budget)?;
        }
    }
    Ok(())
}

fn validate_builtin(index: usize, value: I<'_, '_>, budget: &mut Budget) -> Result<(), E> {
    if primitive::nil(value) {
        return Ok(());
    }
    if matches!(index, 4 | 6..=10 | 12) {
        if primitive::integer(value, budget)?.is_none_or(|v| i64::try_from(v).is_err()) {
            return Err(E::InvalidRecord);
        }
        return Ok(());
    }
    if matches!(value, I::Binary(bytes) if bytes.len() <= 65536) {
        Ok(())
    } else {
        Err(E::InvalidRecord)
    }
}
