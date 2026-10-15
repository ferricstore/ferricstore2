//! Constant-time views of validated canonical nodes and captured row permutations.

use crate::policy::complete::flow_input::value::PolicyInputKind;

#[derive(Clone, Copy, Debug)]
pub(in crate::policy::complete) struct FlowPolicyFrameCursor<'frame> {
    pub(super) node: &'frame [u8],
    pub(super) intent: &'frame [u8],
    pub(super) sidecar: &'frame [u8],
}

impl<'frame> FlowPolicyFrameCursor<'frame> {
    pub(in crate::policy::complete) fn kind(self) -> PolicyInputKind {
        use PolicyInputKind as K;
        match self.node.get(4) {
            Some(0) => K::Nil,
            Some(1) => K::Integer,
            Some(2) => K::Atom,
            Some(3) => K::Binary,
            Some(4) => K::Map,
            Some(5) => K::List,
            Some(6) => K::Tuple,
            Some(7) => K::ImproperList,
            _ => K::Other,
        }
    }

    pub(in crate::policy::complete) fn atom_bytes(self) -> Option<&'frame [u8]> {
        match self.node.get(4) {
            Some(0) => Some(b"nil"),
            Some(2) => self.node.get(9..),
            _ => None,
        }
    }

    pub(in crate::policy::complete) fn binary_bytes(self) -> Option<&'frame [u8]> {
        (self.node.get(4) == Some(&3))
            .then(|| self.node.get(9..))
            .flatten()
    }

    pub(in crate::policy::complete) fn big_integer(self) -> Option<(bool, &'frame [u8])> {
        (self.node.get(4) == Some(&1))
            .then(|| Some((*self.node.get(5)? == 1, self.node.get(10..)?)))
            .flatten()
    }

    pub(in crate::policy::complete) fn integer(self) -> Option<i128> {
        let (negative, bytes) = self.big_integer()?;
        if bytes.len() > 16 {
            return None;
        }
        let mut magnitude = [0; 16];
        magnitude[..bytes.len()].copy_from_slice(bytes);
        let value = u128::from_le_bytes(magnitude);
        if negative && value == (1_u128 << 127) {
            return Some(i128::MIN);
        }
        let value = i128::try_from(value).ok()?;
        Some(if negative { -value } else { value })
    }

    pub(in crate::policy::complete) fn len(self) -> usize {
        let offset = if self.node.get(4) == Some(&4) { 9 } else { 5 };
        match self.node.get(4) {
            Some(4..=7) => word(self.node, offset).unwrap_or(0),
            _ => 0,
        }
    }

    pub(in crate::policy::complete) fn child(self, index: usize) -> Option<Self> {
        if !matches!(self.node.get(4), Some(5..=7)) || index >= self.len() {
            return None;
        }
        self.at(9, index)
    }

    pub(in crate::policy::complete) fn tail(self) -> Option<Self> {
        (self.node.get(4) == Some(&7))
            .then(|| self.at(9, self.len()))
            .flatten()
    }

    pub(in crate::policy::complete) fn canonical_row(self, index: usize) -> Option<(Self, Self)> {
        if self.node.get(4) != Some(&4) || index >= self.len() {
            return None;
        }
        Some((
            self.at(13, index.checked_mul(2)?)?,
            self.at(13, index.checked_mul(2)?.checked_add(1)?)?,
        ))
    }

    pub(in crate::policy::complete) fn row(self, source: usize) -> Option<(Self, Self)> {
        if source >= self.len() {
            return None;
        }
        let id = word(self.node, 5)?;
        let record = word(self.sidecar, 4_usize.checked_add(id.checked_mul(4)?)?)?;
        let node_offset = word(self.sidecar, record)?;
        if self.intent.get(node_offset..)?.as_ptr() != self.node.as_ptr() {
            return None;
        }
        let rank = word(
            self.sidecar,
            record.checked_add(8)?.checked_add(source.checked_mul(4)?)?,
        )?;
        self.canonical_row(rank)
    }

    fn at(self, table: usize, index: usize) -> Option<Self> {
        let entry = table.checked_add(index.checked_mul(4)?)?;
        let start = word(self.node, entry)?;
        let end = word(self.node, entry.checked_add(4)?)?;
        Some(Self {
            node: self.node.get(start..end)?,
            ..self
        })
    }
}

pub(super) fn word(bytes: &[u8], offset: usize) -> Option<usize> {
    let end = offset.checked_add(4)?;
    usize::try_from(u32::from_be_bytes(bytes.get(offset..end)?.try_into().ok()?)).ok()
}
