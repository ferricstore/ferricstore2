//! Logical constant-time rows and one immutable reached native-order binding.

use super::{Error, PolicyMergeArena, PolicyMergeRef as Ref, authority, overflow, pay};
use crate::policy::complete::PolicySchemaCharge;
use crate::policy::complete::flow_input::value::PolicyInputKind as Kind;

#[derive(Clone, Copy)]
pub(in crate::policy::complete) struct PolicyMergeCursor<'input> {
    pub(super) arena: &'input PolicyMergeArena<'input>,
    pub(super) value: Ref<'input>,
    pub(super) forward: Option<&'input [u32]>,
}

impl std::fmt::Debug for PolicyMergeCursor<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PolicyMergeCursor").finish_non_exhaustive()
    }
}

impl<'input> PolicyMergeCursor<'input> {
    pub(in crate::policy::complete) fn kind(self) -> Kind {
        match self.value {
            Ref::Current(v) => v.kind(),
            Ref::Frame(v) => v.kind(),
            Ref::Map(_) => Kind::Map,
        }
    }
    pub(in crate::policy::complete) fn atom_bytes(self) -> Option<&'input [u8]> {
        match self.value {
            Ref::Current(v) => v.atom_bytes(),
            Ref::Frame(v) => v.atom_bytes(),
            Ref::Map(_) => None,
        }
    }
    pub(in crate::policy::complete) fn binary_bytes(self) -> Option<&'input [u8]> {
        match self.value {
            Ref::Current(v) => v.binary_bytes(),
            Ref::Frame(v) => v.binary_bytes(),
            Ref::Map(_) => None,
        }
    }
    pub(in crate::policy::complete) fn integer(self) -> Option<i128> {
        match self.value {
            Ref::Current(v) => v.integer(),
            Ref::Frame(v) => v.integer(),
            Ref::Map(_) => None,
        }
    }
    pub(in crate::policy::complete) fn big_integer(self) -> Option<(bool, &'input [u8])> {
        match self.value {
            Ref::Current(v) => v.big_integer(),
            Ref::Frame(v) => v.big_integer(),
            Ref::Map(_) => None,
        }
    }
    pub(in crate::policy::complete) fn len(self) -> usize {
        match self.value {
            Ref::Current(v) => v.len(),
            Ref::Frame(v) => v.len(),
            Ref::Map(id) => self.arena.maps.get(id).map_or(0, |m| m.len),
        }
    }
    pub(in crate::policy::complete) fn child(self, index: usize) -> Option<Self> {
        let value = match self.value {
            Ref::Current(v) => Ref::Current(v.child(index)?),
            Ref::Frame(v) => Ref::Frame(v.child(index)?),
            Ref::Map(_) => return None,
        };
        Some(Self {
            value,
            forward: None,
            ..self
        })
    }
    pub(in crate::policy::complete) fn tail(self) -> Option<Self> {
        let value = match self.value {
            Ref::Current(v) => Ref::Current(v.tail()?),
            Ref::Frame(v) => Ref::Frame(v.tail()?),
            Ref::Map(_) => return None,
        };
        Some(Self {
            value,
            forward: None,
            ..self
        })
    }
    pub(in crate::policy::complete) fn row(self, index: usize) -> Option<(Self, Self)> {
        let (key, value) = match self.value {
            Ref::Current(v) => {
                let (k, v) = v.row(index)?;
                (Ref::Current(k), Ref::Current(v))
            }
            Ref::Frame(v) => {
                let (k, v) = v.row(index)?;
                (Ref::Frame(k), Ref::Frame(v))
            }
            Ref::Map(id) => {
                let map = self.arena.maps.get(id)?;
                if index >= map.len {
                    return None;
                }
                let row = *self.arena.order.get(map.order.checked_add(index)?)?;
                let row = self.arena.rows.get(row)?;
                (row.key, row.value)
            }
        };
        Some((
            Self {
                value: key,
                forward: None,
                ..self
            },
            Self {
                value,
                forward: None,
                ..self
            },
        ))
    }

    pub(in crate::policy::complete) fn bind_source_order(
        mut self,
        charge: &mut PolicySchemaCharge,
    ) -> Result<Self, Error> {
        self.guard_rows(charge)?;
        if self.len() > 1
            && let Some(orders) = self.arena.orders
        {
            self.forward = match self.value {
                Ref::Current(v) => Some(
                    orders
                        .current_record(v.map_id().ok_or_else(authority)?, self.len(), charge)?
                        .0,
                ),
                Ref::Map(id) => Some(
                    orders
                        .merged_record(
                            u32::try_from(id).map_err(|_| overflow())?,
                            self.len(),
                            charge,
                        )?
                        .0,
                ),
                Ref::Frame(_) => None,
            };
        }
        Ok(self)
    }

    pub(in crate::policy::complete) fn needs_native_order(self) -> bool {
        self.arena.orders.is_none() && !matches!(self.value, Ref::Frame(_))
    }

    pub(in crate::policy::complete) fn source_row(
        self,
        index: usize,
    ) -> Option<(usize, Self, Self)> {
        let logical = match self.forward {
            Some(p) => usize::try_from(*p.get(index)?).ok()?,
            None => index,
        };
        let (key, value) = self.row(logical)?;
        Some((logical, key, value))
    }

    fn guard_rows(self, charge: &mut PolicySchemaCharge) -> Result<(), Error> {
        for index in 0..self.len() {
            pay(charge, 128)?;
            let (key, value) = self.row(index).ok_or(Error::Malformed)?;
            guard_pair(key, value, charge)?;
        }
        Ok(())
    }
}

pub(super) fn guard_pair(
    key: PolicyMergeCursor<'_>,
    value: PolicyMergeCursor<'_>,
    charge: &mut PolicySchemaCharge,
) -> Result<(), Error> {
    if let Some(bytes) = key.atom_bytes() {
        pay(
            charge,
            138_usize.checked_add(bytes.len()).ok_or_else(overflow)?,
        )?;
        if bytes == b"__struct__" && value.atom_bytes().is_some() {
            return Err(authority());
        }
    }
    Ok(())
}
