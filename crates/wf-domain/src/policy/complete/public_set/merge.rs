//! One borrowed source overlay and four prospectively bounded metadata owners.

use super::order::OriginalPolicySetMapOrders;
use super::{Error, PolicySchemaCharge, PolicySchemaError, charge_schema};
use crate::policy::complete::original_entry::indexed::OriginalPolicyCurrentCursor;
use crate::policy::complete::outer::public_set_frame::PolicySetFrameCursor;

mod apply;
mod equality;
mod rows;
mod value;

pub(in crate::policy::complete) use value::PolicyMergeCursor;

#[derive(Clone, Copy, Debug)]
enum PolicyMergeRef<'input> {
    Current(OriginalPolicyCurrentCursor<'input, 'input, 'input>),
    Frame(PolicySetFrameCursor<'input>),
    Map(usize),
}

struct PolicyMergeMap {
    last: usize,
    len: usize,
    staged: usize,
    table: usize,
    capacity: usize,
    order: usize,
}

struct PolicyMergeRow<'input> {
    key: PolicyMergeRef<'input>,
    value: PolicyMergeRef<'input>,
    hash: u64,
    previous: usize,
    live: bool,
}

pub(in crate::policy::complete) struct PolicyMergeArena<'input> {
    maps: Vec<PolicyMergeMap>,
    rows: Vec<PolicyMergeRow<'input>>,
    lookup: Vec<u64>,
    order: Vec<usize>,
    orders: Option<&'input OriginalPolicySetMapOrders<'input>>,
    root: usize,
}

impl<'input> PolicyMergeArena<'input> {
    pub(in crate::policy::complete) fn bound(count: usize, depth: usize) -> Result<usize, Error> {
        count
            .checked_mul(360)
            .and_then(|n| n.checked_add(512))
            .and_then(|n| n.checked_add(depth.checked_mul(128)?))
            .ok_or_else(overflow)
    }

    pub(in crate::policy::complete) fn new(
        count: usize,
        depth: usize,
        orders: &'input OriginalPolicySetMapOrders<'input>,
        live: usize,
        charge: &mut PolicySchemaCharge,
    ) -> Result<Self, Error> {
        Self::allocate(count, depth, Some(orders), live, charge)
    }

    pub(in crate::policy::complete) fn new_native(
        count: usize,
        depth: usize,
        live: usize,
        charge: &mut PolicySchemaCharge,
    ) -> Result<Self, Error> {
        Self::allocate(count, depth, None, live, charge)
    }

    fn allocate(
        count: usize,
        depth: usize,
        orders: Option<&'input OriginalPolicySetMapOrders<'input>>,
        live: usize,
        charge: &mut PolicySchemaCharge,
    ) -> Result<Self, Error> {
        let peak = live
            .checked_add(Self::bound(count, depth)?)
            .ok_or_else(overflow)?;
        peak_at(peak, charge)?;
        let mut arena = Self {
            maps: Vec::new(),
            rows: Vec::new(),
            lookup: Vec::new(),
            order: Vec::new(),
            orders,
            root: usize::MAX,
        };
        reserve(&mut arena.maps, count, 128, charge)?;
        reserve(&mut arena.rows, count, 160, charge)?;
        reserve(
            &mut arena.lookup,
            count.checked_mul(8).ok_or_else(overflow)?,
            8,
            charge,
        )?;
        reserve(&mut arena.order, count, 8, charge)?;
        Ok(arena)
    }

    pub(in crate::policy::complete) fn merge(
        &mut self,
        current: Option<OriginalPolicyCurrentCursor<'input, 'input, 'input>>,
        patch: PolicySetFrameCursor<'input>,
        charge: &mut PolicySchemaCharge,
    ) -> Result<(), Error> {
        self.root = apply::merge(
            self,
            current.map(PolicyMergeRef::Current),
            PolicyMergeRef::Frame(patch),
            false,
            charge,
        )?;
        rows::freeze(self, self.root, charge)
    }

    pub(in crate::policy::complete) fn cursor(&'input self) -> PolicyMergeCursor<'input> {
        self.view(PolicyMergeRef::Map(self.root))
    }

    fn view(&self, value: PolicyMergeRef<'input>) -> PolicyMergeCursor<'_> {
        PolicyMergeCursor {
            arena: self,
            value,
            forward: None,
        }
    }

    fn map(&mut self, charge: &mut PolicySchemaCharge) -> Result<usize, Error> {
        pay(charge, 128)?;
        if self.maps.len() == self.maps.capacity() {
            return Err(Error::ItemLimit);
        }
        let id = self.maps.len();
        self.maps.push(PolicyMergeMap {
            last: usize::MAX,
            len: 0,
            staged: 0,
            table: 0,
            capacity: 0,
            order: usize::MAX,
        });
        Ok(id)
    }
}

fn reserve<T>(
    target: &mut Vec<T>,
    count: usize,
    width: usize,
    charge: &mut PolicySchemaCharge,
) -> Result<(), Error> {
    pay(
        charge,
        count
            .checked_mul(width)
            .and_then(|n| n.checked_add(128))
            .ok_or_else(overflow)?,
    )?;
    target
        .try_reserve_exact(count)
        .map_err(|_| Error::AllocationFailure)
}

pub(in crate::policy::complete) fn peak_at(
    peak: usize,
    charge: &mut PolicySchemaCharge,
) -> Result<(), Error> {
    charge_schema(charge, 0, u64::try_from(peak).map_err(|_| overflow())?).map_err(Error::Schema)
}
fn pay(charge: &mut PolicySchemaCharge, amount: usize) -> Result<(), Error> {
    charge_schema(charge, u64::try_from(amount).map_err(|_| overflow())?, 0).map_err(Error::Schema)
}
fn overflow() -> Error {
    Error::Schema(PolicySchemaError::LengthOverflow)
}
fn authority() -> Error {
    Error::Schema(PolicySchemaError::InvalidAuthority)
}
