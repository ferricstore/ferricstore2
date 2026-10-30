//! Continuous prospective owner-capacity, traversal and allocation admission.

use super::{
    CreditError as E, CreditLeaseData, CreditLimits, CreditOperation, CreditOwner, CreditOwnerData,
    CreditReservationValue, CreditTransition,
};
use crate::workflow::MAX_EXACT_MILLIS as MAX;
use std::mem::size_of;

pub(super) struct CreditCharge {
    limits: CreditLimits,
    work: u64,
    owned: usize,
    reservations: usize,
}

impl CreditCharge {
    pub(super) fn inspect(data: &CreditOwnerData, limits: CreditLimits) -> Result<Self, E> {
        let mut charge = Self {
            limits,
            work: 0,
            owned: 0,
            reservations: 0,
        };
        charge.work(4096)?;
        charge.own(
            size_of::<CreditOwner>()
                .checked_add(size_of::<CreditTransition>())
                .and_then(|n| n.checked_add(size_of::<Self>()))
                .and_then(|n| n.checked_add(size_of::<CreditOperation<'_>>()))
                .ok_or(E::Limit)?,
        )?;
        charge.own(data.scope.capacity())?;
        charge.own(product(
            data.leases.capacity(),
            size_of::<CreditLeaseData>(),
        )?)?;
        charge.work(data.scope.len())?;
        if data.scope.is_empty()
            || data.scope.len() > 65_535
            || data.limit > MAX
            || data.free > MAX
            || data.epoch > MAX
            || data.config_version > MAX
            || data.cleanup_head == 0
            || data.cleanup_head > MAX
            || data.cleanup_tail > MAX
            || data.cleanup_head > data.cleanup_tail + 1
        {
            return Err(E::InvalidState);
        }
        if data.leases.len() > limits.max_leases() {
            return Err(E::Limit);
        }
        let mut previous = None;
        let mut allocated = 0_u64;
        for lease in &data.leases {
            charge.work(256)?;
            if previous.is_some_and(|shard| shard >= lease.shard_id) {
                return Err(E::InvalidState);
            }
            inspect_lease(lease, data.epoch, &mut charge)?;
            allocated = allocated
                .checked_add(lease.available)
                .and_then(|n| n.checked_add(lease.in_use))
                .filter(|n| *n <= MAX)
                .ok_or(E::InvalidState)?;
            previous = Some(lease.shard_id);
        }
        if data.free != data.limit.saturating_sub(allocated) {
            return Err(E::InvalidState);
        }
        Ok(charge)
    }

    pub(super) fn work(&mut self, bytes: usize) -> Result<(), E> {
        self.work = self
            .work
            .checked_add(u64::try_from(bytes).map_err(|_| E::Limit)?)
            .filter(|n| *n <= self.limits.max_work())
            .ok_or(E::Limit)?;
        Ok(())
    }

    pub(super) fn own(&mut self, bytes: usize) -> Result<(), E> {
        self.temporary(bytes)?;
        self.owned = self.owned.checked_add(bytes).ok_or(E::Limit)?;
        Ok(())
    }

    pub(super) fn temporary(&self, bytes: usize) -> Result<(), E> {
        self.owned
            .checked_add(bytes)
            .filter(|n| *n <= self.limits.max_owned_bytes())
            .ok_or(E::Limit)
            .map(|_| ())
    }

    pub(super) fn lease_count(&self, count: usize) -> Result<(), E> {
        if count > self.limits.max_leases() {
            Err(E::Limit)
        } else {
            Ok(())
        }
    }

    pub(super) fn add_reservations(&mut self, count: usize) -> Result<(), E> {
        self.reservations = self
            .reservations
            .checked_add(count)
            .filter(|n| *n <= self.limits.max_reservations())
            .ok_or(E::Limit)?;
        Ok(())
    }

    pub(super) fn input_ids(&self, count: usize) -> Result<(), E> {
        if count > self.limits.max_reservations() {
            Err(E::Limit)
        } else {
            Ok(())
        }
    }

    pub(super) fn retire_reservations(&mut self, count: usize) -> Result<(), E> {
        self.reservations = self
            .reservations
            .checked_sub(count)
            .ok_or(E::InvalidState)?;
        Ok(())
    }
}

fn inspect_lease(lease: &CreditLeaseData, epoch: u64, charge: &mut CreditCharge) -> Result<(), E> {
    let rate = f64::from_bits(lease.drain_rate_bits);
    if lease.shard_id > MAX
        || lease.epoch == 0
        || lease.epoch > epoch
        || lease.expires_at_ms > MAX
        || lease.available > MAX
        || lease.in_use > MAX
        || lease.pending_reclaim > lease.available
        || !rate.is_finite()
        || rate < 0.0
        || rate > 9_007_199_254_740_991.0
        || lease.last_spend_at_ms.is_some_and(|time| time > MAX)
        || lease.reservation_page > 256
        || lease.reservation_page_fill > 256
        || (lease.reservation_page == 0) != (lease.reservation_page_fill == 0)
    {
        return Err(E::InvalidState);
    }
    charge.add_reservations(lease.reservations.len())?;
    charge.own(product(
        lease.reservations.capacity(),
        size_of::<(Vec<u8>, CreditReservationValue)>(),
    )?)?;
    let mut previous = None;
    for (id, value) in &lease.reservations {
        charge.work(128_usize.checked_add(id.len()).ok_or(E::Limit)?)?;
        charge.own(id.capacity())?;
        if id.is_empty()
            || id.len() > 256
            || previous.is_some_and(|key: &[u8]| key >= id.as_slice())
        {
            return Err(E::InvalidState);
        }
        match value {
            CreditReservationValue::Amount(amount) if *amount > MAX => return Err(E::InvalidState),
            CreditReservationValue::Opaque(bytes) => {
                charge.work(bytes.len())?;
                charge.own(bytes.capacity())?;
            }
            CreditReservationValue::Amount(_) => {}
        }
        previous = Some(id.as_slice());
    }
    Ok(())
}

pub(super) fn product(left: usize, right: usize) -> Result<usize, E> {
    left.checked_mul(right).ok_or(E::Limit)
}

pub(super) fn reserve<T>(
    values: &mut Vec<T>,
    count: usize,
    charge: &mut CreditCharge,
) -> Result<(), E> {
    let length = values.len().checked_add(count).ok_or(E::Limit)?;
    if length > values.capacity() {
        charge.work(product(values.len(), size_of::<T>())?)?;
        charge.temporary(product(length, size_of::<T>())?)?;
        charge.own(product(length - values.capacity(), size_of::<T>())?)?;
    }
    values.try_reserve_exact(count).map_err(|_| E::Allocation)
}
