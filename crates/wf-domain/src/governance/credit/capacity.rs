//! Source expiry, deterministic idle draining and nonbinding reclaim requests.

use super::admission::{CreditCharge, product};
use super::{CreditError as E, CreditOwnerData};

pub(in crate::governance) fn expire(
    owner: &mut CreditOwnerData,
    now: u64,
    charge: &mut CreditCharge,
) -> Result<(), E> {
    charge.work(product(owner.leases.len(), 128)?)?;
    let retired = owner
        .leases
        .iter()
        .filter(|lease| lease.expires_at_ms <= now)
        .try_fold(0_usize, |total, lease| {
            total.checked_add(lease.reservations.len()).ok_or(E::Limit)
        })?;
    charge.retire_reservations(retired)?;
    owner.leases.retain(|lease| lease.expires_at_ms > now);
    rebalance(owner, charge)
}

/// Retires only detached expired generations whose page references were queued.
pub(in crate::governance) fn expire_detached_bounded(
    owner: &mut CreditOwnerData,
    now: u64,
    maximum_paged: usize,
    charge: &mut CreditCharge,
) -> Result<(), E> {
    charge.work(product(owner.leases.len(), 256)?)?;
    if owner
        .leases
        .iter()
        .any(|lease| !lease.reservations.is_empty())
    {
        return Err(E::InvalidState);
    }
    let mut retired = 0;
    owner.leases.retain(|lease| {
        if lease.expires_at_ms > now {
            return true;
        }
        if lease.reservation_page == 0 {
            return false;
        }
        if retired == maximum_paged {
            return true;
        }
        retired += 1;
        false
    });
    rebalance(owner, charge)
}

pub(super) fn rebalance(owner: &mut CreditOwnerData, charge: &mut CreditCharge) -> Result<(), E> {
    charge.work(product(owner.leases.len(), 256)?)?;
    let allocated = owner.leases.iter().try_fold(0_u64, |sum, lease| {
        sum.checked_add(lease.available)
            .and_then(|n| n.checked_add(lease.in_use))
            .ok_or(E::InvalidState)
    })?;
    let mut excess = allocated.saturating_sub(owner.limit);
    let mut drained = 0;
    for lease in &mut owner.leases {
        let amount = lease.available.min(excess);
        lease.available -= amount;
        lease.pending_reclaim = lease.pending_reclaim.min(lease.available);
        excess -= amount;
        drained += amount;
    }
    owner.free = owner.limit.saturating_sub(allocated - drained);
    Ok(())
}

pub(super) fn mark_reclaim(
    owner: &mut CreditOwnerData,
    requester: u64,
    amount: u64,
    charge: &mut CreditCharge,
) -> Result<(), E> {
    charge.work(product(owner.leases.len(), 128)?)?;
    let mut remaining = amount;
    for lease in &mut owner.leases {
        if lease.shard_id != requester && remaining != 0 {
            let reclaimable = lease.available - lease.pending_reclaim;
            let reclaim = reclaimable.min(remaining);
            lease.pending_reclaim += reclaim;
            remaining -= reclaim;
        }
    }
    Ok(())
}
