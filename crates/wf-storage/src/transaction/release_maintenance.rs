//! Retained-first native maintenance; no remote credit success is invented here.
use super::{
    RecordBatchTransactionError as E, RecordBatchTransactionLimits as L,
    RecordBatchTransactionOutcome as O,
};
use crate::state::{RedbStateStore, RedbStateTransaction, redb::release::maintenance};
use wf_domain::{
    governance::release::{ReleaseMaintenanceReply as R, prepare_release_maintenance},
    workflow::{
        control::{ControlPlan, ControlState},
        request::{
            RequestScope, RetainedMetadata, RetainedOutcome, RetainedStatus,
            gate::{GateDecision, PointGateView, inspect_point_request},
        },
    },
};
use wf_ports::{
    log::{
        PreparedReleaseMaintenanceRequest, RecordBatchApplyError, encode_release_maintenance_reply,
        prepare_release_maintenance_request,
    },
    state::{GroupApplyStore, GroupApplyTransaction, StateTransactionError as S},
};
use wf_types::version::{LogIdentity, TimestampMillis};

/// Applies one trusted native ACK/completion under actual release custody.
/// This operation does not establish that any remote credit release succeeded.
/// # Errors
/// Refuses input/profile/authority/resources or unproved durable completion.
pub fn apply_release_maintenance_transaction(
    store: &mut RedbStateStore,
    scope: &RequestScope,
    encoded: &[u8],
    log: LogIdentity,
    limits: L,
) -> Result<O, E> {
    let available = workspace(limits)?;
    let prepared = prepare_release_maintenance_request(encoded, limits.codec, available)
        .map_err(RecordBatchApplyError::from)?;
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
    tx.require_workflow_release()?;
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
    )
    .map_err(RecordBatchApplyError::from)?;
    let control = gate.control();
    let kind = match gate.decision() {
        GateDecision::Fresh => None,
        GateDecision::Replay(_) => Some(0),
        GateDecision::IntentConflict => Some(1),
        GateDecision::Expired => Some(2),
        GateDecision::LogReplay => Some(3),
    };
    if let Some(kind) = kind {
        maintenance::apply(
            tx,
            scope,
            expected,
            control,
            None,
            None,
            (encoded, prepared.command()),
        )?;
        return Ok(match kind {
            0 => O::Retained {
                control: control.next(),
                outcome: Box::new(original.ok_or(S::Corrupt)?),
            },
            1 => O::IntentConflict(control.next()),
            2 => O::Expired(control.next()),
            _ => O::LogReplay(control.next()),
        });
    }
    drop(original);
    fresh(tx, scope, prepared, expected, control, limits)
}
fn fresh(
    tx: RedbStateTransaction<'_>,
    scope: &RequestScope,
    prepared: PreparedReleaseMaintenanceRequest<'_>,
    expected: ControlState,
    control: ControlPlan,
    limits: L,
) -> Result<O, E> {
    let meta = maintenance::current(&tx, scope)?;
    let encoded = prepared.encoded();
    let (owned_scope, request, intent, command) = prepared.into_retained_parts();
    let plan = prepare_release_maintenance(meta, &command).map_err(|_| S::Corrupt)?;
    let reply = plan.reply();
    let response = encode_release_maintenance_reply(
        reply,
        limits.codec.admission().decision_bytes(),
        limits.maximum_staging_bytes,
    )
    .map_err(RecordBatchApplyError::from)?;
    let expiry = control
        .execution_time()
        .get()
        .checked_add(limits.outcome_ttl_ms)
        .filter(|end| *end <= wf_domain::workflow::MAX_EXACT_MILLIS)
        .ok_or(S::Limit)?;
    let outcome = RetainedOutcome::new(
        RetainedMetadata {
            scope: owned_scope,
            request,
            intent,
            original_position: control.next().last_entry().ok_or(S::InvalidChanges)?.log(),
            original_time: control.execution_time(),
            expires_at: TimestampMillis::new(expiry),
            status: match reply {
                R::Accepted { .. } => RetainedStatus::Accepted,
                R::Rejected { reason, .. } => RetainedStatus::WorkflowReleaseRejected(reason),
            },
        },
        15,
        response,
        limits.codec.admission(),
    )
    .map_err(RecordBatchApplyError::from)?;
    maintenance::apply(
        tx,
        scope,
        expected,
        control,
        Some(&plan),
        Some(&outcome),
        (encoded, &command),
    )?;
    Ok(O::Retained {
        control: control.next(),
        outcome: Box::new(outcome),
    })
}
fn workspace(limits: L) -> Result<usize, E> {
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
    if limits
        .codec
        .admission()
        .decision_bytes()
        .checked_add(4096)
        .ok_or(S::Limit)?
        > limits.maximum_staging_bytes
    {
        return Err(S::Limit.into());
    }
    let reserved = limits
        .maximum_read_bytes
        .checked_add(limits.maximum_staging_bytes)
        .and_then(|n| n.checked_add(limits.maximum_adapter_bytes))
        .ok_or(S::Limit)?;
    limits
        .workspace_bytes
        .checked_sub(reserved)
        .filter(|n| *n > 0)
        .ok_or(S::Limit.into())
}
