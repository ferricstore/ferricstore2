//! One caller byte traversal serves hashing, measuring and final writing.
use super::{
    ClientIntent, CodecLimits, CommittedTime, CreditPolicyInput, CreditSpendPreparation, E,
    NormalizedCreditCall, PreparedBytes, PreparedDigest, RequestId, RequestScope,
};
use crate::log::{keys, writer::Writer};
use sha2::{Digest, Sha256};
use wf_domain::governance::caller::{CreditCallAction as A, CreditCallConfiguration as C};
const PREFIX: &[u8] = b"wf-public-credit-intent-v1\0";
pub(super) fn intent(call: NormalizedCreditCall<'_>) -> Result<ClientIntent, E> {
    let mut hash = Sha256::new();
    hash.update(PREFIX);
    caller(call, &mut |bytes| {
        hash.update(bytes);
        Ok(())
    })?;
    Ok(ClientIntent::public_credit(hash.finalize().into()))
}
pub(super) fn request(
    scope: &RequestScope,
    id: &RequestId,
    call: NormalizedCreditCall<'_>,
    stamp: CommittedTime,
    spend: Option<CreditSpendPreparation>,
    limits: CodecLimits,
    maximum_owned: usize,
) -> Result<PreparedBytes, E> {
    if call.now_ms().is_some_and(|now| now > stamp.wall().get()) {
        return Err(E::Malformed);
    }
    NormalizedCreditCall::from_parts(
        call.scope(),
        call.shard_count(),
        Some(stamp.wall().get()),
        call.action(),
    )
    .map_err(|_| E::Malformed)?;
    if matches!(call.action(), A::Spend { .. }) != spend.is_some()
        || spend.is_some_and(|v| v.lease_epoch > wf_domain::workflow::MAX_EXACT_MILLIS)
    {
        return Err(E::Malformed);
    }
    keys::scope_length([
        scope.tenant().as_bytes(),
        scope.group().as_bytes(),
        scope.recovery().as_bytes(),
    ])?;
    let intent = intent(call)?;
    let mut measure = Writer::measure(limits.admission().decision_bytes());
    header(&mut measure, scope, id, stamp, intent)?;
    body(&mut measure, call, spend)?;
    if measure.len().checked_add(4096).ok_or(E::LengthOverflow)? > maximum_owned {
        return Err(E::OwnedLimit);
    }
    let mut out = Writer::allocate(measure.len(), limits.admission().decision_bytes())?;
    header(&mut out, scope, id, stamp, intent)?;
    body(&mut out, call, spend)?;
    let bytes = out.finish()?;
    Ok(PreparedBytes {
        digest: PreparedDigest::new(Sha256::digest(&bytes).into()),
        bytes,
    })
}
fn header(
    out: &mut Writer<'_>,
    scope: &RequestScope,
    id: &RequestId,
    stamp: CommittedTime,
    intent: ClientIntent,
) -> Result<(), E> {
    out.write(b"FWCP\0\x01\x0f")?;
    out.raw(scope.tenant().as_bytes(), 8192)?;
    for value in [
        scope.group().as_bytes(),
        scope.recovery().as_bytes(),
        id.as_bytes(),
    ] {
        out.raw(value, 256)?;
    }
    out.u16(17)?;
    out.write(intent.digest())?;
    out.u64(stamp.hlc().physical().get())?;
    out.u16(stamp.hlc().logical())?;
    out.u64(stamp.wall().get())
}
fn body(
    out: &mut Writer<'_>,
    call: NormalizedCreditCall<'_>,
    spend: Option<CreditSpendPreparation>,
) -> Result<(), E> {
    caller(call, &mut |bytes| out.write(bytes))?;
    if let Some(spend) = spend {
        out.u64(spend.lease_epoch)?;
        out.write(&spend.batch)?;
    }
    Ok(())
}
fn raw(write: &mut impl FnMut(&[u8]) -> Result<(), E>, bytes: &[u8]) -> Result<(), E> {
    write(
        &u32::try_from(bytes.len())
            .map_err(|_| E::LengthOverflow)?
            .to_be_bytes(),
    )?;
    write(bytes)
}
fn optional(write: &mut impl FnMut(&[u8]) -> Result<(), E>, n: Option<u64>) -> Result<(), E> {
    write(&[u8::from(n.is_some())])?;
    if let Some(n) = n {
        write(&n.to_be_bytes())?;
    }
    Ok(())
}
fn caller(
    call: NormalizedCreditCall<'_>,
    write: &mut impl FnMut(&[u8]) -> Result<(), E>,
) -> Result<(), E> {
    raw(write, call.scope())?;
    write(&call.shard_count().to_be_bytes())?;
    let tag = match call.action() {
        A::Lease { .. } => 1,
        A::Spend { .. } => 2,
        A::Release { .. } => 3,
        A::Renew { .. } => 4,
        A::Get => 5,
        A::Cleanup => 6,
    };
    write(&[tag])?;
    optional(write, call.now_ms())?;
    match call.action() {
        A::Lease {
            shard_id,
            amount,
            ttl_ms,
            configuration,
        } => {
            for n in [shard_id, amount, ttl_ms] {
                write(&n.to_be_bytes())?;
            }
            config(write, configuration)
        }
        A::Spend {
            shard_id,
            amount,
            ttl_ms,
            configuration,
        } => {
            write(&shard_id.to_be_bytes())?;
            write(&amount.to_be_bytes())?;
            optional(write, ttl_ms)?;
            config(write, configuration)
        }
        A::Release {
            shard_id,
            amount,
            reservation_ids,
        } => {
            write(&shard_id.to_be_bytes())?;
            write(&amount.to_be_bytes())?;
            write(
                &u32::try_from(reservation_ids.len())
                    .map_err(|_| E::LengthOverflow)?
                    .to_be_bytes(),
            )?;
            for id in reservation_ids.iter() {
                raw(write, id)?;
            }
            Ok(())
        }
        A::Renew { shard_id, ttl_ms } => {
            write(&shard_id.to_be_bytes())?;
            write(&ttl_ms.to_be_bytes())
        }
        A::Get | A::Cleanup => Ok(()),
    }
}
fn config(write: &mut impl FnMut(&[u8]) -> Result<(), E>, value: Option<C<'_>>) -> Result<(), E> {
    write(&[u8::from(value.is_some())])?;
    let Some(value) = value else {
        return Ok(());
    };
    write(&value.limit.to_be_bytes())?;
    optional(write, value.config_version)?;
    match value.policy_version {
        None => write(&[0]),
        Some(CreditPolicyInput::Number(n)) => {
            write(&[1])?;
            write(&n.to_be_bytes())
        }
        Some(CreditPolicyInput::Bytes(bytes)) => {
            write(&[2])?;
            raw(write, bytes)
        }
    }
}
