//! Complete borrowed original response binding, without owning another credit owner.
use super::{ClientIntent, E, PublicCreditReplyExpectation};
use wf_domain::{
    governance::{
        caller::CreditCallKind as K, credit::CreditLimits,
        identified::IdentifiedCreditDecision as D,
    },
    workflow::{admission::AdmissionLimits, request::RetainedStatus},
};
pub(super) fn inspect(
    intent: ClientIntent,
    version: u16,
    response: &[u8],
    expected: PublicCreditReplyExpectation<'_>,
    credit: CreditLimits,
    admission: AdmissionLimits,
) -> Result<RetainedStatus, E> {
    if intent.version() != 17 || version != 10 {
        return Err(E::UnsupportedVersion);
    }
    let scanned = crate::log::credit::response::inspect(
        response,
        credit,
        admission.decision_bytes(),
        credit.max_owned_bytes(),
    )?;
    if scanned
        .owner_scope()
        .is_some_and(|scope| scope != expected.scope)
    {
        return Err(E::Malformed);
    }
    let (decision, count) = scanned.summary();
    if let D::Rejected(reason) = decision {
        return Ok(RetainedStatus::GovernanceRejected(reason));
    }
    let matches = match (expected.kind, decision) {
        (K::Lease, D::Granted { shard_id }) | (K::Renew, D::Renewed { shard_id }) => {
            Some(shard_id) == expected.shard
        }
        (K::Spend, D::Spent { shard_id }) => {
            Some(shard_id) == expected.shard && u64::try_from(count).ok() == expected.amount
        }
        (K::Release, D::Released) | (K::Get, D::Owner) | (K::Cleanup, D::Cleaned { .. }) => true,
        _ => false,
    };
    if !matches {
        return Err(E::Malformed);
    }
    if let Some(ids) = expected.ids
        && !scanned.matches_ids(ids)?
    {
        return Err(E::Malformed);
    }
    Ok(RetainedStatus::Accepted)
}
