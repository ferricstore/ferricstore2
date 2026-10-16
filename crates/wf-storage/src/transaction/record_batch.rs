//! One raw preparation/gate and consumed atomic publication, returning no borrows.

use wf_domain::workflow::batch::ApplyError;
use wf_domain::workflow::control::ControlState;
use wf_domain::workflow::request::RequestScope;
use wf_domain::workflow::request::gate::PointGateView;
use wf_ports::log::{
    RecordBatchApplyDecision, RecordBatchApplyError, RecordBatchApplyPlan, RecordBatchInspection,
    inspect_record_batch_point, prepare_record_batch,
};
use wf_ports::state::{
    GroupApplyChanges, GroupApplyStore, GroupApplyTransaction, StateTransactionError,
};
use wf_types::version::LogIdentity;

use super::load::{LoadedRecordBatch, sum};
use super::observations;
use super::{
    RecordBatchTransactionError, RecordBatchTransactionLimits, RecordBatchTransactionOutcome,
};

/// Applies one private batch under one coherent transaction and retained gate.
///
/// This initial driver eagerly prefetches bounded physical inputs. Operational
/// read failures may precede pure business refusal, even exact log replay when
/// its original retained read fails. Expected scope is independently caller-owned.
/// # Errors
/// Rejects profile, scope, preparation, authority, budgets or durable completion;
/// no acknowledgement is returned for an uncertain or failed commit.
pub fn apply_record_batch_transaction<S: GroupApplyStore>(
    store: &mut S,
    expected_scope: &RequestScope,
    encoded: &[u8],
    log: LogIdentity,
    limits: RecordBatchTransactionLimits,
) -> Result<RecordBatchTransactionOutcome, RecordBatchTransactionError> {
    let live = live_bytes(limits)?;
    let available =
        limits
            .workspace_bytes
            .checked_sub(live)
            .ok_or(RecordBatchApplyError::Workspace {
                attempted: live,
                maximum: limits.workspace_bytes,
            })?;
    let decoded = prepare_record_batch(encoded, limits.codec, available)?;
    if &decoded.request().scope != expected_scope {
        return Err(RecordBatchApplyError::Apply(ApplyError::InvalidAuthority).into());
    }
    let tx = store.begin_apply(
        expected_scope.group(),
        expected_scope.recovery(),
        limits.maximum_read_steps,
        limits.maximum_read_bytes,
        limits.maximum_adapter_bytes,
    )?;
    let predecessor = tx.control()?;
    let retained_nonempty = tx.retained_nonempty(expected_scope.tenant())?;
    let original = tx.outcome(expected_scope.tenant(), &decoded.request().request)?;
    let inspection = inspect_record_batch_point(
        decoded,
        log,
        PointGateView {
            scope: expected_scope,
            control: predecessor,
            outcome: original.as_ref(),
            retained_nonempty,
        },
        limits.codec,
        limits.workspace_bytes,
    )?;
    match inspection {
        RecordBatchInspection::Complete(plan) => {
            let changes = GroupApplyChanges::new(expected_scope, predecessor, plan)?;
            tx.commit(&changes)?;
            let (control, kind) = complete_kind(changes.into_plan())?;
            Ok(match kind {
                0 => RecordBatchTransactionOutcome::Retained {
                    control,
                    outcome: Box::new(original.ok_or(StateTransactionError::Corrupt)?),
                },
                1 => RecordBatchTransactionOutcome::IntentConflict(control),
                2 => RecordBatchTransactionOutcome::Expired(control),
                _ => RecordBatchTransactionOutcome::LogReplay(control),
            })
        }
        RecordBatchInspection::Fresh(fresh) => {
            let mut loaded = LoadedRecordBatch::load(&tx, expected_scope, fresh.decoded(), limits)?;
            let plan = observations::decide(fresh, expected_scope, &mut loaded, limits, live)?;
            let changes = GroupApplyChanges::new(expected_scope, predecessor, plan)?;
            tx.commit(&changes)?;
            fresh_reply(changes.into_plan())
        }
    }
}

fn live_bytes(limits: RecordBatchTransactionLimits) -> Result<usize, RecordBatchTransactionError> {
    if limits.maximum_read_steps == 0
        || limits.maximum_read_bytes == 0
        || limits.maximum_staging_bytes == 0
        || limits.maximum_adapter_bytes == 0
        || limits.workspace_bytes == 0
        || limits.outcome_ttl_ms == 0
    {
        return Err(RecordBatchTransactionError::InvalidLimits);
    }
    sum(&[
        limits.maximum_read_bytes,
        limits.maximum_staging_bytes,
        limits.maximum_adapter_bytes,
    ])
}

// Erase the retained borrow before moving its original owner into the reply.
fn complete_kind(
    plan: RecordBatchApplyPlan<'_>,
) -> Result<(ControlState, u8), RecordBatchTransactionError> {
    let (control, decision) = plan.into_parts();
    let kind = match decision {
        RecordBatchApplyDecision::Replay(_) => 0,
        RecordBatchApplyDecision::IntentConflict => 1,
        RecordBatchApplyDecision::Expired => 2,
        RecordBatchApplyDecision::LogReplay => 3,
        RecordBatchApplyDecision::Fresh { .. } => return Err(StateTransactionError::Corrupt.into()),
    };
    Ok((control.next(), kind))
}

fn fresh_reply(
    plan: RecordBatchApplyPlan<'_>,
) -> Result<RecordBatchTransactionOutcome, RecordBatchTransactionError> {
    let (control, decision) = plan.into_parts();
    let RecordBatchApplyDecision::Fresh { operation, outcome } = decision else {
        return Err(StateTransactionError::Corrupt.into());
    };
    drop(operation);
    Ok(RecordBatchTransactionOutcome::Retained {
        control: control.next(),
        outcome,
    })
}
