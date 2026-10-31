//! Complete caller/EOF/digest validation precedes private ID payload ownership.
use super::{
    ClientIntent, CodecLimits, CommittedTime, CreditPolicyInput, CreditSpendPreparation, E,
    NormalizedCreditCall, PreparedCreditRequest, PreparedDigest, RequestId, RequestScope, encode,
    ids, policy,
};
use crate::log::{
    common::RawHeader,
    credit::model::OwnedCreditMutation as M,
    keys,
    reader::{Reader, copy},
};
use sha2::{Digest, Sha256};
use wf_domain::governance::{
    caller::{CreditCallAction as A, CreditCallConfiguration as C, CreditIdList},
    identified::CreditConfiguration,
};
use wf_types::{
    identity::{GroupId, RecoveryId, TenantId},
    version::HlcTimestamp,
};
pub(super) fn prepare(
    encoded: &[u8],
    limits: CodecLimits,
    maximum_owned: usize,
) -> Result<PreparedCreditRequest<'_>, E> {
    if encoded.len() > limits.admission().decision_bytes() {
        return Err(E::ByteLimit);
    }
    let base = encoded.len().checked_add(4096).ok_or(E::LengthOverflow)?;
    if base > maximum_owned {
        return Err(E::OwnedLimit);
    }
    let mut reader = Reader::new(encoded, limits);
    let header = header(&mut reader)?;
    let credit_scope = reader.nonempty(65_535)?;
    let count = reader.u64()?;
    let tag = reader.byte()?;
    let now = reader.optional(Reader::u64)?;
    let mut ids = Vec::new();
    let action = if tag == 3 {
        let shard_id = reader.u64()?;
        let amount = reader.u64()?;
        let n = usize::try_from(reader.u32()?).map_err(|_| E::LengthOverflow)?;
        if n > 1000 || n > limits.admission().batch_items() {
            return Err(E::ItemLimit);
        }
        admit(base, n, maximum_owned)?;
        ids.try_reserve_exact(n).map_err(|_| E::Allocation)?;
        for _ in 0..n {
            ids.push(reader.nonempty(256)?);
        }
        A::Release {
            shard_id,
            amount,
            reservation_ids: CreditIdList::Borrowed(&ids),
        }
    } else {
        action(&mut reader, tag)?
    };
    let call = NormalizedCreditCall::from_parts(credit_scope, count, now, action)
        .map_err(|_| E::Malformed)?;
    let spend = if tag == 2 {
        Some(CreditSpendPreparation {
            lease_epoch: reader.u64()?,
            batch: reader.fixed()?,
        })
    } else {
        None
    };
    reader.finish()?;
    if encode::intent(call)? != header.intent || now.is_some_and(|n| n > header.stamp.wall().get())
    {
        return Err(E::Malformed);
    }
    NormalizedCreditCall::from_parts(credit_scope, count, Some(header.stamp.wall().get()), action)
        .map_err(|_| E::Malformed)?;
    let n = match action {
        A::Spend { amount, .. } | A::Release { amount, .. } => {
            usize::try_from(amount).map_err(|_| E::LengthOverflow)?
        }
        _ => 0,
    };
    let owned = admit(base, n, maximum_owned)?;
    let mutation = mutation(action, spend, maximum_owned - base)?;
    let configuration = match action {
        A::Lease { configuration, .. } | A::Spend { configuration, .. } => {
            configuration.map(materialize_config).transpose()?
        }
        _ => None,
    };
    let isolation = keys::scope(header.scope())?;
    Ok(PreparedCreditRequest {
        encoded,
        scope: RequestScope::new(
            TenantId::new(copy(header.tenant)?).map_err(|_| E::Malformed)?,
            GroupId::new(copy(header.group)?).map_err(|_| E::Malformed)?,
            RecoveryId::new(copy(header.recovery)?).map_err(|_| E::Malformed)?,
        ),
        request: RequestId::new(copy(header.request)?).map_err(|_| E::Malformed)?,
        stamp: header.stamp,
        intent: header.intent,
        digest: PreparedDigest::new(Sha256::digest(encoded).into()),
        isolation,
        credit_scope: copy(credit_scope)?,
        shard_count: count,
        configuration,
        mutation,
        owned_bytes: owned,
    })
}
fn admit(base: usize, count: usize, maximum: usize) -> Result<usize, E> {
    let slot = std::mem::size_of::<Vec<u8>>() + 2 * std::mem::size_of::<&[u8]>() + 256;
    let owned = count
        .checked_mul(slot)
        .and_then(|n| base.checked_add(n))
        .ok_or(E::LengthOverflow)?;
    if owned > maximum {
        return Err(E::OwnedLimit);
    }
    Ok(owned)
}
fn action<'a>(reader: &mut Reader<'a>, tag: u8) -> Result<A<'a>, E> {
    Ok(match tag {
        1 => A::Lease {
            shard_id: reader.u64()?,
            amount: reader.u64()?,
            ttl_ms: reader.u64()?,
            configuration: reader.optional(config)?,
        },
        2 => A::Spend {
            shard_id: reader.u64()?,
            amount: reader.u64()?,
            ttl_ms: reader.optional(Reader::u64)?,
            configuration: reader.optional(config)?,
        },
        4 => A::Renew {
            shard_id: reader.u64()?,
            ttl_ms: reader.u64()?,
        },
        5 => A::Get,
        6 => A::Cleanup,
        _ => return Err(E::UnsupportedTag),
    })
}
fn config<'a>(reader: &mut Reader<'a>) -> Result<C<'a>, E> {
    let limit = reader.u64()?;
    let config_version = reader.optional(Reader::u64)?;
    let policy_version = match reader.byte()? {
        0 => None,
        1 => Some(CreditPolicyInput::Number(reader.u64()?)),
        2 => Some(CreditPolicyInput::Bytes(reader.nonempty(262_144)?)),
        _ => return Err(E::UnsupportedTag),
    };
    Ok(C {
        limit,
        config_version,
        policy_version,
    })
}
fn materialize_config(value: C<'_>) -> Result<CreditConfiguration, E> {
    Ok(CreditConfiguration {
        limit: value.limit,
        config_version: value.config_version,
        policy_fingerprint: value.policy_version.map(policy::fingerprint).transpose()?,
    })
}
fn mutation(action: A<'_>, spend: Option<CreditSpendPreparation>, maximum: usize) -> Result<M, E> {
    Ok(match action {
        A::Lease {
            shard_id,
            amount,
            ttl_ms,
            ..
        } => M::Lease {
            shard_id,
            amount,
            ttl_ms,
        },
        A::Spend {
            shard_id,
            amount,
            ttl_ms,
            ..
        } => {
            let spend = spend.ok_or(E::Malformed)?;
            M::Spend {
                shard_id,
                lease_epoch: spend.lease_epoch,
                amount,
                ttl_ms,
                ids: ids::generate(amount, spend, maximum)?,
            }
        }
        A::Release {
            shard_id,
            amount,
            reservation_ids,
        } => {
            let mut ids = Vec::new();
            ids.try_reserve_exact(reservation_ids.len())
                .map_err(|_| E::Allocation)?;
            for id in reservation_ids.iter() {
                ids.push(copy(id)?);
            }
            M::Release {
                shard_id,
                amount,
                ids,
            }
        }
        A::Renew { shard_id, ttl_ms } => M::Renew { shard_id, ttl_ms },
        A::Get => M::Get,
        A::Cleanup => M::Cleanup,
    })
}
fn header<'a>(reader: &mut Reader<'a>) -> Result<RawHeader<'a>, E> {
    if reader.take(4)? != b"FWCP" {
        return Err(E::Malformed);
    }
    if reader.u16()? != 1 {
        return Err(E::UnsupportedVersion);
    }
    if reader.byte()? != 15 {
        return Err(E::UnsupportedTag);
    }
    let tenant = reader.nonempty(8192)?;
    let group = reader.nonempty(256)?;
    let recovery = reader.nonempty(256)?;
    let request = reader.nonempty(256)?;
    if reader.u16()? != 17 {
        return Err(E::UnsupportedVersion);
    }
    let intent = ClientIntent::public_credit(reader.fixed()?);
    let hlc = HlcTimestamp::new(reader.u64()?, reader.u16()?).map_err(|_| E::Malformed)?;
    let stamp = CommittedTime::new(hlc, reader.u64()?).map_err(|_| E::Malformed)?;
    Ok(RawHeader {
        operation_tag: 15,
        tenant,
        group,
        recovery,
        request,
        intent,
        stamp,
    })
}
pub(super) fn recovery_header(
    encoded: &[u8],
    limits: CodecLimits,
) -> Result<(u16, [&[u8]; 3], CommittedTime), E> {
    if encoded.len() > limits.admission().decision_bytes() {
        return Err(E::ByteLimit);
    }
    let h = header(&mut Reader::new(encoded, limits))?;
    Ok((17, [h.tenant, h.group, h.recovery], h.stamp))
}
