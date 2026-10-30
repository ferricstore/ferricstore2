//! Retained-first dispatch, sparse same-writer observations and one atomic commit.

use super::{
    CreditTransactionError as E, RecordBatchTransactionLimits as Limits,
    RecordBatchTransactionOutcome as Outcome,
};
use crate::state::{
    RedbStateStore, RedbStateTransaction,
    redb::credit::{self, CreditReadContext},
};
use wf_domain::governance::credit::CreditLimits;
use wf_domain::governance::identified::{
    IdentifiedCreditDecision as D, prepare_identified_credit, resume_identified_credit,
};
use wf_domain::workflow::control::{ControlPlan, ControlState};
use wf_domain::workflow::request::{
    RequestScope, RetainedMetadata, RetainedOutcome, RetainedStatus,
    gate::{GateDecision, PointGateView, inspect_point_request},
};
use wf_ports::log::{PreparedCreditRequest, encode_credit_reply_parts, prepare_credit_request};
use wf_ports::state::{GroupApplyStore, GroupApplyTransaction, StateTransactionError as S};
use wf_types::{
    identity::{GroupId, RecoveryId, RequestId, TenantId},
    version::{LogIdentity, TimestampMillis},
};

pub(super) fn apply(
    store: &mut RedbStateStore,
    scope: &RequestScope,
    encoded: &[u8],
    log: LogIdentity,
    limits: Limits,
    credit: CreditLimits,
) -> Result<Outcome, E> {
    let (available, credit) = workspace(limits, credit)?;
    let prepared = prepare_credit_request(encoded, limits.codec, available)?;
    let header = prepared.header(log);
    if header.scope != scope {
        return Err(S::InvalidChanges.into());
    }
    let tx = store.begin_apply(
        scope.group(),
        scope.recovery(),
        limits.codec.admission(),
        limits.maximum_read_steps,
        limits.maximum_read_bytes,
        limits.maximum_adapter_bytes,
    )?;
    let expected = tx.control()?;
    let nonempty = tx.retained_nonempty(scope.tenant())?;
    let original = tx.outcome(scope.tenant(), header.request)?;
    let gate = inspect_point_request(
        PointGateView {
            scope,
            control: expected,
            retained_nonempty: nonempty,
            outcome: original.as_ref(),
        },
        header,
        limits.codec.admission(),
    )?;
    let control = gate.control();
    let kind = match gate.decision() {
        GateDecision::Fresh => None,
        GateDecision::Replay(_) => Some(0),
        GateDecision::IntentConflict => Some(1),
        GateDecision::Expired => Some(2),
        GateDecision::LogReplay => Some(3),
    };
    if let Some(kind) = kind {
        credit::commit(tx, scope, expected, control, None, None, credit)?;
        return Ok(match kind {
            0 => Outcome::Retained {
                control: control.next(),
                outcome: Box::new(original.ok_or(S::Corrupt)?),
            },
            1 => Outcome::IntentConflict(control.next()),
            2 => Outcome::Expired(control.next()),
            _ => Outcome::LogReplay(control.next()),
        });
    }
    drop(original);
    fresh(tx, scope, &prepared, expected, control, limits, credit)
}

fn fresh(
    tx: RedbStateTransaction<'_>,
    scope: &RequestScope,
    prepared: &PreparedCreditRequest<'_>,
    expected: ControlState,
    control: ControlPlan,
    limits: Limits,
    credit: CreditLimits,
) -> Result<Outcome, E> {
    let command = prepared.command();
    let context = CreditReadContext::new(&tx, scope, command.scope, credit)?;
    if context.shards != command.shard_count {
        return Err(S::InvalidChanges.into());
    }
    let owner = context.owner()?;
    let mut pending = prepare_identified_credit(owner, command, control.execution_time(), credit)?;
    while let Some(query) = pending.query() {
        let observed = context.observe(query)?;
        pending = resume_identified_credit(pending, observed)?;
    }
    let plan = pending.finish()?;
    let response = encode_credit_reply_parts(
        plan.decision(),
        plan.owner(),
        plan.response_ids(),
        limits.codec.admission().decision_bytes(),
        limits.maximum_staging_bytes,
    )?;
    let log = control.next().last_entry().ok_or(S::InvalidChanges)?.log();
    let header = prepared.header(log);
    let expires = control
        .execution_time()
        .get()
        .checked_add(limits.outcome_ttl_ms)
        .filter(|end| *end <= wf_domain::workflow::MAX_EXACT_MILLIS)
        .ok_or(S::Limit)?;
    let outcome = RetainedOutcome::new(
        RetainedMetadata {
            scope: owned_scope(scope)?,
            request: RequestId::new(copy(header.request.as_bytes())?)
                .map_err(|_| S::InvalidChanges)?,
            intent: header.intent,
            original_position: log,
            original_time: control.execution_time(),
            expires_at: TimestampMillis::new(expires),
            status: match plan.decision() {
                D::Rejected(reason) => RetainedStatus::GovernanceRejected(reason),
                _ => RetainedStatus::Accepted,
            },
        },
        10,
        response,
        limits.codec.admission(),
    )
    .map_err(|_| S::InvalidChanges)?;
    credit::commit(
        tx,
        scope,
        expected,
        control,
        Some(&plan),
        Some(&outcome),
        credit,
    )?;
    Ok(Outcome::Retained {
        control: control.next(),
        outcome: Box::new(outcome),
    })
}

fn workspace(limits: Limits, credit: CreditLimits) -> Result<(usize, CreditLimits), E> {
    if [
        limits.maximum_read_steps,
        limits.maximum_read_bytes,
        limits.maximum_staging_bytes,
        limits.maximum_adapter_bytes,
        limits.workspace_bytes,
    ]
    .contains(&0)
        || limits.outcome_ttl_ms == 0
    {
        return Err(E::InvalidLimits);
    }
    let reserved = limits
        .maximum_read_bytes
        .checked_add(limits.maximum_staging_bytes)
        .and_then(|n| n.checked_add(limits.maximum_adapter_bytes))
        .ok_or(S::Limit)?;
    let available = limits
        .workspace_bytes
        .checked_sub(reserved)
        .ok_or(S::Limit)?;
    let mathematical = limits
        .maximum_staging_bytes
        .checked_sub(limits.codec.admission().decision_bytes())
        .and_then(|n| n.checked_sub(4096))
        .filter(|n| *n != 0)
        .ok_or(S::Limit)?;
    let credit = CreditLimits::new(
        credit.max_leases(),
        credit.max_reservations(),
        credit.max_owned_bytes().min(mathematical),
        credit.max_work(),
    )?;
    Ok((available, credit))
}

fn owned_scope(scope: &RequestScope) -> Result<RequestScope, E> {
    Ok(RequestScope::new(
        TenantId::new(copy(scope.tenant().as_bytes())?).map_err(|_| S::InvalidChanges)?,
        GroupId::new(copy(scope.group().as_bytes())?).map_err(|_| S::InvalidChanges)?,
        RecoveryId::new(copy(scope.recovery().as_bytes())?).map_err(|_| S::InvalidChanges)?,
    ))
}

fn copy(bytes: &[u8]) -> Result<Vec<u8>, E> {
    let mut out = Vec::new();
    out.try_reserve_exact(bytes.len())
        .map_err(|_| S::Allocation)?;
    out.extend_from_slice(bytes);
    Ok(out)
}
