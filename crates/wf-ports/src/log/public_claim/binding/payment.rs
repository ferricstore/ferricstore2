//! Canonical original source batch validation without copying ID payloads.
use super::{CodecError as E, NormalizedClaimDue, PublicClaimReplyLimits};
use crate::log::{GovernedClaimPaymentRef as Paid, running_claim_scope, writer::Writer};
use wf_domain::{governance::release::LimitEnforcement, retry::source_binary_hash};

pub(super) fn verify(
    paid: Option<Paid<'_>>,
    call: &NormalizedClaimDue<'_>,
    limits: PublicClaimReplyLimits,
    accepted: bool,
) -> Result<(), E> {
    let explicit = (call.governance_limit_scope(), call.governance_shard_id());
    let Some(paid) = paid else {
        return if accepted && explicit != (None, None) {
            Err(E::Malformed)
        } else {
            Ok(())
        };
    };
    if paid.shard_id() >= limits.credit_shard_count
        || u64::try_from(paid.count()).map_err(|_| E::LengthOverflow)? != call.limit()
    {
        return Err(E::Malformed);
    }
    match explicit {
        (Some(scope), Some(shard)) => {
            if paid.scope() != scope
                || paid.shard_id() != shard
                || paid.enforcement() != LimitEnforcement::ApproximateGlobal
            {
                return Err(E::Malformed);
            }
        }
        (None, None) => {
            if paid.scope() != running_claim_scope(call.workflow_type())
                || paid.shard_id()
                    != u64::from(source_binary_hash(call.worker())) % limits.credit_shard_count
            {
                return Err(E::Malformed);
            }
        }
        _ => return Err(E::Malformed),
    }
    ids(paid)
}
fn ids(paid: Paid<'_>) -> Result<(), E> {
    let mut digits = [0; 20];
    let epoch = decimal(paid.epoch(), &mut digits)?;
    let offset = 6_usize.checked_add(epoch.len()).ok_or(E::LengthOverflow)?;
    let first = paid.reservation_ids().next().ok_or(E::Malformed)??;
    let nonce = first.get(offset..offset + 22).ok_or(E::Malformed)?;
    if !nonce
        .iter()
        .all(|b| b.is_ascii_alphanumeric() || matches!(*b, b'-' | b'_'))
        || !matches!(nonce[21], b'A' | b'Q' | b'g' | b'w')
    {
        return Err(E::Malformed);
    }
    for (at, id) in paid.reservation_ids().enumerate() {
        let id = id?;
        let mut number = [0; 20];
        let ordinal = u64::try_from(at)
            .map_err(|_| E::LengthOverflow)?
            .checked_add(1)
            .ok_or(E::LengthOverflow)?;
        let mut out = Writer::verify(id, 256)?;
        for part in [
            b"flr1:".as_slice(),
            epoch,
            b":",
            nonce,
            b":",
            decimal(ordinal, &mut number)?,
        ] {
            out.write(part)?;
        }
        out.finish_verification()?;
    }
    Ok(())
}
fn decimal(mut number: u64, buffer: &mut [u8; 20]) -> Result<&[u8], E> {
    let mut at = buffer.len();
    loop {
        at = at.checked_sub(1).ok_or(E::LengthOverflow)?;
        buffer[at] = b'0' + u8::try_from(number % 10).map_err(|_| E::LengthOverflow)?;
        number /= 10;
        if number == 0 {
            return Ok(&buffer[at..]);
        }
    }
}
