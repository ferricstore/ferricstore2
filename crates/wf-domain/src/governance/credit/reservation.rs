//! Identified membership and ordered move-only insertion/release, without maps.

use super::admission::{CreditCharge, product, reserve};
use super::{CreditError as E, CreditLeaseData, CreditReservationValue as Value};
use std::{cmp::Ordering, mem::size_of};

pub(super) fn valid_ids(ids: &[&[u8]], amount: u64, charge: &mut CreditCharge) -> Result<bool, E> {
    charge.input_ids(ids.len())?;
    charge.work(product(ids.len(), 128)?)?;
    if u64::try_from(ids.len()).map_err(|_| E::Limit)? != amount || ids.is_empty() {
        return Ok(false);
    }
    let mut ordered: Vec<&[u8]> = Vec::new();
    reserve(&mut ordered, ids.len(), charge)?;
    for id in ids {
        charge.work(id.len())?;
        if id.is_empty() {
            return Ok(false);
        }
        if id.len() > 256 {
            return Err(E::InvalidInput);
        }
        let (mut low, mut high) = (0, ordered.len());
        while low < high {
            let mid = low + (high - low) / 2;
            charge.work(ordered[mid].len().checked_add(id.len()).ok_or(E::Limit)?)?;
            match ordered[mid].cmp(id) {
                Ordering::Equal => return Ok(false),
                Ordering::Less => low = mid + 1,
                Ordering::Greater => high = mid,
            }
        }
        charge.work(product(ordered.len() - low, size_of::<&[u8]>())?)?;
        ordered.insert(low, id);
    }
    Ok(true)
}

pub(super) fn position(
    lease: &CreditLeaseData,
    id: &[u8],
    charge: &mut CreditCharge,
) -> Result<Result<usize, usize>, E> {
    let (mut low, mut high) = (0, lease.reservations.len());
    while low < high {
        let mid = low + (high - low) / 2;
        let candidate = &lease.reservations[mid].0;
        charge.work(
            candidate
                .len()
                .checked_add(id.len())
                .and_then(|n| n.checked_add(16))
                .ok_or(E::Limit)?,
        )?;
        match candidate.as_slice().cmp(id) {
            Ordering::Equal => return Ok(Ok(mid)),
            Ordering::Less => low = mid + 1,
            Ordering::Greater => high = mid,
        }
    }
    Ok(Err(low))
}

pub(super) fn present(
    lease: &CreditLeaseData,
    ids: &[&[u8]],
    charge: &mut CreditCharge,
) -> Result<usize, E> {
    let mut present = 0;
    for id in ids {
        if position(lease, id, charge)?.is_ok() {
            present += 1;
        }
    }
    Ok(present)
}

pub(super) fn insert(
    lease: &mut CreditLeaseData,
    ids: &[&[u8]],
    charge: &mut CreditCharge,
) -> Result<(), E> {
    charge.add_reservations(ids.len())?;
    reserve(&mut lease.reservations, ids.len(), charge)?;
    for id in ids {
        let index = position(lease, id, charge)?.err().ok_or(E::InvalidState)?;
        charge.work(product(
            lease.reservations.len() - index,
            size_of::<(Vec<u8>, Value)>(),
        )?)?;
        charge.work(id.len())?;
        charge.own(id.len())?;
        let mut copied = Vec::new();
        copied
            .try_reserve_exact(id.len())
            .map_err(|_| E::Allocation)?;
        copied.extend_from_slice(id);
        lease.reservations.insert(index, (copied, Value::Amount(1)));
    }
    Ok(())
}

pub(super) fn release(
    lease: &mut CreditLeaseData,
    ids: &[&[u8]],
    charge: &mut CreditCharge,
) -> Result<u64, E> {
    let mut released = 0;
    for id in ids {
        let Ok(index) = position(lease, id, charge)? else {
            continue;
        };
        let Value::Amount(amount) = lease.reservations[index].1 else {
            continue;
        };
        if amount == 0 {
            continue;
        }
        charge.work(product(
            lease.reservations.len() - index - 1,
            size_of::<(Vec<u8>, Value)>(),
        )?)?;
        released += amount.min(lease.in_use - released);
        lease.reservations.remove(index);
    }
    Ok(released)
}
