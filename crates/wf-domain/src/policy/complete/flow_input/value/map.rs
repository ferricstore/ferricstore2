//! Existing source-map adapters, kept separate from cursor scalar operations.

use super::{
    Error, PolicyInputCursor, PolicyInputKind, PolicySchemaCharge, PolicySchemaError, overflow,
    visit,
};

#[derive(Clone, Copy, Debug)]
pub(in crate::policy::complete) enum PolicyInputMap<'input> {
    Direct(PolicyInputCursor<'input>),
    Keyword(PolicyInputCursor<'input>),
    StatePairs(PolicyInputCursor<'input>),
    Empty,
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

    pub(in crate::policy::complete) fn needs_native_order(self) -> bool {
        matches!(self, Self::Direct(PolicyInputCursor::Merge(value)) if value.needs_native_order())
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
