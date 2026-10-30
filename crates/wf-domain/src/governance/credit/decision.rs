//! Source owner operations consume canonical data; no remote confirmation occurs.

use super::admission::{CreditCharge, product, reserve};
use super::{
    CreditDecision as D, CreditError as E, CreditLeaseData, CreditOperation as Op, CreditOwnerData,
    CreditRejection as R, capacity, reservation,
};
use crate::workflow::MAX_EXACT_MILLIS as MAX;

pub(super) fn apply(
    owner: &mut CreditOwnerData,
    operation: Op<'_>,
    charge: &mut CreditCharge,
) -> Result<D, E> {
    charge.work(128)?;
    match operation {
        Op::Configure {
            limit,
            config_version,
            policy_fingerprint,
        } => configure(owner, limit, config_version, policy_fingerprint, charge),
        Op::Grant {
            shard_id,
            amount,
            now,
            ttl_ms,
        } => {
            input(shard_id, amount)?;
            let deadline = deadline(now.get(), ttl_ms)?;
            capacity::expire(owner, now.get(), charge)?;
            grant(owner, shard_id, amount, deadline, charge)
        }
        Op::Spend {
            shard_id,
            amount,
            now,
            ttl_ms,
            reservation_ids,
        } => {
            input(shard_id, amount)?;
            let end = ttl_ms.map(|ttl| deadline(now.get(), ttl)).transpose()?;
            if now.get() > MAX {
                return Err(E::InvalidInput);
            }
            capacity::expire(owner, now.get(), charge)?;
            spend(
                owner,
                (shard_id, amount, now.get(), end),
                reservation_ids,
                charge,
            )
        }
        Op::Renew {
            shard_id,
            now,
            ttl_ms,
        } => {
            input(shard_id, 1)?;
            let end = deadline(now.get(), ttl_ms)?;
            capacity::expire(owner, now.get(), charge)?;
            let Ok(index) = locate(owner, shard_id, charge)? else {
                return Ok(D::Denied(R::LeaseExpired));
            };
            owner.leases[index].expires_at_ms = owner.leases[index].expires_at_ms.max(end);
            Ok(D::Renewed { shard_id })
        }
        Op::Release {
            shard_id,
            amount,
            reservation_ids,
        } => {
            input(shard_id, amount)?;
            let Some(ids) = reservation_ids else {
                return Ok(D::Released);
            };
            if !reservation::valid_ids(ids, amount, charge)? {
                return Ok(D::Released);
            }
            let Ok(index) = locate(owner, shard_id, charge)? else {
                return Ok(D::Released);
            };
            let released = reservation::release(&mut owner.leases[index], ids, charge)?;
            release_amount(owner, index, released, charge)
        }
        Op::ReleaseIdentifiedAmount { shard_id, amount } => {
            if shard_id > MAX || amount > MAX {
                return Err(E::InvalidInput);
            }
            let Ok(index) = locate(owner, shard_id, charge)? else {
                return Ok(D::Released);
            };
            release_amount(owner, index, amount, charge)
        }
        Op::ReclaimExpired { now } => {
            if now.get() > MAX {
                return Err(E::InvalidInput);
            }
            capacity::expire(owner, now.get(), charge)?;
            Ok(D::Reclaimed)
        }
    }
}

fn configure(
    owner: &mut CreditOwnerData,
    limit: u64,
    version: u64,
    fingerprint: Option<crate::workflow::control::PreparedDigest>,
    charge: &mut CreditCharge,
) -> Result<D, E> {
    if limit > MAX || version > MAX {
        return Err(E::InvalidInput);
    }
    if version < owner.config_version {
        return Ok(D::Configured);
    }
    if version == owner.config_version {
        return Ok(
            if limit != owner.limit
                || fingerprint.is_some_and(|hash| Some(hash) != owner.policy_fingerprint)
            {
                D::Denied(R::ConfigurationConflict)
            } else {
                D::Configured
            },
        );
    }
    owner.limit = limit;
    owner.config_version = version;
    owner.policy_fingerprint = fingerprint;
    capacity::rebalance(owner, charge)?;
    Ok(D::Configured)
}

fn grant(
    owner: &mut CreditOwnerData,
    shard_id: u64,
    requested: u64,
    deadline: u64,
    charge: &mut CreditCharge,
) -> Result<D, E> {
    let amount = requested.min(owner.free);
    if amount == 0 {
        capacity::mark_reclaim(owner, shard_id, requested, charge)?;
        return Ok(exhausted(owner, requested));
    }
    let index = match locate(owner, shard_id, charge)? {
        Ok(index) => index,
        Err(index) => {
            if owner.epoch == MAX {
                return Ok(D::Denied(R::GenerationExhausted));
            }
            charge.lease_count(owner.leases.len().checked_add(1).ok_or(E::Limit)?)?;
            reserve(&mut owner.leases, 1, charge)?;
            charge.work(product(
                owner.leases.len() - index,
                std::mem::size_of::<CreditLeaseData>(),
            )?)?;
            let epoch = owner.epoch + 1;
            owner.leases.insert(
                index,
                CreditLeaseData {
                    shard_id,
                    epoch,
                    ..CreditLeaseData::default()
                },
            );
            owner.epoch = epoch;
            index
        }
    };
    let lease = &mut owner.leases[index];
    lease.available += amount;
    lease.expires_at_ms = lease.expires_at_ms.max(deadline);
    lease.pending_reclaim = lease.pending_reclaim.saturating_sub(amount);
    owner.free -= amount;
    Ok(D::Granted { shard_id })
}

fn spend(
    owner: &mut CreditOwnerData,
    (shard_id, amount, now, end): (u64, u64, u64, Option<u64>),
    ids: &[&[u8]],
    charge: &mut CreditCharge,
) -> Result<D, E> {
    let Ok(index) = locate(owner, shard_id, charge)? else {
        return Ok(exhausted(owner, amount));
    };
    if !ids.is_empty() && !reservation::valid_ids(ids, amount, charge)? {
        return Ok(D::Denied(R::ReservationIdsMismatch));
    }
    let present = reservation::present(&owner.leases[index], ids, charge)?;
    if !ids.is_empty() && present == ids.len() {
        return Ok(D::Spent { shard_id });
    }
    if present != 0 {
        return Ok(D::Denied(R::ReservationConflict));
    }
    let lease = &mut owner.leases[index];
    if lease.last_spend_at_ms.is_some_and(|last| now < last) {
        return Ok(D::Denied(R::BackwardTime));
    }
    if lease.available < amount {
        return Ok(exhausted(owner, amount));
    }
    reservation::insert(lease, ids, charge)?;
    lease.available -= amount;
    lease.pending_reclaim = lease.pending_reclaim.min(lease.available);
    lease.in_use += amount;
    lease.last_spend_at_ms = Some(now);
    if let Some(end) = end {
        lease.expires_at_ms = lease.expires_at_ms.max(end);
    }
    Ok(D::Spent { shard_id })
}

fn release_amount(
    owner: &mut CreditOwnerData,
    index: usize,
    amount: u64,
    charge: &mut CreditCharge,
) -> Result<D, E> {
    let lease = &mut owner.leases[index];
    let amount = amount.min(lease.in_use);
    lease.in_use -= amount;
    lease.available += amount;
    capacity::rebalance(owner, charge)?;
    Ok(D::Released)
}

fn locate(
    owner: &CreditOwnerData,
    shard_id: u64,
    charge: &mut CreditCharge,
) -> Result<Result<usize, usize>, E> {
    charge.work(product(owner.leases.len(), 16)?)?;
    Ok(owner
        .leases
        .binary_search_by_key(&shard_id, |lease| lease.shard_id))
}

fn input(shard_id: u64, amount: u64) -> Result<(), E> {
    if shard_id > MAX || amount == 0 || amount > MAX {
        Err(E::InvalidInput)
    } else {
        Ok(())
    }
}

fn deadline(now: u64, ttl: u64) -> Result<u64, E> {
    if now > MAX || ttl == 0 {
        return Err(E::InvalidInput);
    }
    now.checked_add(ttl)
        .filter(|n| *n <= MAX)
        .ok_or(E::InvalidInput)
}

fn exhausted(owner: &CreditOwnerData, requested: u64) -> D {
    D::Denied(R::LimitExceeded {
        requested,
        free: owner.free,
        limit: owner.limit,
    })
}
