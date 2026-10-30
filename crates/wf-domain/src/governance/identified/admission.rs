//! Canonical detached input and bounded original-ID admission.

use super::{
    IdentifiedCreditMutation as Op, IdentifiedCreditRequest,
    protocol::{IdentifiedCreditPlan, PreparedIdentifiedCredit},
};
use crate::governance::credit::{
    CreditError as E, CreditOwnerData,
    admission::{CreditCharge, product},
};
use crate::workflow::MAX_EXACT_MILLIS as MAX;

pub(super) fn request(
    command: IdentifiedCreditRequest<'_>,
    now: u64,
    charge: &mut CreditCharge,
) -> Result<(), E> {
    charge.work(4096)?;
    charge.own(
        std::mem::size_of::<PreparedIdentifiedCredit<'_>>()
            .checked_add(std::mem::size_of::<IdentifiedCreditPlan<'_>>())
            .ok_or(E::Limit)?,
    )?;
    if command.scope.is_empty()
        || command.scope.len() > 65_535
        || now > MAX
        || command.shard_count == 0
        || command.shard_count > MAX
    {
        return Err(E::InvalidInput);
    }
    charge.work(command.scope.len())?;
    if let Some(config) = command.configuration
        && (config.limit > MAX
            || config.config_version.is_some_and(|version| version > MAX)
            || !matches!(command.mutation, Op::Lease { .. } | Op::Spend { .. }))
    {
        return Err(E::InvalidInput);
    }
    match command.mutation {
        Op::Lease {
            shard_id,
            amount,
            ttl_ms,
        } => {
            scalar(shard_id, amount, command.shard_count)?;
            deadline(now, ttl_ms)?;
        }
        Op::Spend {
            shard_id,
            lease_epoch,
            amount,
            ttl_ms,
            reservation_ids,
        } => {
            scalar(shard_id, amount, command.shard_count)?;
            if lease_epoch > MAX {
                return Err(E::InvalidInput);
            }
            if let Some(ttl) = ttl_ms {
                deadline(now, ttl)?;
            }
            ids(reservation_ids, amount, charge)?;
        }
        Op::Release {
            shard_id,
            amount,
            reservation_ids,
        } => {
            scalar(shard_id, amount, command.shard_count)?;
            ids(reservation_ids, amount, charge)?;
        }
        Op::Renew { shard_id, ttl_ms } => {
            scalar(shard_id, 1, command.shard_count)?;
            deadline(now, ttl_ms)?;
        }
        Op::Get | Op::Cleanup => {}
    }
    Ok(())
}

pub(super) fn detached(
    data: &CreditOwnerData,
    command: IdentifiedCreditRequest<'_>,
    charge: &mut CreditCharge,
) -> Result<(), E> {
    charge.work(
        product(data.leases.len(), 128)?
            .checked_add(data.scope.len())
            .ok_or(E::Limit)?,
    )?;
    if data.scope != command.scope
        || data.cleanup_tail + 1 - data.cleanup_head > 256
        || data
            .leases
            .iter()
            .any(|lease| lease.shard_id >= command.shard_count || !lease.reservations.is_empty())
    {
        return Err(E::InvalidState);
    }
    Ok(())
}

fn scalar(shard: u64, amount: u64, count: u64) -> Result<(), E> {
    if shard >= count || amount == 0 || amount > 1000 {
        Err(E::InvalidInput)
    } else {
        Ok(())
    }
}

fn deadline(now: u64, ttl: u64) -> Result<(), E> {
    if ttl == 0 || now.checked_add(ttl).is_none_or(|end| end > MAX) {
        Err(E::InvalidInput)
    } else {
        Ok(())
    }
}

fn ids(values: &[Vec<u8>], amount: u64, charge: &mut CreditCharge) -> Result<(), E> {
    charge.input_ids(values.len())?;
    if u64::try_from(values.len()).map_err(|_| E::Limit)? != amount {
        return Err(E::InvalidInput);
    }
    charge.work(product(values.len(), 16_384)?)?;
    charge.own(product(values.len(), std::mem::size_of::<&[u8]>())?)?;
    let mut ordered = Vec::new();
    ordered
        .try_reserve_exact(values.len())
        .map_err(|_| E::Allocation)?;
    for id in values {
        if id.is_empty() || id.len() > 256 {
            return Err(E::InvalidInput);
        }
        ordered.push(id.as_slice());
    }
    ordered.sort_unstable();
    if ordered.windows(2).any(|pair| pair[0] == pair[1]) {
        return Err(E::InvalidInput);
    }
    Ok(())
}
