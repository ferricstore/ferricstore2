//! Constant-time retained span/STRING children; payloads borrow original bytes.

use super::super::{TermPosition as Position, TermSpan};
use super::OriginalPolicyCurrentIndex;
use crate::policy::complete::flow_input::value::PolicyInputKind as Kind;

#[derive(Clone, Copy)]
pub(in crate::policy::complete) struct OriginalPolicyCurrentCursor<'index, 'raw, 'context> {
    pub(super) index: &'index OriginalPolicyCurrentIndex<'raw, 'context>,
    pub(super) position: Position,
}

impl std::fmt::Debug for OriginalPolicyCurrentCursor<'_, '_, '_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OriginalPolicyCurrentCursor")
            .finish_non_exhaustive()
    }
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
