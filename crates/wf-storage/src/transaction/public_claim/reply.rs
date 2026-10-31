//! Real prefix abort followed by deterministic funding and full original-ID refund.
use super::{
    ClaimTransactionError, ClaimTransactionOutcome, PublicClaimTransactionLimits as Limits,
    RedbStateStore, RequestScope, S, begin,
};
use wf_domain::{
    claim::apply::ClaimRejection, policy::complete::PolicySchemaCharge,
    workflow::control::ControlState,
};
use wf_ports::{log::FreshAtomicClaim, state::GroupApplyTransaction};

pub(super) fn rejected(
    store: &mut RedbStateStore,
    scope: &RequestScope,
    expected: ControlState,
    fresh: &FreshAtomicClaim<'_, '_>,
    limits: Limits,
    reply_owned: usize,
    (reason, usage, schema, work): (
        ClaimRejection,
        (usize, usize, usize),
        &mut PolicySchemaCharge,
        &mut u64,
    ),
) -> Result<ClaimTransactionOutcome, ClaimTransactionError> {
    let mut base = limits.base.values.record_batch;
    base.maximum_read_steps = base
        .maximum_read_steps
        .checked_sub(usage.0)
        .ok_or(S::Limit)?;
    base.maximum_read_bytes = base
        .maximum_read_bytes
        .checked_sub(usage.1)
        .ok_or(S::Limit)?;
    base.maximum_adapter_bytes = base
        .maximum_adapter_bytes
        .checked_sub(usage.2)
        .ok_or(S::Limit)?;
    let tx = begin(store, scope, base)?;
    if tx.control()? != expected
        || tx
            .outcome(scope.tenant(), fresh.prepared().request())?
            .is_some()
    {
        return Err(S::KnownAborted.into());
    }
    let (staged, funding) = crate::state::redb::start_public_claim(
        tx,
        expected,
        fresh,
        reply_owned,
        limits,
        schema,
        work,
    )?;
    if matches!(funding, crate::state::redb::FundingResult::Rejected(_)) {
        return Err(S::KnownAborted.into());
    }
    let (control, outcome) = crate::state::redb::finish_public_claim(
        staged,
        fresh,
        funding,
        limits,
        Some(reason),
        work,
    )?;
    Ok(ClaimTransactionOutcome::Retained { control, outcome })
}
