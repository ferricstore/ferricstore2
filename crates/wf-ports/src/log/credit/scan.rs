//! Full borrowed framing and original body digest before payload ownership.

use super::{
    admission,
    model::{OwnedCreditMutation as Op, ScannedCreditRequest},
};
use crate::log::{CodecError as E, CodecLimits, common::RawHeader, keys, reader::Reader};
use sha2::{Digest, Sha256};
use wf_domain::{
    governance::identified::{CreditConfiguration, IdentifiedCreditRequest},
    workflow::{control::PreparedDigest, request::ClientIntent},
};
use wf_types::version::{CommittedTime, HlcTimestamp};

pub(super) fn inspect(
    encoded: &[u8],
    limits: CodecLimits,
    maximum_owned: usize,
) -> Result<ScannedCreditRequest<'_>, E> {
    if encoded.len() > limits.admission().decision_bytes() {
        return Err(E::ByteLimit);
    }
    let base = encoded
        .len()
        .checked_mul(4)
        .and_then(|n| n.checked_add(4096))
        .ok_or(E::LengthOverflow)?;
    if base > maximum_owned {
        return Err(E::OwnedLimit);
    }
    let mut reader = Reader::new(encoded, limits);
    let header = header(&mut reader)?;
    keys::scope_length(header.scope())?;
    let body_at = encoded.len() - reader.remaining();
    let scope = reader.nonempty(65_535)?;
    let shard_count = reader.u64()?;
    let configuration = reader.optional(|reader| {
        Ok(CreditConfiguration {
            limit: reader.u64()?,
            config_version: reader.optional(Reader::u64)?,
            policy_fingerprint: reader
                .optional(|reader| Ok(PreparedDigest::new(reader.fixed()?)))?,
        })
    })?;
    let (mutation, ids) = mutation(&mut reader)?;
    reader.finish()?;
    let owned_bytes = ids
        .len()
        .checked_mul(64)
        .and_then(|n| base.checked_add(n))
        .ok_or(E::LengthOverflow)?;
    if owned_bytes > maximum_owned {
        return Err(E::OwnedLimit);
    }
    admission::metadata(
        IdentifiedCreditRequest {
            scope,
            shard_count,
            configuration,
            mutation: mutation.borrowed(),
        },
        header.stamp.wall().get(),
    )?;
    if let Some(amount) = mutation.amount() {
        admission::borrowed_ids(ids.iter().copied(), amount, limits, maximum_owned)?;
    }
    let mut hash = Sha256::new();
    hash.update(b"wf-governing-credit-intent-v1\0");
    hash.update(&encoded[body_at..]);
    if hash.finalize().as_slice() != header.intent.digest() {
        return Err(E::Malformed);
    }
    Ok(ScannedCreditRequest {
        header,
        scope,
        shard_count,
        configuration,
        mutation,
        ids,
        body_at,
        owned_bytes,
    })
}

fn header<'a>(reader: &mut Reader<'a>) -> Result<RawHeader<'a>, E> {
    if reader.take(4)? != b"FWCP" {
        return Err(E::Malformed);
    }
    if reader.u16()? != 1 {
        return Err(E::UnsupportedVersion);
    }
    if reader.byte()? != 10 {
        return Err(E::UnsupportedTag);
    }
    let tenant = reader.nonempty(8192)?;
    let group = reader.nonempty(256)?;
    let recovery = reader.nonempty(256)?;
    let request = reader.nonempty(256)?;
    if reader.u16()? != 12 {
        return Err(E::UnsupportedVersion);
    }
    let intent = ClientIntent::governance_credit(reader.fixed()?);
    let hlc = HlcTimestamp::new(reader.u64()?, reader.u16()?).map_err(|_| E::Malformed)?;
    let stamp = CommittedTime::new(hlc, reader.u64()?).map_err(|_| E::Malformed)?;
    Ok(RawHeader {
        operation_tag: 10,
        tenant,
        group,
        recovery,
        request,
        intent,
        stamp,
    })
}

fn mutation<'a>(reader: &mut Reader<'a>) -> Result<(Op, Vec<&'a [u8]>), E> {
    Ok(match reader.byte()? {
        1 => (
            Op::Lease {
                shard_id: reader.u64()?,
                amount: reader.u64()?,
                ttl_ms: reader.u64()?,
            },
            Vec::new(),
        ),
        2 => {
            let operation = Op::Spend {
                shard_id: reader.u64()?,
                lease_epoch: reader.u64()?,
                amount: reader.u64()?,
                ttl_ms: reader.optional(Reader::u64)?,
                ids: Vec::new(),
            };
            (operation, ids(reader)?)
        }
        3 => {
            let operation = Op::Release {
                shard_id: reader.u64()?,
                amount: reader.u64()?,
                ids: Vec::new(),
            };
            (operation, ids(reader)?)
        }
        4 => (
            Op::Renew {
                shard_id: reader.u64()?,
                ttl_ms: reader.u64()?,
            },
            Vec::new(),
        ),
        5 => (Op::Get, Vec::new()),
        6 => (Op::Cleanup, Vec::new()),
        _ => return Err(E::UnsupportedTag),
    })
}

fn ids<'a>(reader: &mut Reader<'a>) -> Result<Vec<&'a [u8]>, E> {
    let count = usize::try_from(reader.u32()?).map_err(|_| E::LengthOverflow)?;
    if count == 0 || count > 1000 || count > reader.remaining() / 5 {
        return Err(E::Malformed);
    }
    if count > reader.limits.admission().batch_items() {
        return Err(E::ItemLimit);
    }
    let mut values = Vec::new();
    values.try_reserve_exact(count).map_err(|_| E::Allocation)?;
    for _ in 0..count {
        values.push(reader.nonempty(256)?);
    }
    Ok(values)
}
