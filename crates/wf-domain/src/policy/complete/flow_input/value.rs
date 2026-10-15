//! Actual Flow/Retry/Indexed borrowed adapters and freshly paid selectors.

use super::{
    Error, PolicySchemaCharge, PolicySchemaError, RawFlowPolicyValue as Raw, charge_schema,
};
use crate::policy::complete::RawPolicyRetryValue as Retry;
use crate::policy::complete::outer::flow_frame::FlowPolicyFrameCursor;
use crate::policy::complete::outer::public_set_frame::PolicySetFrameCursor;
use crate::policy::complete::public_set::merge::PolicyMergeCursor;
use crate::policy::complete::{OriginalPolicyMapKey as Key, RawPolicyIndexedValue as Indexed};

#[derive(Clone, Copy, Debug)]
pub(in crate::policy::complete) enum PolicyInputCursor<'input> {
    Flow(Raw<'input>),
    Retry(Retry<'input>),
    Indexed(Indexed<'input>),
    Frame(FlowPolicyFrameCursor<'input>),
    Frame4(PolicySetFrameCursor<'input>),
    Merge(PolicyMergeCursor<'input>),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::policy::complete) enum PolicyInputKind {
    Nil,
    Integer,
    Atom,
    Binary,
    Map,
    Keyword,
    List,
    Tuple,
    ImproperList,
    Other,
}

#[derive(Clone, Copy, Debug)]
pub(in crate::policy::complete) enum PolicyInputMap<'input> {
    Direct(PolicyInputCursor<'input>),
    Keyword(PolicyInputCursor<'input>),
    StatePairs(PolicyInputCursor<'input>),
    Empty,
}

impl<'input> PolicyInputCursor<'input> {
    pub(in crate::policy::complete) fn kind(self) -> PolicyInputKind {
        use PolicyInputKind as K;
        match self {
            Self::Frame(value) => value.kind(),
            Self::Frame4(value) => value.kind(),
            Self::Merge(value) => value.kind(),
            Self::Flow(value) => match value {
                Raw::Nil => K::Nil,
                Raw::Integer(_) | Raw::BigInteger { .. } => K::Integer,
                Raw::Atom(_) => K::Atom,
                Raw::Binary(_) => K::Binary,
                Raw::Map(_) => K::Map,
                Raw::List(_) => K::List,
                Raw::Tuple(_) => K::Tuple,
                Raw::ImproperList { .. } => K::ImproperList,
                Raw::Other => K::Other,
            },
            Self::Retry(value) => match value {
                Retry::Nil => K::Nil,
                Retry::Integer(_) => K::Integer,
                Retry::Atom(_) => K::Atom,
                Retry::Binary(_) => K::Binary,
                Retry::Map(_) => K::Map,
                Retry::Keyword(_) => K::Keyword,
                Retry::Other => K::Other,
            },
            Self::Indexed(value) => match value {
                Indexed::Nil => K::Nil,
                Indexed::Atom(_) => K::Atom,
                Indexed::Binary(_) => K::Binary,
                Indexed::List(_) => K::List,
                Indexed::Other => K::Other,
            },
        }
    }
    pub(in crate::policy::complete) fn is_nil(self) -> bool {
        self.kind() == PolicyInputKind::Nil || self.atom_bytes() == Some(b"nil")
    }
    pub(in crate::policy::complete) fn is_atom(self) -> bool {
        self.atom_bytes().is_some()
    }
    pub(in crate::policy::complete) fn atom_bytes(self) -> Option<&'input [u8]> {
        match self {
            Self::Frame(value) => value.atom_bytes(),
            Self::Frame4(value) => value.atom_bytes(),
            Self::Merge(value) => value.atom_bytes(),
            Self::Flow(Raw::Nil) | Self::Retry(Retry::Nil) | Self::Indexed(Indexed::Nil) => {
                Some(b"nil")
            }
            Self::Flow(Raw::Atom(s))
            | Self::Retry(Retry::Atom(s))
            | Self::Indexed(Indexed::Atom(s)) => Some(s.as_bytes()),
            _ => None,
        }
    }
    pub(in crate::policy::complete) fn binary_bytes(self) -> Option<&'input [u8]> {
        match self {
            Self::Frame(value) => value.binary_bytes(),
            Self::Frame4(value) => value.binary_bytes(),
            Self::Merge(value) => value.binary_bytes(),
            Self::Flow(Raw::Binary(b))
            | Self::Retry(Retry::Binary(b))
            | Self::Indexed(Indexed::Binary(b)) => Some(b),
            _ => None,
        }
    }
    pub(in crate::policy::complete) fn key_bytes(self) -> Option<&'input [u8]> {
        self.atom_bytes().or_else(|| self.binary_bytes())
    }
    pub(in crate::policy::complete) fn integer(self) -> Option<i128> {
        match self {
            Self::Frame(value) => value.integer(),
            Self::Frame4(value) => value.integer(),
            Self::Merge(value) => value.integer(),
            Self::Flow(Raw::Integer(n)) | Self::Retry(Retry::Integer(n)) => Some(n),
            _ => None,
        }
    }
    pub(in crate::policy::complete) fn big_integer(self) -> Option<(bool, &'input [u8])> {
        match self {
            Self::Frame(value) => value.big_integer(),
            Self::Frame4(value) => value.big_integer(),
            Self::Merge(value) => value.big_integer(),
            Self::Flow(Raw::BigInteger {
                negative,
                little_endian,
            }) => Some((negative, little_endian)),
            _ => None,
        }
    }
    pub(in crate::policy::complete) fn len(self) -> usize {
        match self {
            Self::Frame(value) => value.len(),
            Self::Frame4(value) => value.len(),
            Self::Merge(value) => value.len(),
            Self::Flow(Raw::Map(rows)) => rows.len(),
            Self::Flow(
                Raw::List(rows) | Raw::Tuple(rows) | Raw::ImproperList { prefix: rows, .. },
            ) => rows.len(),
            Self::Retry(Retry::Map(rows)) => rows.len(),
            Self::Retry(Retry::Keyword(rows)) => rows.len(),
            Self::Indexed(Indexed::List(rows)) => rows.len(),
            _ => 0,
        }
    }
    pub(in crate::policy::complete) fn child(self, index: usize) -> Option<Self> {
        match self {
            Self::Frame(value) => value.child(index).map(Self::Frame),
            Self::Frame4(value) => value.child(index).map(Self::Frame4),
            Self::Merge(value) => value.child(index).map(Self::Merge),
            Self::Flow(
                Raw::List(rows) | Raw::Tuple(rows) | Raw::ImproperList { prefix: rows, .. },
            ) => rows.get(index).copied().map(Self::Flow),
            Self::Indexed(Indexed::List(rows)) => rows.get(index).copied().map(Self::Indexed),
            _ => None,
        }
    }
    pub(in crate::policy::complete) fn row(self, index: usize) -> Option<(Self, Self)> {
        match self {
            Self::Frame(value) => value
                .row(index)
                .map(|(key, child)| (Self::Frame(key), Self::Frame(child))),
            Self::Frame4(value) => value
                .row(index)
                .map(|(k, v)| (Self::Frame4(k), Self::Frame4(v))),
            Self::Merge(value) => value
                .row(index)
                .map(|(k, v)| (Self::Merge(k), Self::Merge(v))),
            Self::Flow(Raw::Map(rows)) => rows
                .get(index)
                .map(|&(k, v)| (Self::Flow(k), Self::Flow(v))),
            Self::Retry(Retry::Map(rows)) => rows.get(index).map(|&(k, v)| {
                let k = match k {
                    Key::Atom(s) => Raw::Atom(s),
                    Key::Binary(b) => Raw::Binary(b),
                    Key::Other => Raw::Other,
                };
                (Self::Flow(k), Self::Retry(v))
            }),
            Self::Retry(Retry::Keyword(rows)) => rows
                .get(index)
                .map(|&(k, v)| (Self::Flow(Raw::Atom(k)), Self::Retry(v))),
            _ => None,
        }
    }
    pub(in crate::policy::complete) fn tail(self) -> Option<Self> {
        match self {
            Self::Frame(value) => value.tail().map(Self::Frame),
            Self::Frame4(value) => value.tail().map(Self::Frame4),
            Self::Merge(value) => value.tail().map(Self::Merge),
            Self::Flow(Raw::ImproperList { tail, .. }) => Some(Self::Flow(*tail)),
            _ => None,
        }
    }
    pub(in crate::policy::complete) fn list_item(
        mut self,
        mut index: usize,
        charge: &mut PolicySchemaCharge,
    ) -> Result<Option<Self>, Error> {
        loop {
            if !matches!(
                self.kind(),
                PolicyInputKind::List | PolicyInputKind::ImproperList
            ) {
                return Err(Error::Schema(PolicySchemaError::InvalidAuthority));
            }
            if index < self.len() {
                return Ok(self.child(index));
            }
            index -= self.len();
            let Some(tail) = self.tail() else {
                return Ok(None);
            };
            visit(charge, 0)?;
            self = tail;
        }
    }
}

impl<'input> PolicyInputMap<'input> {
    pub(in crate::policy::complete) fn bind_source_order(
        self,
        charge: &mut PolicySchemaCharge,
    ) -> Result<Self, Error> {
        match self {
            Self::Direct(PolicyInputCursor::Merge(value)) => Ok(Self::Direct(
                PolicyInputCursor::Merge(value.bind_source_order(charge)?),
            )),
            Self::Direct(PolicyInputCursor::Frame4(value)) => {
                // This new adapter's sidecar is already source ordered.
                for index in 0..value.len() {
                    visit(charge, 0)?;
                    let (key, child) = value.row(index).ok_or(Error::Malformed)?;
                    if let Some(bytes) = key.atom_bytes() {
                        visit(charge, bytes.len().checked_add(10).ok_or_else(overflow)?)?;
                        if bytes == b"__struct__" && child.atom_bytes().is_some() {
                            return Err(Error::Schema(PolicySchemaError::InvalidAuthority));
                        }
                    }
                }
                Ok(self)
            }
            _ => Ok(self),
        }
    }

    pub(in crate::policy::complete) fn source_row_at(
        &self,
        index: usize,
        charge: &mut PolicySchemaCharge,
    ) -> Result<(usize, PolicyInputCursor<'input>, PolicyInputCursor<'input>), Error> {
        if let Self::Direct(PolicyInputCursor::Merge(value)) = *self {
            let (logical, key, child) = value.source_row(index).ok_or(Error::Malformed)?;
            return Ok((
                logical,
                PolicyInputCursor::Merge(key),
                PolicyInputCursor::Merge(child),
            ));
        }
        let (key, child) = self.row_at(index, charge)?;
        Ok((index, key, child))
    }

    pub(in crate::policy::complete) fn len(
        &self,
        charge: &mut PolicySchemaCharge,
    ) -> Result<usize, Error> {
        match *self {
            Self::Empty => Ok(0),
            Self::Direct(c) => Ok(c.len()),
            Self::Keyword(c) if c.kind() == PolicyInputKind::Keyword => Ok(c.len()),
            Self::Keyword(mut c) | Self::StatePairs(mut c) => {
                let mut length = c.len();
                while let Some(tail) = c.tail() {
                    visit(charge, 0)?;
                    c = tail;
                    if !matches!(
                        c.kind(),
                        PolicyInputKind::List | PolicyInputKind::ImproperList
                    ) {
                        return Err(Error::Malformed);
                    }
                    length = length.checked_add(c.len()).ok_or_else(overflow)?;
                }
                Ok(length)
            }
        }
    }
    pub(in crate::policy::complete) fn row_at(
        &self,
        index: usize,
        charge: &mut PolicySchemaCharge,
    ) -> Result<(PolicyInputCursor<'input>, PolicyInputCursor<'input>), Error> {
        match *self {
            Self::Empty => Err(Error::Malformed),
            Self::Direct(c) => c.row(index).ok_or(Error::Malformed),
            Self::Keyword(c) if c.kind() == PolicyInputKind::Keyword => {
                c.row(index).ok_or(Error::Malformed)
            }
            Self::Keyword(c) | Self::StatePairs(c) => {
                let tuple = c.list_item(index, charge)?.ok_or(Error::Malformed)?;
                Ok((
                    tuple.child(0).ok_or(Error::Malformed)?,
                    tuple.child(1).ok_or(Error::Malformed)?,
                ))
            }
        }
    }
}

pub(in crate::policy::complete) fn classify_map<'input>(
    input: PolicyInputCursor<'input>,
    charge: &mut PolicySchemaCharge,
) -> Result<Option<PolicyInputMap<'input>>, Error> {
    Ok(match input.kind() {
        PolicyInputKind::Map => Some(PolicyInputMap::Direct(input)),
        PolicyInputKind::Keyword => Some(PolicyInputMap::Keyword(input)),
        PolicyInputKind::List | PolicyInputKind::ImproperList
            if super::preflight::keyword(input, charge)? =>
        {
            Some(PolicyInputMap::Keyword(input))
        }
        _ => None,
    })
}

pub(in crate::policy::complete) fn select<'input>(
    input: PolicyInputMap<'input>,
    name: &[u8],
    charge: &mut PolicySchemaCharge,
) -> Result<Option<PolicyInputCursor<'input>>, Error> {
    visit(charge, 0)?;
    let (mut atom, mut binary) = (None, None);
    for index in 0..input.len(charge)? {
        let (key, value) = input.row_at(index, charge)?;
        visit(charge, key.key_bytes().map_or(0, <[u8]>::len))?;
        if let Some(bytes) = key.key_bytes() {
            visit(
                charge,
                bytes.len().checked_add(name.len()).ok_or_else(overflow)?,
            )?;
            if bytes == name {
                if key.is_atom() {
                    atom = Some(value);
                } else {
                    binary = Some(value);
                }
            }
        }
    }
    Ok(atom.or(binary))
}

pub(in crate::policy::complete) fn unsigned(
    input: PolicyInputCursor<'_>,
    maximum: u64,
    charge: &mut PolicySchemaCharge,
) -> Result<Option<u64>, Error> {
    if let Some(value) = input.integer() {
        return Ok(u64::try_from(value).ok().filter(|&n| n <= maximum));
    }
    let Some((negative, bytes)) = input.big_integer() else {
        return Ok(None);
    };
    visit(charge, bytes.len())?;
    let length = bytes.iter().rposition(|&b| b != 0).map_or(0, |n| n + 1);
    if (negative && length != 0) || length > 8 {
        return Ok(None);
    }
    let mut value = [0; 8];
    value[..length].copy_from_slice(&bytes[..length]);
    Ok(Some(u64::from_le_bytes(value)).filter(|&n| n <= maximum))
}

pub(in crate::policy::complete) fn visit(
    charge: &mut PolicySchemaCharge,
    length: usize,
) -> Result<(), Error> {
    let fee = u64::try_from(length)
        .ok()
        .and_then(|n| n.checked_add(128))
        .ok_or_else(overflow)?;
    charge_schema(charge, fee, 0).map_err(Error::Schema)
}

fn overflow() -> Error {
    Error::Schema(PolicySchemaError::LengthOverflow)
}
