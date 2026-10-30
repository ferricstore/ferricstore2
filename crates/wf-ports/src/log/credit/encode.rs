//! One measured final frame; intent and prepared digests hash actual bytes.

use super::{CodecError as E, CodecLimits, PreparedBytes, admission};
use crate::log::writer::Writer;
use sha2::{Digest, Sha256};
use wf_domain::governance::identified::{
    CreditConfiguration, IdentifiedCreditMutation as Op, IdentifiedCreditRequest,
};
use wf_domain::workflow::control::PreparedDigest;
use wf_domain::workflow::request::RequestScope;
use wf_types::identity::RequestId;
use wf_types::version::CommittedTime;

pub(super) fn request(
    scope: &RequestScope,
    request: &RequestId,
    stamp: CommittedTime,
    command: IdentifiedCreditRequest<'_>,
    limits: CodecLimits,
    maximum_owned: usize,
) -> Result<PreparedBytes, E> {
    crate::log::keys::scope_length([
        scope.tenant().as_bytes(),
        scope.group().as_bytes(),
        scope.recovery().as_bytes(),
    ])?;
    admission::validate(command, stamp.wall().get(), limits, maximum_owned)?;
    let maximum = limits.admission().decision_bytes();
    let mut measure = Writer::measure(maximum);
    header(&mut measure, scope, request, stamp)?;
    let body_at = measure.len();
    body(&mut measure, command)?;
    let length = measure.len();
    if length.checked_add(2048).ok_or(E::LengthOverflow)? > maximum_owned {
        return Err(E::OwnedLimit);
    }
    let mut output = Writer::allocate(length, maximum)?;
    header(&mut output, scope, request, stamp)?;
    body(&mut output, command)?;
    let mut bytes = output.finish()?;
    let mut intent = Sha256::new();
    intent.update(b"wf-governing-credit-intent-v1\0");
    intent.update(bytes.get(body_at..).ok_or(E::Malformed)?);
    let digest = intent.finalize();
    let intent_at = body_at.checked_sub(50).ok_or(E::Malformed)?;
    bytes
        .get_mut(intent_at..intent_at + 32)
        .ok_or(E::Malformed)?
        .copy_from_slice(&digest);
    Ok(PreparedBytes {
        digest: PreparedDigest::new(Sha256::digest(&bytes).into()),
        bytes,
    })
}

fn header(
    out: &mut Writer<'_>,
    scope: &RequestScope,
    request: &RequestId,
    stamp: CommittedTime,
) -> Result<(), E> {
    out.write(b"FWCP\0\x01\x0a")?;
    out.raw(scope.tenant().as_bytes(), 8192)?;
    for field in [
        scope.group().as_bytes(),
        scope.recovery().as_bytes(),
        request.as_bytes(),
    ] {
        out.raw(field, 256)?;
    }
    out.u16(12)?;
    out.write(&[0; 32])?;
    out.u64(stamp.hlc().physical().get())?;
    out.u16(stamp.hlc().logical())?;
    out.u64(stamp.wall().get())
}

pub(super) fn body(out: &mut Writer<'_>, command: IdentifiedCreditRequest<'_>) -> Result<(), E> {
    out.raw(command.scope, 65_535)?;
    out.u64(command.shard_count)?;
    out.optional(command.configuration.as_ref(), configuration)?;
    mutation(out, command.mutation)
}

fn configuration(out: &mut Writer<'_>, value: &CreditConfiguration) -> Result<(), E> {
    out.u64(value.limit)?;
    out.optional(value.config_version.as_ref(), |out, version| {
        out.u64(*version)
    })?;
    out.optional(value.policy_fingerprint.as_ref(), |out, digest| {
        out.write(digest.as_bytes())
    })
}

fn mutation(out: &mut Writer<'_>, mutation: Op<'_>) -> Result<(), E> {
    match mutation {
        Op::Lease {
            shard_id,
            amount,
            ttl_ms,
        } => {
            out.byte(1)?;
            for value in [shard_id, amount, ttl_ms] {
                out.u64(value)?;
            }
        }
        Op::Spend {
            shard_id,
            lease_epoch,
            amount,
            ttl_ms,
            reservation_ids,
        } => {
            out.byte(2)?;
            for value in [shard_id, lease_epoch, amount] {
                out.u64(value)?;
            }
            out.optional(ttl_ms.as_ref(), |out, ttl| out.u64(*ttl))?;
            ids(out, reservation_ids)?;
        }
        Op::Release {
            shard_id,
            amount,
            reservation_ids,
        } => {
            out.byte(3)?;
            out.u64(shard_id)?;
            out.u64(amount)?;
            ids(out, reservation_ids)?;
        }
        Op::Renew { shard_id, ttl_ms } => {
            out.byte(4)?;
            out.u64(shard_id)?;
            out.u64(ttl_ms)?;
        }
        Op::Get => out.byte(5)?,
        Op::Cleanup => out.byte(6)?,
    }
    Ok(())
}

fn ids(out: &mut Writer<'_>, values: &[Vec<u8>]) -> Result<(), E> {
    out.u32(u32::try_from(values.len()).map_err(|_| E::LengthOverflow)?)?;
    for value in values {
        out.raw(value, 256)?;
    }
    Ok(())
}
