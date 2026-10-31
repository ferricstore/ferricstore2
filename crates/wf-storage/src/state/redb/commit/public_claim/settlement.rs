//! Refund only original IDs absent from actual accepted postimage bindings.
use super::{Limits, funding::FundedClaimPayment};
use crate::state::redb::{credit::stage_claim_credit, store::RedbStateTransaction};
use wf_domain::governance::identified::{
    IdentifiedCreditDecision as D, IdentifiedCreditMutation as M, IdentifiedCreditRequest,
};
use wf_ports::{log::FreshAtomicClaim, state::StateTransactionError as E};

pub(super) fn release(
    tx: &RedbStateTransaction<'_>,
    fresh: &FreshAtomicClaim<'_, '_>,
    mut paid: FundedClaimPayment,
    used: &[u16],
    limits: Limits,
    work: &mut u64,
) -> Result<(), E> {
    if used.windows(2).any(|p| p[0] >= p[1])
        || used
            .last()
            .is_some_and(|n| usize::from(*n) >= paid.ids.len())
    {
        return Err(E::InvalidChanges);
    }
    let mut ordinal = 0_usize;
    paid.ids.retain(|_| {
        let keep = u16::try_from(ordinal).is_ok_and(|at| used.binary_search(&at).is_err());
        ordinal += 1;
        keep
    });
    if paid.ids.is_empty() {
        return Ok(());
    }
    let decision = stage_claim_credit(
        tx,
        fresh,
        IdentifiedCreditRequest {
            scope: &paid.scope,
            shard_count: paid.shards,
            configuration: None,
            mutation: M::Release {
                shard_id: paid.shard,
                amount: u64::try_from(paid.ids.len()).map_err(|_| E::Limit)?,
                reservation_ids: &paid.ids,
            },
        },
        limits.base.credit,
        (work, limits.maximum_credit_work),
    )?;
    if decision != D::Released {
        return Err(E::KnownAborted);
    }
    Ok(())
}
