//! Bounded borrowed request validation without copying original ID owners.

use super::{CodecError as E, CodecLimits};
use wf_domain::governance::identified::{IdentifiedCreditMutation as Op, IdentifiedCreditRequest};
use wf_domain::workflow::MAX_EXACT_MILLIS as MAX;

pub(super) fn validate(
    command: IdentifiedCreditRequest<'_>,
    now: u64,
    limits: CodecLimits,
    maximum_owned: usize,
) -> Result<(), E> {
    if command.scope.is_empty()
        || command.scope.len() > 65_535
        || now > MAX
        || command.shard_count == 0
        || command.shard_count > MAX
    {
        return Err(E::Malformed);
    }
    if let Some(config) = command.configuration
        && (config.limit > MAX
            || config.config_version.is_some_and(|version| version > MAX)
            || !matches!(command.mutation, Op::Lease { .. } | Op::Spend { .. }))
    {
        return Err(E::Malformed);
    }
    match command.mutation {
        Op::Lease {
            shard_id,
            amount,
            ttl_ms,
        } => {
            shard(shard_id, command.shard_count)?;
            amount_bound(amount)?;
            deadline(now, ttl_ms)?;
        }
        Op::Spend {
            shard_id,
            lease_epoch,
            amount,
            ttl_ms,
            reservation_ids,
        } => {
            shard(shard_id, command.shard_count)?;
            if lease_epoch > MAX {
                return Err(E::Malformed);
            }
            if let Some(ttl) = ttl_ms {
                deadline(now, ttl)?;
            }
            ids(reservation_ids, amount, limits, maximum_owned)?;
        }
        Op::Release {
            shard_id,
            amount,
            reservation_ids,
        } => {
            shard(shard_id, command.shard_count)?;
            ids(reservation_ids, amount, limits, maximum_owned)?;
        }
        Op::Renew { shard_id, ttl_ms } => {
            shard(shard_id, command.shard_count)?;
            deadline(now, ttl_ms)?;
        }
        Op::Get | Op::Cleanup => {}
    }
    Ok(())
}

fn shard(shard: u64, count: u64) -> Result<(), E> {
    if shard < count {
        Ok(())
    } else {
        Err(E::Malformed)
    }
}

fn amount_bound(amount: u64) -> Result<(), E> {
    if (1..=1000).contains(&amount) {
        Ok(())
    } else {
        Err(E::Malformed)
    }
}

fn deadline(now: u64, ttl: u64) -> Result<(), E> {
    if ttl == 0 || now.checked_add(ttl).is_none_or(|end| end > MAX) {
        return Err(E::Malformed);
    }
    Ok(())
}

pub(super) fn ids(
    values: &[Vec<u8>],
    amount: u64,
    limits: CodecLimits,
    maximum_owned: usize,
) -> Result<(), E> {
    amount_bound(amount)?;
    if values.len() > limits.admission().batch_items()
        || u64::try_from(values.len()).map_err(|_| E::LengthOverflow)? != amount
    {
        return Err(E::ItemLimit);
    }
    let owned = values
        .len()
        .checked_mul(std::mem::size_of::<&[u8]>())
        .and_then(|n| n.checked_add(2048))
        .ok_or(E::LengthOverflow)?;
    if owned > maximum_owned {
        return Err(E::OwnedLimit);
    }
    let mut ordered = Vec::new();
    ordered
        .try_reserve_exact(values.len())
        .map_err(|_| E::Allocation)?;
    for value in values {
        if value.is_empty() || value.len() > 256 {
            return Err(E::Malformed);
        }
        ordered.push(value.as_slice());
    }
    ordered.sort_unstable();
    if ordered.windows(2).any(|pair| pair[0] == pair[1]) {
        return Err(E::Malformed);
    }
    Ok(())
}
