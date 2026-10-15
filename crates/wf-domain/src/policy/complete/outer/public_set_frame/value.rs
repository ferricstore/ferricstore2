//! Constant-time views of validated canonical nodes and captured row permutations.

use crate::policy::complete::flow_input::value::PolicyInputKind;

#[derive(Clone, Copy, Debug)]
pub(in crate::policy::complete) struct PolicySetFrameCursor<'frame> {
    pub(super) node: &'frame [u8],
    pub(super) intent: &'frame [u8],
    pub(super) sidecar: &'frame [u8],
}

impl<'frame> PolicySetFrameCursor<'frame> {
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

use crate::policy::complete::flow_input::value::PolicyInputCursor;
use crate::policy::complete::{OriginalPolicySetPatchView, RawFlowPolicyValue};

#[derive(Clone, Copy)]
pub(super) enum PolicySetFrameValue<'input> {
    Prepared(OriginalPolicySetPatchView<'input>),
    Wire(PolicySetFrameCursor<'input>),
    Raw(RawFlowPolicyValue<'input>),
}

impl<'input> PolicySetFrameValue<'input> {
    fn raw(self) -> Option<PolicyInputCursor<'input>> {
        match self {
            Self::Prepared(value) => value.raw().map(PolicyInputCursor::Flow),
            Self::Raw(value) => Some(PolicyInputCursor::Flow(value)),
            Self::Wire(_) => None,
        }
    }
    pub(super) fn kind(self) -> PolicyInputKind {
        match self {
            Self::Wire(value) => value.kind(),
            Self::Prepared(value) if value.map_len().is_some() => PolicyInputKind::Map,
            Self::Prepared(value) if value.list_len().is_some() => PolicyInputKind::List,
            _ => self
                .raw()
                .map_or(PolicyInputKind::Other, PolicyInputCursor::kind),
        }
    }
    pub(super) fn atom_bytes(self) -> Option<&'input [u8]> {
        match self {
            Self::Wire(value) => value.atom_bytes(),
            _ => self.raw()?.atom_bytes(),
        }
    }
    pub(super) fn binary_bytes(self) -> Option<&'input [u8]> {
        match self {
            Self::Wire(value) => value.binary_bytes(),
            _ => self.raw()?.binary_bytes(),
        }
    }
    pub(super) fn integer(self) -> Option<i128> {
        match self {
            Self::Wire(value) => value.integer(),
            _ => self.raw()?.integer(),
        }
    }
    pub(super) fn big_integer(self) -> Option<(bool, &'input [u8])> {
        match self {
            Self::Wire(value) => value.big_integer(),
            _ => self.raw()?.big_integer(),
        }
    }
    pub(super) fn is_atom(self) -> bool {
        self.atom_bytes().is_some()
    }
    pub(super) fn is_nil(self) -> bool {
        self.atom_bytes() == Some(b"nil")
    }
    pub(super) fn len(self) -> usize {
        match self {
            Self::Wire(value) => value.len(),
            Self::Prepared(value) => value
                .map_len()
                .or_else(|| value.list_len())
                .unwrap_or_else(|| self.raw().map_or(0, PolicyInputCursor::len)),
            Self::Raw(_) => self.raw().map_or(0, PolicyInputCursor::len),
        }
    }
    pub(super) fn child(self, index: usize) -> Option<Self> {
        match self {
            Self::Wire(value) => value.child(index).map(Self::Wire),
            Self::Prepared(value) if value.list_len().is_some() => {
                value.list_item(index).map(Self::Prepared)
            }
            _ => match self.raw()?.child(index)? {
                PolicyInputCursor::Flow(value) => Some(Self::Raw(value)),
                _ => None,
            },
        }
    }
    pub(super) fn row(self, index: usize) -> Option<(Self, Self)> {
        match self {
            Self::Wire(value) => value
                .canonical_row(index)
                .map(|(k, v)| (Self::Wire(k), Self::Wire(v))),
            Self::Prepared(value) if value.map_len().is_some() => value
                .map_entry(index)
                .map(|(k, v)| (Self::Prepared(k), Self::Prepared(v))),
            _ => match self.raw()?.row(index)? {
                (PolicyInputCursor::Flow(k), PolicyInputCursor::Flow(v)) => {
                    Some((Self::Raw(k), Self::Raw(v)))
                }
                _ => None,
            },
        }
    }
    pub(super) fn tail(self) -> Option<Self> {
        match self {
            Self::Wire(value) => value.tail().map(Self::Wire),
            _ => match self.raw()?.tail()? {
                PolicyInputCursor::Flow(value) => Some(Self::Raw(value)),
                _ => None,
            },
        }
    }
}

use super::{Error, MAX_BYTES, MAX_NODES, PolicySchemaCharge, add, visit};
use crate::policy::complete::flow_input::value::PolicyInputKind as Kind;

pub(super) fn preflight(
    input: PolicySetFrameValue<'_>,
    depth: usize,
    original: (usize, usize),
    metrics: &mut (usize, usize, usize),
    charge: &mut PolicySchemaCharge,
) -> Result<(), Error> {
    visit(charge, 128)?;
    metrics.0 = add(metrics.0, 1)?;
    if metrics.0 > MAX_NODES.min(original.0) || depth > 64.min(original.1) {
        return Err(Error::ItemLimit);
    }
    metrics.1 = metrics.1.max(depth);
    match input.kind() {
        Kind::Nil => (),
        Kind::Integer => {
            if let Some((_, bytes)) = input.big_integer() {
                visible(bytes.len(), &mut metrics.2, charge)?;
            }
        }
        Kind::Atom => visible(
            input.atom_bytes().ok_or(Error::Malformed)?.len(),
            &mut metrics.2,
            charge,
        )?,
        Kind::Binary => visible(
            input.binary_bytes().ok_or(Error::Malformed)?.len(),
            &mut metrics.2,
            charge,
        )?,
        Kind::Map => {
            for index in 0..input.len() {
                let (k, v) = input.row(index).ok_or(Error::Malformed)?;
                preflight(k, depth + 1, original, metrics, charge)?;
                preflight(v, depth + 1, original, metrics, charge)?;
            }
        }
        Kind::List | Kind::Tuple | Kind::ImproperList => {
            for index in 0..input.len() {
                preflight(
                    input.child(index).ok_or(Error::Malformed)?,
                    depth + 1,
                    original,
                    metrics,
                    charge,
                )?;
            }
            if let Some(tail) = input.tail() {
                preflight(tail, depth + 1, original, metrics, charge)?;
            }
        }
        _ => return Err(Error::UnsupportedTag),
    }
    Ok(())
}
fn visible(length: usize, total: &mut usize, charge: &mut PolicySchemaCharge) -> Result<(), Error> {
    *total = add(*total, length)?;
    if *total > MAX_BYTES {
        return Err(Error::ByteLimit);
    }
    visit(charge, length)
}
