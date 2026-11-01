//! Checked borrowed primitive parsing, with no skipped-list allocation.
use super::super::{
    budget::{Budget, add},
    model::OriginalQueryRecordError as E,
};
use wf_domain::metadata::{OriginalAttributeValue as A, OriginalMetadataScalar as S};

pub(super) struct Cursor<'a> {
    pub(super) rest: &'a [u8],
}
pub(super) struct ParsedValue<'a> {
    pub(super) scalar: Option<S<'a>>,
    list: [&'a [u8]; 16],
    count: usize,
    pub(super) semantic: usize,
}
impl<'a> ParsedValue<'a> {
    pub(super) fn materialize(self, b: &mut Budget) -> Result<A<'a>, E> {
        if let Some(scalar) = self.scalar {
            return Ok(A::Scalar(scalar));
        }
        let mut values = b.reserve(self.count)?;
        values.extend_from_slice(self.list.get(..self.count).ok_or(E::Limit)?);
        Ok(A::List(values))
    }
}
impl<'a> Cursor<'a> {
    pub(super) const fn new(bytes: &'a [u8]) -> Self {
        Self { rest: bytes }
    }
    pub(super) fn take(&mut self, n: usize, b: &mut Budget) -> Result<&'a [u8], E> {
        b.spend(add(n, 1)?)?;
        let bytes = self.rest.get(..n).ok_or(E::InvalidRecord)?;
        self.rest = self.rest.get(n..).ok_or(E::InvalidRecord)?;
        Ok(bytes)
    }
    pub(super) fn byte(&mut self, b: &mut Budget) -> Result<u8, E> {
        self.take(1, b)?.first().copied().ok_or(E::InvalidRecord)
    }
    pub(super) fn u16(&mut self, b: &mut Budget) -> Result<usize, E> {
        Ok(usize::from(u16::from_be_bytes(
            self.take(2, b)?.try_into().map_err(|_| E::InvalidRecord)?,
        )))
    }
    pub(super) fn varint(&mut self, b: &mut Budget) -> Result<u64, E> {
        let mut result = 0;
        for group in 0..10 {
            let byte = self.byte(b)?;
            let payload = byte & 127;
            let more = byte & 128 != 0;
            if (group == 9 && (more || payload != 1)) || (!more && group > 0 && payload == 0) {
                return Err(E::InvalidRecord);
            }
            result |= u64::from(payload) << (group * 7);
            if !more {
                return Ok(result);
            }
        }
        Err(E::InvalidRecord)
    }
    pub(super) fn finish(&self) -> Result<(), E> {
        if self.rest.is_empty() {
            Ok(())
        } else {
            Err(E::InvalidRecord)
        }
    }
    pub(super) fn value(
        &mut self,
        maximum: usize,
        lists: bool,
        b: &mut Budget,
    ) -> Result<ParsedValue<'a>, E> {
        let tag = self.byte(b)?;
        let (scalar, semantic) = match tag {
            0 => (Some(S::Nil), 0),
            1 => (Some(S::Boolean(false)), 1),
            2 => (Some(S::Boolean(true)), 1),
            3 => {
                let n = self.varint(b)?;
                let value = if n & 1 == 0 {
                    i128::from(n >> 1)
                } else {
                    -i128::from(n >> 1) - 1
                };
                (
                    Some(S::Integer(
                        i64::try_from(value).map_err(|_| E::InvalidRecord)?,
                    )),
                    8,
                )
            }
            4 => {
                let value = f64::from_bits(u64::from_be_bytes(
                    self.take(8, b)?.try_into().map_err(|_| E::InvalidRecord)?,
                ));
                if !value.is_finite() {
                    return Err(E::InvalidRecord);
                }
                (Some(S::Float(value)), 8)
            }
            5 => {
                let bytes = self.binary(maximum, b)?;
                (Some(S::Binary(bytes)), bytes.len())
            }
            6 if lists => return self.list(maximum, b),
            _ => return Err(E::InvalidRecord),
        };
        Ok(ParsedValue {
            scalar,
            list: [&[]; 16],
            count: 0,
            semantic,
        })
    }
    fn binary(&mut self, maximum: usize, b: &mut Budget) -> Result<&'a [u8], E> {
        let n = usize::try_from(self.varint(b)?).map_err(|_| E::InvalidRecord)?;
        if n > maximum {
            return Err(E::InvalidRecord);
        }
        self.take(n, b)
    }
    fn list(&mut self, maximum: usize, b: &mut Budget) -> Result<ParsedValue<'a>, E> {
        let count = usize::from(self.byte(b)?);
        if count == 0 || count > 16 {
            return Err(E::InvalidRecord);
        }
        let mut value = ParsedValue {
            scalar: None,
            list: [&[]; 16],
            count,
            semantic: 0,
        };
        for index in 0..count {
            let binary = self.binary(maximum, b)?;
            b.spend(binary.len().checked_mul(16).ok_or(E::Limit)?)?;
            if value.list.get(..index).ok_or(E::Limit)?.contains(&binary) {
                return Err(E::InvalidRecord);
            }
            *value.list.get_mut(index).ok_or(E::Limit)? = binary;
            value.semantic = add(value.semantic, binary.len())?;
        }
        Ok(value)
    }
}
pub(super) fn input(scalar: S<'_>) -> super::super::I<'_, '_> {
    use super::super::I;
    match scalar {
        S::Nil => I::Nil,
        S::Boolean(v) => I::Boolean(v),
        S::Integer(v) => I::Integer(i128::from(v)),
        S::Float(v) => I::Float(v),
        S::Binary(v) => I::Binary(v),
    }
}
