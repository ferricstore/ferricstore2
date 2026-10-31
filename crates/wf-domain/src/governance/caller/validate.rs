//! Complete typed restoration checks shared by normalization and wire callers.
use super::{
    CreditCallAction as A, CreditCallConfiguration as C, CreditCallerError as E, CreditIdList,
    CreditPolicyInput as P,
};
use crate::workflow::MAX_EXACT_MILLIS as MAX;

pub(super) fn parts(scope: &[u8], count: u64, now: Option<u64>, action: A<'_>) -> Result<(), E> {
    if scope.is_empty() || scope.len() > 65_535 || !(1..=MAX).contains(&count) {
        return Err(E::InvalidOptions);
    }
    if now.is_some_and(|n| n > MAX) {
        return Err(E::InvalidField("now_ms"));
    }
    match action {
        A::Lease {
            shard_id,
            amount,
            ttl_ms,
            configuration,
        } => {
            shard(shard_id, count)?;
            amount_bound(amount)?;
            deadline(now, ttl_ms)?;
            configuration.map_or(Ok(()), config)
        }
        A::Spend {
            shard_id,
            amount,
            ttl_ms,
            configuration,
        } => {
            shard(shard_id, count)?;
            amount_bound(amount)?;
            if let Some(ttl) = ttl_ms {
                deadline(now, ttl)?;
            }
            configuration.map_or(Ok(()), config)
        }
        A::Release {
            shard_id,
            amount,
            reservation_ids,
        } => {
            shard(shard_id, count)?;
            ids(reservation_ids)?;
            if u64::try_from(reservation_ids.len()).ok() != Some(amount) {
                return Err(E::ReservationIds);
            }
            Ok(())
        }
        A::Renew { shard_id, ttl_ms } => {
            shard(shard_id, count)?;
            deadline(now, ttl_ms)
        }
        A::Get | A::Cleanup => Ok(()),
    }
}
pub(super) fn shard(shard: u64, count: u64) -> Result<(), E> {
    if shard < count {
        Ok(())
    } else {
        Err(E::Shard {
            maximum: count.saturating_sub(1),
        })
    }
}
pub(super) fn amount_bound(amount: u64) -> Result<(), E> {
    if (1..=1000).contains(&amount) {
        Ok(())
    } else {
        Err(E::Amount)
    }
}
pub(super) fn deadline(now: Option<u64>, ttl: u64) -> Result<(), E> {
    if ttl == 0 || ttl > MAX {
        return Err(E::InvalidField("ttl_ms"));
    }
    if now.is_some_and(|now| now.checked_add(ttl).is_none_or(|n| n > MAX)) {
        return Err(E::Deadline);
    }
    Ok(())
}
fn config(value: C<'_>) -> Result<(), E> {
    if value.limit > MAX {
        return Err(E::InvalidField("limit"));
    }
    if value.config_version.is_some_and(|n| n > MAX) {
        return Err(E::InvalidField("config_version"));
    }
    match value.policy_version {
        Some(P::Number(n)) if n > MAX => return Err(E::PolicyVersion),
        Some(P::Number(_)) if value.config_version.is_none() => {
            return Err(E::InvalidField("config_version"));
        }
        Some(P::Bytes(bytes)) if bytes.is_empty() || bytes.len() > 262_144 => {
            return Err(E::PolicyVersion);
        }
        Some(P::Bytes(_)) if value.config_version.is_none() => {
            return Err(E::PolicyVersionRequiresConfig);
        }
        _ => (),
    }
    Ok(())
}
pub(super) fn ids(ids: CreditIdList<'_>) -> Result<(), E> {
    if ids.is_empty() || ids.len() > 1000 {
        return Err(E::ReservationIds);
    }
    let mut ordered = Vec::new();
    ordered
        .try_reserve_exact(ids.len())
        .map_err(|_| E::Allocation)?;
    for id in ids.iter() {
        if id.is_empty() || id.len() > 256 {
            return Err(E::ReservationIds);
        }
        ordered.push(id);
    }
    ordered.sort_unstable();
    if ordered.windows(2).any(|pair| pair[0] == pair[1]) {
        return Err(E::ReservationIds);
    }
    Ok(())
}
