//! One continuous work and cumulative requested-owner account.

use std::mem::size_of;

use super::model::{
    HistoryValueEntry, HistoryValueError as E, HistoryValueKind, HistoryValueLimits as L,
    HistoryValuePlan, HistoryValueReference,
};
use super::overlay::{HistoryValue, HistoryValueOverlay, HistoryValueWrite};
use super::{HistoryValueReadBatch, HistoryValueReadResult};
use crate::value::{OriginalValueContext, OriginalValueDocument};

#[derive(Debug)]
pub(super) struct HistoryValueCharge {
    pub(super) work_units: usize,
    pub(super) requested_bytes: usize,
}

impl HistoryValueCharge {
    pub(super) fn new<T: ?Sized>(limits: &L) -> Result<Self, E> {
        // Fixed bootstrap checks six scalar bounds before an account exists.
        if [
            limits.maximum_entries,
            limits.maximum_references,
            limits.maximum_read_results,
            limits.maximum_writes,
            limits.maximum_work_units,
            limits.maximum_requested_bytes,
        ]
        .contains(&0)
        {
            return Err(E::InvalidLimits);
        }
        let mut charge = Self {
            work_units: 0,
            requested_bytes: 0,
        };
        // Four cohorts prepay bounded fixed sizing, fold and control setup.
        charge.events(limits, 4)?;
        let local = add(
            size_of::<[usize; 35]>(),
            add(
                size_of::<std::array::IntoIter<usize, 35>>(),
                size_of::<Result<usize, E>>(),
            )?,
        )?;
        charge.pay(limits, 0, local)?;
        charge.pay(limits, 0, fixed_bytes::<T>()?)?;
        Ok(charge)
    }

    pub(super) fn pay(&mut self, limits: &L, work: usize, requested: usize) -> Result<(), E> {
        let work = add(self.work_units, work)?;
        let requested = add(self.requested_bytes, requested)?;
        if work > limits.maximum_work_units || requested > limits.maximum_requested_bytes {
            return Err(E::Limit);
        }
        self.work_units = work;
        self.requested_bytes = requested;
        Ok(())
    }

    pub(super) fn events(&mut self, limits: &L, count: usize) -> Result<(), E> {
        // Logical scalar/control allowance, not native instructions or CPU time.
        self.pay(limits, mul(count, 128)?, 0)
    }

    pub(super) fn compare(&mut self, limits: &L, left: usize, right: usize) -> Result<(), E> {
        self.pay(limits, add(128, add(left, right)?)?, 0)
    }
}

pub(super) fn add(left: usize, right: usize) -> Result<usize, E> {
    left.checked_add(right).ok_or(E::Limit)
}

pub(super) fn mul(left: usize, right: usize) -> Result<usize, E> {
    left.checked_mul(right).ok_or(E::Limit)
}

fn fixed_bytes<T: ?Sized>() -> Result<usize, E> {
    // Phase-summed typed headers for preparation, collection, application and
    // write construction. Incoming T/context/diagnostic backing stays borrowed.
    let parts = [
        size_of::<HistoryValuePlan<'_, T>>(),
        size_of::<HistoryValueOverlay<'_, T>>(),
        size_of::<Result<HistoryValuePlan<'_, T>, E>>(),
        size_of::<Result<HistoryValueOverlay<'_, T>, E>>(),
        size_of::<Option<HistoryValueReadBatch<'_>>>(),
        size_of::<L>(),
        size_of::<HistoryValueCharge>(),
        size_of::<&OriginalValueContext<'_>>(),
        size_of::<Vec<&[u8]>>(),
        size_of::<Vec<Option<usize>>>(),
        size_of::<Vec<HistoryValueWrite>>(),
        size_of::<HistoryValueWrite>(),
        size_of::<HistoryValue>(),
        add(
            size_of::<Option<&HistoryValueReadResult<'_>>>(),
            mul(2, size_of::<Option<&[HistoryValueReadResult<'_>]>>())?,
        )?,
        size_of::<Result<(), E>>(),
        mul(4, size_of::<&[HistoryValueEntry<'_, T>]>())?,
        mul(8, size_of::<&[u8]>())?,
        mul(4, size_of::<(usize, usize, usize, bool)>())?,
        mul(4, size_of::<Option<usize>>())?,
        size_of::<Vec<u8>>(),
        size_of::<std::slice::Iter<'_, HistoryValueEntry<'_, T>>>(),
        size_of::<std::iter::Enumerate<std::slice::Iter<'_, HistoryValueEntry<'_, T>>>>(),
        size_of::<std::array::IntoIter<HistoryValueReference<'_>, 3>>(),
        size_of::<std::array::IntoIter<(HistoryValueKind, HistoryValueReference<'_>), 3>>(),
        size_of::<std::slice::Iter<'_, &[u8]>>(),
        size_of::<std::iter::Enumerate<std::slice::Iter<'_, &[u8]>>>(),
        size_of::<std::vec::IntoIter<&[u8]>>(),
        size_of::<std::slice::Iter<'_, HistoryValueReadResult<'_>>>(),
        size_of::<std::slice::Iter<'_, bool>>(),
        size_of::<std::ops::Range<usize>>(),
        size_of::<Result<Vec<&[u8]>, E>>(),
        size_of::<Result<Vec<Option<usize>>, E>>(),
        size_of::<Result<Vec<u8>, E>>(),
        size_of::<Result<Option<OriginalValueDocument>, E>>(),
        size_of::<Option<OriginalValueDocument>>(),
    ];
    parts.into_iter().try_fold(0, add)
}

pub(super) fn reserve<T>(
    output: &mut Vec<T>,
    target: usize,
    limits: &L,
    charge: &mut HistoryValueCharge,
) -> Result<(), E> {
    charge.events(limits, 1)?;
    if target <= output.capacity() {
        return Ok(());
    }
    let requested = mul(target, size_of::<T>())?;
    let relocation = mul(output.capacity(), size_of::<T>())?;
    let liability = add(requested, relocation)?;
    charge.pay(limits, add(liability, 1)?, liability)?;
    let additional = target.checked_sub(output.len()).ok_or(E::Limit)?;
    output
        .try_reserve_exact(additional)
        .map_err(E::ReserveFailed)
}

pub(super) fn copy(
    bytes: &[u8],
    limits: &L,
    charge: &mut HistoryValueCharge,
) -> Result<Vec<u8>, E> {
    charge.events(limits, 1)?;
    let mut output = Vec::new();
    reserve(&mut output, bytes.len(), limits, charge)?;
    charge.pay(limits, bytes.len(), 0)?;
    output.extend_from_slice(bytes);
    // The unchanged decoder checks the produced len AND actual capacity. A
    // requested reservation makes no allocator-overprovision guarantee.
    Ok(output)
}
