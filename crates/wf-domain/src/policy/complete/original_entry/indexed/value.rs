//! Constant-time retained span/STRING children; payloads borrow original bytes.

use super::super::{TermPosition as Position, TermSpan};
use super::OriginalPolicyCurrentIndex;
use crate::policy::complete::flow_input::value::PolicyInputKind as Kind;

impl<'raw, 'context> OriginalPolicyCurrentIndex<'raw, 'context> {
    pub(in crate::policy::complete) fn raw_policy_position(&self) -> u32 {
        super::super::compound::resolve(&self.spans, self.policy)
    }
    pub(in crate::policy::complete) fn raw_cursor(
        &self,
        id: u32,
    ) -> OriginalPolicyCurrentCursor<'_, 'raw, 'context> {
        self.at(Position::Encoded(super::super::compound::resolve(
            &self.spans,
            id,
        )))
    }
    pub(in crate::policy::complete) fn raw_span(
        &self,
        id: u32,
    ) -> Result<&'raw [u8], super::Error> {
        let span = self.spans.get(id as usize).ok_or(super::Error::Malformed)?;
        self.raw
            .get(span.start as usize..span.end as usize)
            .ok_or(super::Error::Malformed)
    }
    pub(in crate::policy::complete) fn raw_row_positions(
        &self,
        parent: u32,
        next: u32,
    ) -> Result<Option<(u32, u32, u32)>, super::Error> {
        let map = self
            .spans
            .get(parent as usize)
            .ok_or(super::Error::Malformed)?;
        if map.tag() != 116 || next <= parent {
            return Err(super::Error::Malformed);
        }
        if next == map.next {
            return Ok(None);
        }
        if next > map.next {
            return Err(super::Error::Malformed);
        }
        let key = self
            .spans
            .get(next as usize)
            .ok_or(super::Error::Malformed)?;
        let value = self
            .spans
            .get(key.next as usize)
            .ok_or(super::Error::Malformed)?;
        if value.next > map.next {
            return Err(super::Error::Malformed);
        }
        Ok(Some((next, key.next, value.next)))
    }
    pub(in crate::policy::complete) fn raw_keyword_pair(
        &self,
        cursor: &mut (u32, u32),
        charge: &mut super::PolicySchemaCharge,
    ) -> Result<Option<(u32, u32, u32)>, super::Error> {
        loop {
            let span = *self
                .spans
                .get(cursor.0 as usize)
                .ok_or(super::Error::Malformed)?;
            if span.tag() == 106 || (span.tag() == 107 && span.datum() == 0) {
                return Ok(None);
            }
            if span.tag() != 108
                || span.datum() == 0
                || cursor.1 <= cursor.0
                || cursor.1 > span.keys
            {
                return Err(super::Error::Malformed);
            }
            if cursor.1 == span.keys {
                cursor.0 = self.raw_keyword_start(span.keys, charge)?;
                cursor.1 = cursor.0.checked_add(1).ok_or_else(super::overflow)?;
                continue;
            }
            super::pay(charge, 128)?;
            let id = cursor.1;
            let tuple = *self.spans.get(id as usize).ok_or(super::Error::Malformed)?;
            if !matches!(tuple.tag(), 104 | 105) || tuple.keys != 2 || tuple.next > span.keys {
                return Err(super::Error::Schema(
                    super::super::PolicySchemaError::InvalidAuthority,
                ));
            }
            let key = id.checked_add(1).ok_or_else(super::overflow)?;
            let value = self
                .spans
                .get(key as usize)
                .ok_or(super::Error::Malformed)?
                .next;
            if self
                .spans
                .get(value as usize)
                .is_none_or(|span| span.next != tuple.next)
                || self.raw_cursor(key).atom_bytes().is_none()
            {
                return Err(super::Error::Schema(
                    super::super::PolicySchemaError::InvalidAuthority,
                ));
            }
            cursor.1 = tuple.next;
            return Ok(Some((id, key, value)));
        }
    }

    pub(in crate::policy::complete) fn raw_depth(
        &self,
        id: u32,
        charge: &mut super::PolicySchemaCharge,
    ) -> Result<usize, super::Error> {
        let span = self.spans.get(id as usize).ok_or(super::Error::Malformed)?;
        let count = match span.tag() {
            104 | 105 => span.keys as usize,
            116 => (span.datum() as usize)
                .checked_mul(2)
                .ok_or_else(super::overflow)?,
            108 => (span.datum() as usize)
                .checked_add(1)
                .ok_or_else(super::overflow)?,
            _ => return Ok(0),
        };
        let (mut child, mut depth) = (id + 1, 0);
        for _ in 0..count {
            super::pay(charge, 128)?;
            depth = depth.max(
                self.raw_depth(child, charge)?
                    .checked_add(1)
                    .ok_or_else(super::overflow)?,
            );
            child = self
                .spans
                .get(child as usize)
                .ok_or(super::Error::Malformed)?
                .next;
        }
        Ok(depth)
    }
}

#[derive(Clone, Copy)]
pub(in crate::policy::complete) struct OriginalPolicyCurrentCursor<'index, 'raw, 'context> {
    pub(super) index: &'index OriginalPolicyCurrentIndex<'raw, 'context>,
    pub(super) position: Position,
}

impl<'index, 'raw, 'context> OriginalPolicyCurrentCursor<'index, 'raw, 'context> {
    pub(in crate::policy::complete) const fn position(self) -> Position {
        self.position
    }

    pub(in crate::policy::complete) fn original_encoding(self) -> Option<&'raw [u8]> {
        let Position::Encoded(_) = self.position else {
            return None;
        };
        let span = self.span()?;
        self.index.raw.get(span.start as usize..span.end as usize)
    }

    pub(in crate::policy::complete) fn float(self) -> Option<f64> {
        let span = self.span()?;
        matches!(span.tag(), 70 | 99)
            .then(|| f64::from_bits((u64::from(span.canonical) << 32) | u64::from(span.keys)))
    }

    pub(in crate::policy::complete) fn bitstring(self) -> Option<(&'raw [u8], usize)> {
        let span = self.span()?;
        if !matches!(span.tag(), 109 | 77) {
            return None;
        }
        let bits = usize::try_from(span.keys).ok()?;
        let start = usize::try_from(span.canonical).ok()?;
        let bytes = self
            .index
            .raw
            .get(start..start.checked_add(bits.div_ceil(8))?)?;
        Some((bytes, bits))
    }

    pub(in crate::policy::complete) fn equal(
        self,
        other: Self,
        charge: &mut super::PolicySchemaCharge,
    ) -> Result<Option<bool>, super::Error> {
        // Only one retained current enters an arena. Its descendant cursors
        // preserve that index; Frame/overlay references use cross-source logic.
        Ok(Some(
            super::super::identity::compare(
                (
                    self.index.raw,
                    &self.index.spans,
                    &self.index.keys,
                    &self.index.safety,
                    &self.index.virtual_funs,
                ),
                self.position,
                other.position,
                charge,
            )? == std::cmp::Ordering::Equal,
        ))
    }

    pub(super) fn new(
        index: &'index OriginalPolicyCurrentIndex<'raw, 'context>,
        position: Position,
    ) -> Self {
        let position = match position {
            Position::Encoded(id) => Position::Encoded(
                index
                    .resolve(id as usize)
                    .and_then(|n| u32::try_from(n).ok())
                    .unwrap_or(u32::MAX),
            ),
            other => other,
        };
        Self { index, position }
    }

    fn span(self) -> Option<TermSpan> {
        match self.position {
            Position::Encoded(id) | Position::StringCell { span: id, .. } => {
                self.index.spans.get(id as usize).copied()
            }
            _ => None,
        }
    }

    pub(in crate::policy::complete) fn map_id(self) -> Option<u32> {
        match self.position {
            Position::Encoded(id) if self.kind() == Kind::Map => Some(id),
            _ => None,
        }
    }

    pub(in crate::policy::complete) fn kind(self) -> Kind {
        if let Position::Byte(_) = self.position {
            return Kind::Integer;
        }
        if let Position::Nil = self.position {
            return Kind::List;
        }
        match self.span().map(TermSpan::tag) {
            Some(100 | 115 | 118 | 119) => Kind::Atom,
            Some(97 | 98 | 110 | 111) => Kind::Integer,
            Some(109) => Kind::Binary,
            Some(77) if self.span().is_some_and(|s| s.keys % 8 == 0) => Kind::Binary,
            Some(104 | 105) => Kind::Tuple,
            Some(116) => Kind::Map,
            Some(106 | 107) => Kind::List,
            Some(108) => Kind::ImproperList,
            _ => Kind::Other,
        }
    }

    pub(in crate::policy::complete) fn atom_bytes(self) -> Option<&'context [u8]> {
        let span = self.span()?;
        if !matches!(span.tag(), 100 | 115 | 118 | 119) {
            return None;
        }
        self.index
            .safety
            .atoms
            .get(span.datum() as usize)
            .map(|s| s.as_bytes())
    }

    pub(in crate::policy::complete) fn binary_bytes(self) -> Option<&'raw [u8]> {
        if self.kind() != Kind::Binary {
            return None;
        }
        let span = self.span()?;
        let start = usize::try_from(span.canonical).ok()?;
        self.index
            .raw
            .get(start..start.checked_add(usize::try_from(span.keys / 8).ok()?)?)
    }

    pub(in crate::policy::complete) fn integer(self) -> Option<i128> {
        if let Position::Byte(byte) = self.position {
            return Some(i128::from(byte));
        }
        let span = self.span()?;
        match span.tag() {
            97 => Some(i128::from(span.keys)),
            98 => Some(i128::from(i32::from_be_bytes(span.keys.to_be_bytes()))),
            110 | 111 => {
                let (negative, bytes) = self.big_integer()?;
                if bytes.len() > 16 {
                    return None;
                }
                let mut magnitude = [0; 16];
                magnitude[..bytes.len()].copy_from_slice(bytes);
                let number = u128::from_le_bytes(magnitude);
                if negative && number == (1_u128 << 127) {
                    Some(i128::MIN)
                } else {
                    let number = i128::try_from(number).ok()?;
                    if negative {
                        number.checked_neg()
                    } else {
                        Some(number)
                    }
                }
            }
            _ => None,
        }
    }

    pub(in crate::policy::complete) fn big_integer(self) -> Option<(bool, &'raw [u8])> {
        let span = self.span()?;
        if !matches!(span.tag(), 110 | 111) {
            return None;
        }
        let start = span.canonical as usize;
        Some((
            span.datum() != 0,
            self.index
                .raw
                .get(start..start.checked_add(span.keys as usize)?)?,
        ))
    }

    pub(in crate::policy::complete) fn len(self) -> usize {
        let Some(span) = self.span() else {
            return 0;
        };
        match span.tag() {
            104 | 105 => span.keys as usize,
            116 | 108 => span.datum() as usize,
            107 => {
                let offset = match self.position {
                    Position::StringCell { offset, .. } => offset,
                    _ => 0,
                };
                span.datum().saturating_sub(offset) as usize
            }
            _ => 0,
        }
    }

    pub(in crate::policy::complete) fn child(self, ordinal: usize) -> Option<Self> {
        if ordinal >= self.len() {
            return None;
        }
        let span = self.span()?;
        if span.tag() == 107 {
            let offset = match self.position {
                Position::StringCell { offset, .. } => offset as usize,
                _ => 0,
            };
            let byte = *self.index.raw.get(
                (span.canonical as usize)
                    .checked_add(offset)?
                    .checked_add(ordinal)?,
            )?;
            return Some(Self::new(self.index, Position::Byte(byte)));
        }
        let Position::Encoded(id) = self.position else {
            return None;
        };
        let start = *self.index.catalog.get(id as usize)? as usize;
        let child = *self.index.catalog.get(start.checked_add(ordinal)?)?;
        Some(Self::new(self.index, Position::Encoded(child)))
    }

    pub(in crate::policy::complete) fn tail(self) -> Option<Self> {
        let span = self.span()?;
        (span.tag() == 108).then(|| Self::new(self.index, Position::Encoded(span.keys)))
    }

    pub(in crate::policy::complete) fn row(self, ordinal: usize) -> Option<(Self, Self)> {
        if self.kind() != Kind::Map || ordinal >= self.len() {
            return None;
        }
        let span = self.span()?;
        let key = *self
            .index
            .keys
            .get((span.keys as usize).checked_add(ordinal)?)?;
        let value = self.index.spans.get(key as usize)?.next;
        Some((
            Self::new(self.index, Position::Encoded(key)),
            Self::new(self.index, Position::Encoded(value)),
        ))
    }
}
