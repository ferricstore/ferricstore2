//! Original query primitive encoding, distinct from ETF and covering codecs.
use super::{
    I,
    budget::{Budget, add},
    model::OriginalQueryRecordError as E,
    writer::Writer,
};

pub(super) fn nil(value: I<'_, '_>) -> bool {
    matches!(value, I::Nil | I::Atom("nil"))
}
pub(super) fn integer(value: I<'_, '_>, budget: &mut Budget) -> Result<Option<i128>, E> {
    budget.spend(1)?;
    match value {
        I::Integer(value) => Ok(Some(value)),
        I::BigInteger {
            negative,
            little_endian,
        } => {
            budget.spend(add(little_endian.len(), 1)?)?;
            let length = little_endian
                .iter()
                .rposition(|b| *b != 0)
                .map_or(0, |i| i + 1);
            if length > 8 {
                return Ok(None);
            }
            let mut raw = [0; 8];
            raw.get_mut(..length)
                .ok_or(E::Limit)?
                .copy_from_slice(little_endian.get(..length).ok_or(E::Limit)?);
            let magnitude = i128::from(u64::from_le_bytes(raw));
            Ok(Some(if negative { -magnitude } else { magnitude }))
        }
        _ => Ok(None),
    }
}
pub(super) fn uint(value: I<'_, '_>, budget: &mut Budget) -> Result<u64, E> {
    integer(value, budget)?
        .and_then(|v| u64::try_from(v).ok())
        .ok_or(E::InvalidRecord)
}
pub(super) fn emit(
    value: I<'_, '_>,
    maximum: usize,
    list: bool,
    out: &mut Writer,
    budget: &mut Budget,
) -> Result<usize, E> {
    budget.spend(1)?;
    match value {
        I::Nil | I::Atom("nil") => {
            out.byte(0, budget)?;
            Ok(0)
        }
        I::Boolean(false) | I::Atom("false") => {
            out.byte(1, budget)?;
            Ok(1)
        }
        I::Boolean(true) | I::Atom("true") => {
            out.byte(2, budget)?;
            Ok(1)
        }
        I::Integer(_) | I::BigInteger { .. } => {
            let number = integer(value, budget)?
                .and_then(|v| i64::try_from(v).ok())
                .ok_or(E::InvalidRecord)?;
            let zigzag = (number.unsigned_abs() << 1).wrapping_sub(u64::from(number < 0));
            out.byte(3, budget)?;
            out.varint(zigzag, budget)?;
            Ok(8)
        }
        I::Float(value) if value.is_finite() => {
            out.byte(4, budget)?;
            out.put(&value.to_bits().to_be_bytes(), budget)?;
            Ok(8)
        }
        I::Binary(bytes) if bytes.len() <= maximum => {
            out.byte(5, budget)?;
            out.varint(u64::try_from(bytes.len()).map_err(|_| E::Limit)?, budget)?;
            out.put(bytes, budget)?;
            Ok(bytes.len())
        }
        I::List(values) if list && !values.is_empty() && values.len() <= 16 => {
            out.byte(6, budget)?;
            out.byte(u8::try_from(values.len()).map_err(|_| E::Limit)?, budget)?;
            let mut semantic = 0;
            for value in values {
                let I::Binary(bytes) = value else {
                    return Err(E::InvalidRecord);
                };
                if bytes.len() > maximum {
                    return Err(E::InvalidRecord);
                }
                out.varint(u64::try_from(bytes.len()).map_err(|_| E::Limit)?, budget)?;
                out.put(bytes, budget)?;
                semantic = add(semantic, bytes.len())?;
            }
            Ok(semantic)
        }
        _ => Err(E::InvalidRecord),
    }
}
