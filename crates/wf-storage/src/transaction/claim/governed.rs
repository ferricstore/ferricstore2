//! Actual paid Claim: original gate first, same-guard payment, atomic publication.
use super::{
    ClaimTransactionError as E, ClaimTransactionLimits, ClaimTransactionOutcome as O,
    complete_kind, fresh_reply, live_bytes, load, observations, values,
};
use crate::{
    state::{RedbStateStore, RedbStateTransaction},
    transaction::WorkflowValueTransactionLimits,
};
use wf_domain::{
    claim::apply::ClaimWorkLimits,
    workflow::{
        batch::ApplyError,
        control::ControlState,
        request::{RequestScope, gate::PointGateView},
    },
};
use wf_ports::{
    log::{
        CapturedGovernedClaimPayment as C, ClaimApplyPlan, ClaimPhaseInspection as I,
        RecordBatchApplyError, decide_fresh_claim_phase_observed, inspect_governed_claim_point,
        prepare_governed_claim_request, reject_unavailable_governed_claim,
    },
    state::{
        GroupApplyStore, GroupApplyTransaction, GroupClaimChanges, StateTransactionError as S,
    },
};
use wf_types::version::LogIdentity;

/// Applies a trusted paid Claim; public policy, authorization and refunds are outer work.
/// # Errors
/// Refuses full input, scope, actual GC5/payment data, resource limits or unproved commit.
pub fn apply_governed_claim_transaction(
    store: &mut RedbStateStore,
    scope: &RequestScope,
    encoded: &[u8],
    log: LogIdentity,
    limits: WorkflowValueTransactionLimits,
    work: ClaimWorkLimits,
) -> Result<O, E> {
    apply_governed_claim_transaction_with_credit_limits(
        store,
        scope,
        encoded,
        log,
        limits,
        work,
        wf_domain::governance::credit::CreditLimits::default(),
    )
}

/// Applies paid Claim with the caller's explicit governing-credit owner/work bounds.
/// # Errors
/// Refuses the same conditions as the default entrypoint and exceeded credit limits.
pub fn apply_governed_claim_transaction_with_credit_limits(
    store: &mut RedbStateStore,
    scope: &RequestScope,
    encoded: &[u8],
    log: LogIdentity,
    limits: WorkflowValueTransactionLimits,
    work: ClaimWorkLimits,
    credit: wf_domain::governance::credit::CreditLimits,
) -> Result<O, E> {
    let resources = values::ClaimValueResources::new(limits, encoded.len())?;
    let base = limits.record_batch;
    let live = super::super::load::sum(&[live_bytes(base)?, limits.maximum_value_temporary_bytes])?;
    let available =
        base.workspace_bytes
            .checked_sub(live)
            .ok_or(RecordBatchApplyError::Workspace {
                attempted: live,
                maximum: base.workspace_bytes,
            })?;
    let prepared = prepare_governed_claim_request(encoded, base.codec, available)
        .map_err(RecordBatchApplyError::from)?;
    if &prepared.decoded().request().scope != scope {
        return Err(RecordBatchApplyError::Apply(ApplyError::InvalidAuthority).into());
    }
    let capture = prepared.capture();
    let tx = store.begin_apply(
        scope.group(),
        scope.recovery(),
        base.codec.admission(),
        base.maximum_read_steps,
        base.maximum_read_bytes,
        base.maximum_adapter_bytes,
    )?;
    tx.require_governed_claim()?;
    let predecessor = tx.control()?;
    let retained_nonempty = tx.retained_nonempty(scope.tenant())?;
    let original = tx.outcome(scope.tenant(), &prepared.decoded().request().request)?;
    let inspection = inspect_governed_claim_point(
        prepared,
        log,
        PointGateView {
            scope,
            control: predecessor,
            outcome: original.as_ref(),
            retained_nonempty,
        },
        base.codec,
        base.workspace_bytes,
        work,
    )?;
    let I::Fresh(fresh) = inspection else {
        let I::Complete(plan) = inspection else {
            return Err(S::Corrupt.into());
        };
        let changes = GroupClaimChanges::new(scope, predecessor, plan, base.codec.admission())?;
        values::commit_governed(tx, &changes, resources, (capture, credit))?;
        let (control, kind) = complete_kind(changes.into_plan())?;
        return Ok(match kind {
            0 => O::Retained {
                control,
                outcome: Box::new(original.ok_or(S::Corrupt)?),
            },
            1 => O::IntentConflict(control),
            2 => O::Expired(control),
            _ => O::LogReplay(control),
        });
    };
    let limits = ClaimTransactionLimits { base, work };
    if fresh.query_preflight().is_err() {
        let plan = decide_fresh_claim_phase_observed(fresh, None, live, base.outcome_ttl_ms)?;
        return commit_fresh(tx, scope, predecessor, plan, resources, (capture, credit));
    }
    if !crate::state::redb::release::claim::available(
        &tx,
        scope,
        capture,
        fresh.control().execution_time(),
        capture.requested_limit(),
        credit,
    )? {
        let plan = reject_unavailable_governed_claim(fresh, live, base.outcome_ttl_ms)?;
        return commit_fresh(tx, scope, predecessor, plan, resources, (capture, credit));
    }
    let mut loaded = load::LoadedClaim::load(&tx, scope, &fresh, limits)?;
    let plan = observations::decide(fresh, scope, &mut loaded, limits, live)?;
    commit_fresh(tx, scope, predecessor, plan, resources, (capture, credit))
}
fn commit_fresh(
    tx: RedbStateTransaction<'_>,
    scope: &RequestScope,
    predecessor: ControlState,
    plan: ClaimApplyPlan<'_>,
    resources: values::ClaimValueResources,
    paid: (C<'_>, wf_domain::governance::credit::CreditLimits),
) -> Result<O, E> {
    let changes = GroupClaimChanges::new(
        scope,
        predecessor,
        plan,
        resources.limits.record_batch.codec.admission(),
    )?;
    values::commit_governed(tx, &changes, resources, paid)?;
    fresh_reply(changes.into_plan())
}
