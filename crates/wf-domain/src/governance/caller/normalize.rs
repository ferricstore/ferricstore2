//! Raw keyword admission and allocation-free first-exact-key selection.
use super::{
    CreditCallAction as A, CreditCallConfiguration as C, CreditCallKind as K,
    CreditCallerError as E, CreditOptionValue as V, CreditPolicyInput as P, NormalizedCreditCall,
    validate,
};
use crate::workflow::MAX_EXACT_MILLIS as MAX;

/// Captures bounded source keyword options without reading owner state or time.
/// # Errors
/// Refuses invalid options, bounds, ID/configuration fields and raw-byte limits.
pub fn normalize_credit_call<'a>(
    kind: K,
    scope: &'a [u8],
    count: u64,
    options: &[(&str, V<'a>)],
    maximum_raw_bytes: usize,
) -> Result<NormalizedCreditCall<'a>, E> {
    measure(scope, options, maximum_raw_bytes)?;
    if scope.is_empty() || scope.len() > 65_535 || !(1..=MAX).contains(&count) {
        return Err(E::InvalidOptions);
    }
    let shard = if matches!(kind, K::Get | K::Cleanup) {
        0
    } else {
        let value = number(options, "shard_id")
            .ok()
            .flatten()
            .filter(|n| *n < count)
            .ok_or(E::Shard { maximum: count - 1 })?;
        validate::shard(value, count)?;
        value
    };
    let amount = if matches!(kind, K::Lease | K::Spend) {
        let n = number(options, "amount").ok().flatten().ok_or(E::Amount)?;
        validate::amount_bound(n)?;
        n
    } else {
        0
    };
    let released = if kind == K::Release {
        Some(release(options, shard)?)
    } else {
        None
    };
    let (now, ttl) = times(kind, options)?;
    if let Some(ttl) = ttl {
        validate::deadline(now, ttl)?;
    }
    let action = match kind {
        K::Lease => A::Lease {
            shard_id: shard,
            amount,
            ttl_ms: ttl.ok_or(E::InvalidField("ttl_ms"))?,
            configuration: configuration(options)?,
        },
        K::Spend => {
            if field(options, "reservation_ids").is_some() {
                return Err(E::ReservationIdsNotAllowed);
            }
            A::Spend {
                shard_id: shard,
                amount,
                ttl_ms: ttl,
                configuration: configuration(options)?,
            }
        }
        K::Renew => A::Renew {
            shard_id: shard,
            ttl_ms: ttl.ok_or(E::InvalidField("ttl_ms"))?,
        },
        K::Release => released.ok_or(E::ReservationIds)?,
        K::Get => A::Get,
        K::Cleanup => A::Cleanup,
    };
    Ok(NormalizedCreditCall {
        scope,
        shard_count: count,
        now_ms: now,
        action,
    })
}
fn times(kind: K, options: &[(&str, V<'_>)]) -> Result<(Option<u64>, Option<u64>), E> {
    if matches!(kind, K::Lease | K::Renew) {
        let ttl = positive(options, "ttl_ms")?;
        return Ok((number(options, "now_ms")?, Some(ttl)));
    }
    let now = number(options, "now_ms")?;
    let ttl = if kind == K::Spend {
        match field(options, "ttl_ms") {
            None | Some(V::Nil) => None,
            _ => Some(positive(options, "ttl_ms")?),
        }
    } else {
        None
    };
    Ok((now, ttl))
}
fn measure(scope: &[u8], options: &[(&str, V<'_>)], maximum: usize) -> Result<(), E> {
    if maximum == 0 {
        return Err(E::InvalidLimits);
    }
    if options.len() > 16 {
        return Err(E::InvalidOptions);
    }
    let mut size = scope.len().checked_add(16).ok_or(E::Limit)?;
    for (key, value) in options {
        size = size
            .checked_add(key.len())
            .and_then(|n| n.checked_add(1))
            .ok_or(E::Limit)?;
        let bytes = match value {
            V::Nil | V::Unsupported => 0,
            V::Integer(_) => 16,
            V::Bytes(bytes) => bytes.len(),
            V::Ids(ids) => {
                if ids.len() > 1000 {
                    return Err(E::Limit);
                }
                let slots = ids
                    .len()
                    .checked_mul(std::mem::size_of::<&[u8]>())
                    .ok_or(E::Limit)?;
                ids.iter()
                    .try_fold(slots, |n, id| n.checked_add(id.len()))
                    .ok_or(E::Limit)?
            }
        };
        size = size.checked_add(bytes).ok_or(E::Limit)?;
        if size > maximum {
            return Err(E::Limit);
        }
    }
    if size > maximum {
        Err(E::Limit)
    } else {
        Ok(())
    }
}
fn field<'a>(options: &[(&str, V<'a>)], name: &str) -> Option<V<'a>> {
    options
        .iter()
        .find_map(|(key, value)| (*key == name).then_some(*value))
}
fn number(options: &[(&str, V<'_>)], name: &'static str) -> Result<Option<u64>, E> {
    match field(options, name) {
        None => Ok(None),
        Some(V::Integer(n)) => u64::try_from(n)
            .ok()
            .filter(|n| *n <= MAX)
            .map(Some)
            .ok_or(E::InvalidField(name)),
        _ => Err(E::InvalidField(name)),
    }
}
fn positive(options: &[(&str, V<'_>)], name: &'static str) -> Result<u64, E> {
    number(options, name)?
        .filter(|n| *n > 0)
        .ok_or(E::InvalidField(name))
}
fn configuration<'a>(options: &[(&str, V<'a>)]) -> Result<Option<C<'a>>, E> {
    let limit = number(options, "limit")?;
    let policy_version = match field(options, "policy_version") {
        None => None,
        Some(V::Integer(n)) => Some(P::Number(
            u64::try_from(n)
                .ok()
                .filter(|n| *n <= MAX)
                .ok_or(E::PolicyVersion)?,
        )),
        Some(V::Bytes(bytes)) if !bytes.is_empty() && bytes.len() <= 262_144 => {
            Some(P::Bytes(bytes))
        }
        _ => return Err(E::PolicyVersion),
    };
    let config_version = match (number(options, "config_version")?, policy_version) {
        (Some(n), _) | (None, Some(P::Number(n))) => Some(n),
        (None, Some(P::Bytes(_))) => return Err(E::PolicyVersionRequiresConfig),
        (None, None) => None,
    };
    match limit {
        Some(limit) => Ok(Some(C {
            limit,
            config_version,
            policy_version,
        })),
        None if config_version.is_some() || policy_version.is_some() => Err(E::LimitRequired),
        None => Ok(None),
    }
}
fn release<'a>(options: &[(&str, V<'a>)], shard_id: u64) -> Result<A<'a>, E> {
    let Some(V::Ids(reservation_ids)) = field(options, "reservation_ids") else {
        return Err(E::ReservationIds);
    };
    validate::ids(reservation_ids)?;
    let amount = match number(options, "amount") {
        Ok(Some(n)) if usize::try_from(n).ok() == Some(reservation_ids.len()) => n,
        Ok(None) => u64::try_from(reservation_ids.len()).map_err(|_| E::ReservationIds)?,
        _ => return Err(E::ReservationIds),
    };
    Ok(A::Release {
        shard_id,
        amount,
        reservation_ids,
    })
}
