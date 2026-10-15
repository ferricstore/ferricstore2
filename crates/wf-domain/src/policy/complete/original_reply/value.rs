//! Cached scalar views and transactionally advanced, freshly charged map cursors.

use super::model::{
    OriginalPolicyReply as Reply, OriginalPolicyReplyMapEntry as MapEntry,
    OriginalPolicyReplyMapIter as MapIter, OriginalPolicyReplyValue as Value,
    PolicyReplyMap as Map, PolicyReplyRef as Ref,
};
use super::{maps, observe};
use crate::policy::complete::flow_input::value::PolicyInputKind as Kind;
use crate::policy::complete::original_entry::{
    TermPosition as Position, indexed::OriginalPolicyCurrentCursor as Cursor,
};
use crate::policy::complete::{CompletePolicyEncodingError as Error, PolicySchemaCharge};

impl<'view, 'input> Value<'view, 'input> {
    /// Borrows an atom spelling, preserving the distinction from a binary.
    #[must_use]
    pub fn atom_bytes(self) -> Option<&'view [u8]> {
        match self.reference {
            Ref::StaticAtom(name) => Some(name.as_bytes()),
            _ => self.cursor()?.atom_bytes(),
        }
    }

    /// Borrows original, caller or bounded inline bytes without copying them.
    #[must_use]
    pub fn binary_bytes(self) -> Option<&'view [u8]> {
        match self.reference {
            Ref::Bytes(bytes) => Some(bytes),
            Ref::Inline(slot) => {
                let &(start, length) = self.owner.coordinates.get(usize::from(slot))?;
                self.owner.inline.get(start..start.checked_add(length)?)
            }
            _ => self.cursor()?.binary_bytes(),
        }
    }

    /// Reads a mathematical integer when it fits the signed return type.
    #[must_use]
    pub fn integer(self) -> Option<i128> {
        match self.reference {
            Ref::Unsigned(number) => Some(i128::from(number)),
            _ => self.cursor()?.integer(),
        }
    }

    /// Borrows a retained big integer's sign and little-endian magnitude.
    #[must_use]
    pub fn big_integer(self) -> Option<(bool, &'view [u8])> {
        self.cursor()?.big_integer()
    }

    /// Reads the finite float already validated by the original reader.
    #[must_use]
    pub fn float(self) -> Option<f64> {
        self.cursor()?.float()
    }

    /// Borrows bit storage and its significant bit count, including binaries.
    /// Unused trailing storage bits are outside that count.
    #[must_use]
    pub fn bitstring(self) -> Option<(&'view [u8], usize)> {
        if let Some(cursor) = self.cursor() {
            return cursor.bitstring();
        }
        let bytes = self.binary_bytes()?;
        Some((bytes, bytes.len().checked_mul(8)?))
    }

    /// Borrows only an actual contiguous original encoded term span.
    /// Synthetic lists, inline names and residual windows have no such span.
    #[must_use]
    pub fn original_encoding(self) -> Option<&'input [u8]> {
        self.cursor()?.original_encoding()
    }

    /// Reads a cached original tuple arity.
    #[must_use]
    pub fn tuple_len(self) -> Option<usize> {
        let cursor = self.cursor()?;
        (cursor.kind() == Kind::Tuple).then_some(cursor.len())
    }

    /// Borrows an original tuple child through its retained constant-time index.
    #[must_use]
    pub fn tuple_item(self, index: usize) -> Option<Self> {
        let cursor = self.cursor()?;
        if cursor.kind() != Kind::Tuple {
            return None;
        }
        Some(self.with(Ref::Source(cursor.child(index)?.position())))
    }

    /// Reads one proper or improper cons, preserving its actual residual tail.
    #[must_use]
    pub fn cons(self) -> Option<(Self, Self)> {
        let pair = match self.reference {
            Ref::Attributes(offset) if usize::from(offset) < self.owner.attributes => {
                (Ref::Inline(offset), Ref::Attributes(offset.checked_add(1)?))
            }
            Ref::Source(position) => source_cons(self.owner, position, 0)?,
            Ref::ListWindow { source, offset } => {
                source_cons(self.owner, source, usize::try_from(offset).ok()?)?
            }
            _ => return None,
        };
        Some((self.with(pair.0), self.with(pair.1)))
    }

    /// Distinguishes the empty list from the nil atom and improper terminals.
    #[must_use]
    pub fn is_empty_list(self) -> bool {
        match self.reference {
            Ref::Attributes(offset) => usize::from(offset) >= self.owner.attributes,
            _ => self
                .cursor()
                .is_some_and(|cursor| cursor.kind() == Kind::List && cursor.len() == 0),
        }
    }

    /// Reads the cached mathematical size of a source or resolved map.
    #[must_use]
    pub fn map_len(self) -> Option<usize> {
        maps::len(self.owner, self.map()?)
    }

    /// Creates an unpaid cursor; every next call freshly admits resident owners.
    #[must_use]
    pub fn map_iter(self) -> Option<MapIter<'view, 'input>> {
        Some(MapIter {
            owner: self.owner,
            map: self.map()?,
            phase: 0,
            index: 0,
        })
    }

    /// Looks up exactly an atom field, with no binary alias fallback.
    ///
    /// # Errors
    /// Preserves profile, prospective work and retained-index failures.
    pub fn atom_field(
        self,
        name: &str,
        charge: &mut PolicySchemaCharge,
    ) -> Result<Option<Self>, Error> {
        observe(self.owner, charge)?;
        let Some(map) = self.map() else {
            return Ok(None);
        };
        Ok(maps::field(self.owner, map, name.as_bytes(), charge)?
            .map(|reference| self.with(reference)))
    }

    fn cursor(self) -> Option<Cursor<'view, 'input, 'input>> {
        let Ref::Source(position) = self.reference else {
            return None;
        };
        Some(self.owner.index.as_ref()?.at(position))
    }

    fn with(self, reference: Ref<'input>) -> Self {
        Self {
            owner: self.owner,
            reference,
        }
    }

    fn map(self) -> Option<Map> {
        match self.reference {
            Ref::Map(map) => Some(map),
            Ref::Source(_) => {
                let cursor = self.cursor()?;
                (cursor.kind() == Kind::Map).then_some(Map::Source(cursor.position()))
            }
            _ => None,
        }
    }
}

impl<'view, 'input> MapIter<'view, 'input> {
    /// Advances one mathematical row, retaining progress only on success.
    ///
    /// # Errors
    /// Preserves profile/work errors without advancing past a refused candidate.
    /// The account retains all work admitted before an error.
    pub fn next(
        &mut self,
        charge: &mut PolicySchemaCharge,
    ) -> Result<Option<MapEntry<'view, 'input>>, Error> {
        observe(self.owner, charge)?;
        let (mut phase, mut index) = (self.phase, self.index);
        let row = maps::next(self.owner, self.map, &mut phase, &mut index, charge)?;
        (self.phase, self.index) = (phase, index);
        Ok(row.map(|(key, value)| {
            (
                Value {
                    owner: self.owner,
                    reference: key,
                },
                Value {
                    owner: self.owner,
                    reference: value,
                },
            )
        }))
    }
}

fn source_cons<'input>(
    owner: &Reply<'input>,
    position: Position,
    offset: usize,
) -> Option<(Ref<'input>, Ref<'input>)> {
    let cursor = owner.index.as_ref()?.at(position);
    if !matches!(cursor.kind(), Kind::List | Kind::ImproperList) || offset >= cursor.len() {
        return None;
    }
    let head = Ref::Source(cursor.child(offset)?.position());
    let next = offset.checked_add(1)?;
    let tail = if next < cursor.len() {
        Ref::ListWindow {
            source: cursor.position(),
            offset: u32::try_from(next).ok()?,
        }
    } else {
        Ref::Source(cursor.tail().map_or(Position::Nil, Cursor::position))
    };
    Some((head, tail))
}
