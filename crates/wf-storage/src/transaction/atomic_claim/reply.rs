//! Complete original replay or a rejection after real prefix rollback.
use super::{
    AtomicClaimTransactionLimits, ClaimTransactionError, ClaimTransactionOutcome,
    GroupApplyTransaction, RedbStateStore, RequestScope, S, begin,
};
use crate::state::{
    RedbStateTransaction,
    redb::{StagedAtomicClaim, complete_atomic_claim},
};
use wf_domain::workflow::{
    control::ControlState,
    request::RetainedOutcome,
    request::gate::{GateDecision, RequestGate},
};
use wf_ports::log::{FreshAtomicClaim, PreparedAtomicClaim};

pub(in crate::transaction) fn complete(
    tx: RedbStateTransaction<'_>,
    expected: ControlState,
    prepared: &PreparedAtomicClaim<'_>,
    gate: RequestGate<'_>,
) -> Result<(ControlState, u8), ClaimTransactionError> {
    let kind = match gate.decision() {
        GateDecision::Replay(_) => 0,
        GateDecision::IntentConflict => 1,
        GateDecision::Expired => 2,
        GateDecision::LogReplay => 3,
        GateDecision::Fresh => return Err(S::InvalidChanges.into()),
    };
    let control = complete_atomic_claim(tx, expected, prepared, gate)?;
    Ok((control, kind))
}

pub(in crate::transaction) fn classify(
    control: ControlState,
    kind: u8,
    original: Option<RetainedOutcome>,
) -> Result<ClaimTransactionOutcome, ClaimTransactionError> {
    Ok(match kind {
        0 => ClaimTransactionOutcome::Retained {
            control,
            outcome: Box::new(original.ok_or(S::Corrupt)?),
        },
        1 => ClaimTransactionOutcome::IntentConflict(control),
        2 => ClaimTransactionOutcome::Expired(control),
        _ => ClaimTransactionOutcome::LogReplay(control),
    })
}

pub(super) fn rejected(
    store: &mut RedbStateStore,
    scope: &RequestScope,
    expected: ControlState,
    fresh: &FreshAtomicClaim<'_, '_>,
    limits: AtomicClaimTransactionLimits,
    reply_owned: usize,
    (reason, usage, schema): (
        wf_domain::claim::apply::ClaimRejection,
        (usize, usize, usize),
        &mut wf_domain::policy::complete::PolicySchemaCharge,
    ),
) -> Result<ClaimTransactionOutcome, ClaimTransactionError> {
    let mut base = limits.values.record_batch;
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
    // The old guard was actually aborted. This new guard must observe the exact
    // same predecessor and absent request; no second Fresh inspection occurs.
    if tx.control()? != expected
        || tx
            .outcome(scope.tenant(), fresh.prepared().request())?
            .is_some()
    {
        return Err(S::KnownAborted.into());
    }
    let staged = StagedAtomicClaim::new(tx, expected, fresh, reply_owned, limits, schema)?;
    let (control, outcome) = staged.finish(base.outcome_ttl_ms, Some(reason), schema)?;
    Ok(ClaimTransactionOutcome::Retained { control, outcome })
}
