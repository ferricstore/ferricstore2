//! Exact source sealed-field checks and shared canonical ETF sizing.
use super::arena::Arena;
use super::budget::{Budget, add, encoding};
use super::{OriginalMetadataInput as I, OriginalScopeError as E};
use crate::value::{measure_original_external_term, prepare_original_external_term};

pub(super) fn run(metadata: I<'_, '_>, budget: &mut Budget) -> Result<Option<Vec<u8>>, E> {
    let before = budget.live();
    let result = derive(metadata, budget)?;
    budget.retain(before, result.as_ref().map_or(0, Vec::len))?;
    Ok(result)
}

fn derive(metadata: I<'_, '_>, budget: &mut Budget) -> Result<Option<Vec<u8>>, E> {
    let I::Map(entries) = metadata else {
        return Err(E::InvalidSystemMetadata);
    };
    if entries.len() > 16 {
        return Err(E::InvalidSystemMetadata);
    }
    budget.spend(1)?;
    if entries.is_empty() {
        return Ok(None);
    }
    let mut rows = [None; 16];
    let mut byte_count = 20;
    for (slot, &(id, sealed)) in rows.iter_mut().zip(entries) {
        let id = uint(id, budget)?
            .filter(|id| (1..=65535).contains(id))
            .ok_or(E::InvalidSystemMetadata)?;
        let I::Tuple(tuple) = sealed else {
            return Err(E::InvalidSystemMetadata);
        };
        let fields: [I<'_, '_>; 4] = tuple.try_into().map_err(|_| E::InvalidSystemMetadata)?;
        validate(fields, budget)?;
        for value in fields {
            byte_count = add(byte_count, scalar_bytes(value, budget)?)?;
        }
        *slot = Some((id, fields));
    }
    budget.spend(entries.len() * 16)?;
    rows.get_mut(..entries.len())
        .ok_or(E::Limit)?
        .sort_unstable_by_key(|row| row.map(|r| r.0));
    if rows
        .get(..entries.len())
        .ok_or(E::Limit)?
        .windows(2)
        .any(|pair| pair[0].map(|r| r.0) == pair[1].map(|r| r.0))
    {
        return Err(E::InvalidSystemMetadata);
    }
    let mut arena = Arena::new(byte_count, entries.len(), budget)?;
    let mut all = [0; 16];
    let mut selected = [0; 16];
    let mut count = 0;
    let mut compact = None;
    for (index, row) in rows.into_iter().take(entries.len()).enumerate() {
        let (id, fields) = row.ok_or(E::InvalidSystemMetadata)?;
        let key = arena.scalar(I::Integer(i128::from(id)), budget)?;
        let mut children = [0; 4];
        for (child, value) in children.iter_mut().zip(fields) {
            *child = arena.scalar(value, budget)?;
        }
        let sealed = arena.tuple(&children)?;
        let entry = arena.tuple(&[key, sealed])?;
        *all.get_mut(index).ok_or(E::Limit)? = entry;
        if matches!(fields[2], I::Atom("isolation_scope")) {
            *selected.get_mut(count).ok_or(E::Limit)? = entry;
            count += 1;
            compact = if matches!(fields[1], I::Atom("uint64")) {
                uint(fields[3], budget)?
            } else {
                None
            };
        }
    }
    arena.finish(all.get(..entries.len()).ok_or(E::Limit)?, budget)?;
    let (size, work, temporary) =
        measure_original_external_term(arena.input(), budget.codec_limits()?).map_err(encoding)?;
    budget.codec_used(work, temporary)?;
    if size > 8192 {
        return Err(E::InvalidSystemMetadata);
    }
    if count == 0 {
        return Ok(None);
    }
    if let (1, Some(value)) = (count, compact) {
        budget.output(8)?;
        let mut bytes = budget.reserve(8)?;
        bytes.extend_from_slice(&value.to_be_bytes());
        return Ok(Some(bytes));
    }
    arena.select_scope(selected.get(..count).ok_or(E::Limit)?, budget)?;
    let mut limits = budget.codec_limits()?;
    limits.max_encoded_bytes = budget.limits.maximum_output_bytes;
    let encoded = prepare_original_external_term(arena.input(), limits).map_err(encoding)?;
    budget.codec_used(encoded.work_units(), encoded.temporary_bytes())?;
    Ok(Some(encoded.into_external_term()))
}

fn validate(fields: [I<'_, '_>; 4], budget: &mut Budget) -> Result<(), E> {
    budget.spend(4)?;
    let version = match fields[0] {
        I::Integer(value) => value > 0,
        I::BigInteger {
            negative,
            little_endian,
        } => {
            let bytes = trimmed(little_endian, budget)?;
            !negative && !bytes.is_empty() && bytes.len() <= 8192
        }
        _ => false,
    };
    let role = matches!(fields[2], I::Atom("isolation_scope" | "system_metadata"));
    if !version || !role {
        return Err(E::InvalidSystemMetadata);
    }
    let valid = match fields[1] {
        I::Atom("uint64") => uint(fields[3], budget)?.is_some(),
        I::Atom("int64" | "datetime") => signed(fields[3], budget)?,
        I::Atom("keyword") => match fields[3] {
            I::Binary(bytes) if !bytes.is_empty() && bytes.len() <= 4096 => {
                budget.spend(bytes.len())?;
                std::str::from_utf8(bytes).is_ok()
            }
            _ => false,
        },
        I::Atom("boolean") => matches!(fields[3], I::Boolean(_) | I::Atom("true" | "false")),
        _ => false,
    };
    if valid {
        Ok(())
    } else {
        Err(E::InvalidSystemMetadata)
    }
}

fn trimmed<'a>(bytes: &'a [u8], budget: &mut Budget) -> Result<&'a [u8], E> {
    budget.spend(add(bytes.len(), 1)?)?;
    let length = bytes.iter().rposition(|b| *b != 0).map_or(0, |i| i + 1);
    bytes.get(..length).ok_or(E::Limit)
}

fn uint(value: I<'_, '_>, budget: &mut Budget) -> Result<Option<u64>, E> {
    match value {
        I::Integer(value) => Ok(u64::try_from(value).ok()),
        I::BigInteger {
            negative,
            little_endian,
        } => {
            let bytes = trimmed(little_endian, budget)?;
            if bytes.len() > 8 || (negative && !bytes.is_empty()) {
                return Ok(None);
            }
            let mut raw = [0; 8];
            raw.get_mut(..bytes.len())
                .ok_or(E::Limit)?
                .copy_from_slice(bytes);
            Ok(Some(u64::from_le_bytes(raw)))
        }
        _ => Ok(None),
    }
}

fn signed(value: I<'_, '_>, budget: &mut Budget) -> Result<bool, E> {
    match value {
        I::Integer(value) => Ok(i64::try_from(value).is_ok()),
        I::BigInteger {
            negative,
            little_endian,
        } => {
            let magnitude = uint(
                I::BigInteger {
                    negative: false,
                    little_endian,
                },
                budget,
            )?;
            Ok(magnitude.is_some_and(|v| {
                if negative {
                    v <= 1_u64 << 63
                } else {
                    v <= i64::MAX.unsigned_abs()
                }
            }))
        }
        _ => Ok(false),
    }
}

fn scalar_bytes(value: I<'_, '_>, budget: &mut Budget) -> Result<usize, E> {
    match value {
        I::Integer(_) => Ok(16),
        I::BigInteger { little_endian, .. } => Ok(trimmed(little_endian, budget)?.len()),
        I::Atom(value) => Ok(value.len()),
        I::Boolean(_) => Ok(5),
        I::Binary(value) => Ok(value.len()),
        _ => Err(E::InvalidSystemMetadata),
    }
}
