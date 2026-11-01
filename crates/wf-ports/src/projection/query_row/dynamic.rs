//! Canonical original dynamic metadata with fixed sorting spines.
use super::{
    I,
    budget::{Budget, add},
    model::OriginalQueryRecordError as E,
    primitive,
    writer::Writer,
};
use wf_domain::metadata::{valid_original_attributes, valid_original_state_meta};

type DynamicRows<'r, 'a, const N: usize> = [Option<(&'a [u8], I<'r, 'a>)>; N];

pub(super) fn attributes(value: I<'_, '_>, out: &mut Writer, budget: &mut Budget) -> Result<(), E> {
    preflight(value, false, budget)?;
    if !valid_original_attributes(value) {
        return Err(E::InvalidRecord);
    }
    let rows = sorted::<16>(value, budget)?;
    let count = rows.iter().flatten().count();
    let start = out.length;
    out.byte(u8::try_from(count).map_err(|_| E::Limit)?, budget)?;
    let semantic = entries(&rows, true, out, budget)?;
    if semantic > 2048 || out.length - start > 4096 {
        return Err(E::InvalidRecord);
    }
    Ok(())
}
pub(super) fn states(value: I<'_, '_>, out: &mut Writer, budget: &mut Budget) -> Result<(), E> {
    preflight(value, true, budget)?;
    if !valid_original_state_meta(value) {
        return Err(E::InvalidRecord);
    }
    let rows = sorted::<64>(value, budget)?;
    let start = out.length;
    let mut semantic = 0;
    out.byte(
        u8::try_from(rows.iter().flatten().count()).map_err(|_| E::Limit)?,
        budget,
    )?;
    for (name, values) in rows.into_iter().flatten() {
        out.byte(u8::try_from(name.len()).map_err(|_| E::Limit)?, budget)?;
        out.put(name, budget)?;
        let entries = sorted::<16>(values, budget)?;
        out.byte(
            u8::try_from(entries.iter().flatten().count()).map_err(|_| E::Limit)?,
            budget,
        )?;
        semantic = add(
            semantic,
            add(name.len(), self::entries(&entries, false, out, budget)?)?,
        )?;
        if semantic > 16384 {
            return Err(E::InvalidRecord);
        }
    }
    if out.length - start > 20480 {
        return Err(E::InvalidRecord);
    }
    Ok(())
}
fn entries<const N: usize>(
    rows: &[Option<(&[u8], I<'_, '_>)>; N],
    lists: bool,
    out: &mut Writer,
    budget: &mut Budget,
) -> Result<usize, E> {
    let mut bytes = 0;
    for &(name, value) in rows.iter().flatten() {
        if name.starts_with(b"__") {
            return Err(E::InvalidRecord);
        }
        out.byte(u8::try_from(name.len()).map_err(|_| E::Limit)?, budget)?;
        out.put(name, budget)?;
        let semantic = primitive::emit(value, 256, lists, out, budget)?;
        bytes = add(bytes, add(name.len(), semantic)?)?;
        if bytes > 16384 {
            return Err(E::InvalidRecord);
        }
    }
    Ok(bytes)
}
fn sorted<'r, 'a, const N: usize>(
    value: I<'r, 'a>,
    budget: &mut Budget,
) -> Result<DynamicRows<'r, 'a, N>, E> {
    let I::Map(rows) = value else {
        return Err(E::InvalidRecord);
    };
    if rows.len() > N {
        return Err(E::InvalidRecord);
    }
    let mut result = [None; N];
    for (slot, &(key, value)) in result.iter_mut().zip(rows) {
        let I::Binary(name) = key else {
            return Err(E::InvalidRecord);
        };
        if name.is_empty() || name.len() > 64 {
            return Err(E::InvalidRecord);
        }
        budget.spend(add(name.len(), 1)?)?;
        *slot = Some((name, value));
    }
    budget.spend(rows.len().checked_mul(N * 64).ok_or(E::Limit)?)?;
    result
        .get_mut(..rows.len())
        .ok_or(E::Limit)?
        .sort_unstable_by_key(|row| row.map(|(name, _)| name));
    Ok(result)
}
fn preflight(value: I<'_, '_>, states: bool, budget: &mut Budget) -> Result<(), E> {
    let I::Map(rows) = value else {
        return Err(E::InvalidRecord);
    };
    if rows.len() > if states { 64 } else { 16 } {
        return Err(E::InvalidRecord);
    }
    for &(key, value) in rows {
        name(key, budget)?;
        if states {
            let I::Map(values) = value else {
                return Err(E::InvalidRecord);
            };
            if values.len() > 16 {
                return Err(E::InvalidRecord);
            }
            for &(key, value) in values {
                name(key, budget)?;
                scalar(value, false, budget)?;
            }
        } else {
            scalar(value, true, budget)?;
        }
    }
    Ok(())
}
fn name(value: I<'_, '_>, budget: &mut Budget) -> Result<(), E> {
    let I::Binary(bytes) = value else {
        return Err(E::InvalidRecord);
    };
    if bytes.is_empty() || bytes.len() > 64 {
        return Err(E::InvalidRecord);
    }
    budget.spend(add(bytes.len().checked_mul(3).ok_or(E::Limit)?, 1)?)
}
fn scalar(value: I<'_, '_>, lists: bool, budget: &mut Budget) -> Result<(), E> {
    budget.spend(1)?;
    match value {
        I::Binary(bytes) => {
            if bytes.len() > 256 {
                return Err(E::InvalidRecord);
            }
            budget.spend(bytes.len())
        }
        I::BigInteger { little_endian, .. } => {
            budget.spend(little_endian.len().checked_mul(2).ok_or(E::Limit)?)
        }
        I::List(values) if lists && !values.is_empty() && values.len() <= 16 => {
            for value in values {
                let I::Binary(bytes) = value else {
                    return Err(E::InvalidRecord);
                };
                if bytes.len() > 256 {
                    return Err(E::InvalidRecord);
                }
                budget.spend(bytes.len().checked_mul(16).ok_or(E::Limit)?)?;
            }
            Ok(())
        }
        I::Nil
        | I::Boolean(_)
        | I::Atom("nil" | "true" | "false")
        | I::Integer(_)
        | I::Float(_) => Ok(()),
        _ => Err(E::InvalidRecord),
    }
}
