//! Complete original caller binding, distinct from current execution authority.
use super::{CodecError, NormalizedClaimDue, PublicClaimReplyLimits, PublicClaimReplyRef};
use crate::log::{CodecLimits, reader::Reader};
use wf_domain::{
    claim::coordination::{
        ClaimPhase, ClaimPhaseCompletion, advance_claim_schedule, plan_claim_schedule,
    },
    workflow::request::RetainedStatus,
};
use wf_types::version::TimestampMillis;
mod payment;
mod records;
/// Scans the full receipt, then compares the original caller and source ID grammar.
/// Earlier original winners may have a different preparation nonce/time.
/// # Errors
/// Refuses malformed data, changed caller/payment/schedule or current bounds.
pub fn inspect_public_claim_reply_for_call<'a>(
    response: &'a [u8],
    time: TimestampMillis,
    caller: &NormalizedClaimDue<'_>,
    limits: PublicClaimReplyLimits,
) -> Result<PublicClaimReplyRef<'a>, CodecError> {
    let reply = super::inspect_public_claim_reply(response, time, limits.admission)?;
    if !(1..=1_u64 << 32).contains(&limits.credit_shard_count) {
        return Err(CodecError::Malformed);
    }
    let Some(claim) = reply.claim() else {
        return Ok(reply);
    };
    payment::verify(
        claim.payment(),
        caller,
        limits,
        claim.status() == RetainedStatus::Accepted,
    )?;
    if claim.status() != RetainedStatus::Accepted {
        return Ok(reply);
    }
    if u64::try_from(claim.count()).map_err(|_| CodecError::LengthOverflow)? > caller.limit() {
        return Err(CodecError::Malformed);
    }
    let mut schedule =
        plan_claim_schedule(caller, limits.coordination).map_err(|_| CodecError::Malformed)?;
    let mut phases = Reader::new(claim.phases(), CodecLimits::new(limits.admission));
    for _ in 0..claim.phase_count() {
        let phase = schedule
            .phase()
            .map_err(|_| CodecError::Malformed)?
            .ok_or(CodecError::Malformed)?;
        let (index, kind, quota) = (phase.index(), phase.kind(), phase.quota());
        let expected = match kind {
            ClaimPhase::Requested => 0,
            ClaimPhase::Normal => 1,
            ClaimPhase::Running => 2,
        };
        if phases.byte()? != expected {
            return Err(CodecError::Malformed);
        }
        let body = phases.raw(limits.admission.decision_bytes())?;
        let count = records::verify(body, time, caller, kind, limits.admission)?;
        if count > quota {
            return Err(CodecError::Malformed);
        }
        advance_claim_schedule(
            &mut schedule,
            ClaimPhaseCompletion::Accepted {
                phase: index,
                claimed: count,
            },
        )
        .map_err(|_| CodecError::Malformed)?;
    }
    phases.finish()?;
    if schedule
        .phase()
        .map_err(|_| CodecError::Malformed)?
        .is_some()
    {
        return Err(CodecError::Malformed);
    }
    Ok(reply)
}
