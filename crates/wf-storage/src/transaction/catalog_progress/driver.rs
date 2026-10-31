//! Retained-first progress dispatch; original header owners move without clones.

use super::{
    CatalogProgressTransactionError as E, RecordBatchTransactionLimits as Limits,
    RecordBatchTransactionOutcome as Outcome,
};
use crate::state::{RedbStateStore, RedbStateTransaction, redb::governance_catalog};
use wf_domain::workflow::{
    control::{ControlPlan, ControlState},
    request::{
        RequestScope, RetainedMetadata, RetainedOutcome, RetainedStatus,
        gate::{GateDecision, PointGateView, inspect_point_request},
    },
};
use wf_ports::{
    log::{PreparedCatalogProgressRequest, prepare_catalog_progress_request},
    state::{GroupApplyStore, GroupApplyTransaction, StateTransactionError as S},
};
use wf_types::version::{LogIdentity, TimestampMillis};

pub(super) fn apply(
    store: &mut RedbStateStore,
    scope: &RequestScope,
    encoded: &[u8],
    log: LogIdentity,
    limits: Limits,
) -> Result<Outcome, E> {
    let available = workspace(limits)?;
    let prepared = prepare_catalog_progress_request(encoded, limits.codec, available)?;
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
        governance_catalog::commit_progress(tx, scope, expected, control, None, None)?;
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
    fresh(tx, scope, prepared, expected, control, limits)
}

fn fresh(
    tx: RedbStateTransaction<'_>,
    scope: &RequestScope,
    prepared: PreparedCatalogProgressRequest<'_>,
    expected: ControlState,
    control: ControlPlan,
    limits: Limits,
) -> Result<Outcome, E> {
    let (owned_scope, request, intent, command) = prepared.into_retained_parts();
    let mut response = Vec::new();
    response.try_reserve_exact(6).map_err(|_| S::Allocation)?;
    response.extend_from_slice(b"FWPR\0\x01");
    let expires = control
        .execution_time()
        .get()
        .checked_add(limits.outcome_ttl_ms)
        .filter(|end| *end <= wf_domain::workflow::MAX_EXACT_MILLIS)
        .ok_or(S::Limit)?;
    let original_position = control.next().last_entry().ok_or(S::InvalidChanges)?.log();
    let outcome = RetainedOutcome::new(
        RetainedMetadata {
            scope: owned_scope,
            request,
            intent,
            original_position,
            original_time: control.execution_time(),
            expires_at: TimestampMillis::new(expires),
            status: RetainedStatus::Accepted,
        },
        13,
        response,
        limits.codec.admission(),
    )
    .map_err(|_| S::InvalidChanges)?;
    governance_catalog::commit_progress(
        tx,
        scope,
        expected,
        control,
        Some(command),
        Some(&outcome),
    )?;
    Ok(Outcome::Retained {
        control: control.next(),
        outcome: Box::new(outcome),
    })
}

fn workspace(limits: Limits) -> Result<usize, E> {
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
    let required = limits
        .codec
        .admission()
        .decision_bytes()
        .checked_add(4096)
        .ok_or(S::Limit)?;
    if required > limits.maximum_staging_bytes {
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
