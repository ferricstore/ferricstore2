//! Restricted complete core Patch, one real point gate and one consumed writer.

use super::{RecordBatchTransactionLimits, RecordBatchTransactionOutcome};
use crate::state::RedbStateStore;
use std::fmt;
use wf_domain::policy::complete::{
    CompleteCorePolicyPatchApplyError, CompleteCorePolicyPatchFreshView, PolicySchemaCharge,
    StoredPolicyValue,
};
use wf_domain::workflow::control::ControlState;
use wf_domain::workflow::request::gate::PointGateView;
use wf_domain::workflow::request::{DispatchDecision, RequestScope};
use wf_ports::log::{
    CompleteCorePolicyPatchPointInspection, CompleteCorePolicyPatchRequestPlan,
    FreshCompleteCorePolicyPatchFrame, decide_complete_core_policy_patch_point_fresh,
    inspect_complete_core_policy_patch_point, prepare_complete_core_policy_patch_point_frame,
};
use wf_ports::state::{
    GroupApplyStore, GroupApplyTransaction, GroupCorePolicyChanges, GroupCorePolicyTransaction,
    StateTransactionError,
};
use wf_types::version::LogIdentity;

/// Operational refusal, never a retained business response.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CorePolicyTransactionError {
    /// A required immutable limit is invalid.
    InvalidLimits,
    /// Original frame, pure authority, schema or workspace refusal.
    Apply(CompleteCorePolicyPatchApplyError),
    /// Same-writer storage, profile or unproved durable completion.
    Transaction(StateTransactionError),
}
impl fmt::Display for CorePolicyTransactionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidLimits => f.write_str("invalid core policy limits"),
            Self::Apply(e) => fmt::Display::fmt(e, f),
            Self::Transaction(e) => fmt::Display::fmt(e, f),
        }
    }
}
impl std::error::Error for CorePolicyTransactionError {}
impl From<CompleteCorePolicyPatchApplyError> for CorePolicyTransactionError {
    fn from(e: CompleteCorePolicyPatchApplyError) -> Self {
        Self::Apply(e)
    }
}
impl From<StateTransactionError> for CorePolicyTransactionError {
    fn from(e: StateTransactionError) -> Self {
        Self::Transaction(e)
    }
}

/// Applies an internal normalized Patch with actual current/canonical publication.
/// Unresolved received-snapshot handles must remain unserved. This is not public SET.
/// # Errors
/// Refuses operational work, authority, unsupported custody or unproved durability.
pub fn apply_complete_core_policy_patch_transaction(
    store: &mut RedbStateStore,
    scope: &RequestScope,
    encoded: &[u8],
    log: LogIdentity,
    limits: RecordBatchTransactionLimits,
) -> Result<RecordBatchTransactionOutcome, CorePolicyTransactionError> {
    let available = workspace(limits)?;
    let mut charge = PolicySchemaCharge::new(limits.schema);
    let verified = prepare_complete_core_policy_patch_point_frame(
        encoded,
        limits.schema,
        limits.codec.admission(),
        &mut charge,
        available,
    )
    .map_err(|error| apply_error(error, limits))?;
    let tx = store.begin_apply(
        scope.group(),
        scope.recovery(),
        limits.codec.admission(),
        limits.maximum_read_steps,
        limits.maximum_read_bytes,
        limits.maximum_adapter_bytes,
    )?;
    let ceiling = tx.core_policy_schema_limits();
    if limits.schema.policy_limits().max_states() > ceiling.policy_limits().max_states()
        || limits.schema.max_work_bytes() > ceiling.max_work_bytes()
    {
        return Err(StateTransactionError::InvalidChanges.into());
    }
    let expected = tx.control()?;
    let (nonempty, original) = {
        let header = verified.header(log);
        if header.scope != scope {
            return Err(
                wf_domain::policy::complete::CompleteCorePolicyPatchApplyError::Apply(
                    wf_domain::workflow::batch::ApplyError::InvalidAuthority,
                )
                .into(),
            );
        }
        (
            tx.retained_nonempty(scope.tenant())?,
            tx.core_policy_outcome(scope.tenant(), header.request, limits.schema, &mut charge)?,
        )
    };
    let inspection = inspect_complete_core_policy_patch_point(
        verified,
        log,
        PointGateView {
            scope,
            control: expected,
            retained_nonempty: nonempty,
            outcome: original.as_ref(),
        },
        (limits.schema, limits.codec.admission()),
        &mut charge,
        available,
    )
    .map_err(|error| apply_error(error, limits))?;
    let mut staging = 1024_usize;
    admit_staging(staging, limits)?;
    match inspection {
        CompleteCorePolicyPatchPointInspection::Complete(plan) => {
            staging = staging
                .checked_add(validation_bytes(plan.decision())?)
                .ok_or(StateTransactionError::Limit)?;
            admit_staging(staging, limits)?;
            let changes = GroupCorePolicyChanges::new(
                scope,
                expected,
                None,
                *plan,
                (limits.schema, limits.codec.admission()),
                &mut charge,
                staging - 1024,
            )?;
            let layout = tx.commit_core_policy(&changes, &mut charge)?;
            let (control, kind) = nonfresh(changes.into_plan())?;
            store.committed_core_policy_layout(layout);
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
        CompleteCorePolicyPatchPointInspection::Fresh(fresh) => {
            let (layout, outcome) =
                apply_fresh(tx, scope, *fresh, expected, limits, &mut charge, staging)?;
            store.committed_core_policy_layout(layout);
            Ok(outcome)
        }
    }
}
fn apply_fresh(
    tx: impl GroupCorePolicyTransaction,
    scope: &RequestScope,
    fresh: FreshCompleteCorePolicyPatchFrame<'_>,
    expected: ControlState,
    limits: RecordBatchTransactionLimits,
    charge: &mut PolicySchemaCharge,
    mut staging: usize,
) -> Result<(u16, RecordBatchTransactionOutcome), CorePolicyTransactionError> {
    let requested = fresh.decoded().input().command().workflow_type.as_slice();
    staging = staging
        .checked_add(512)
        .and_then(|n| n.checked_add(requested.len()))
        .ok_or(StateTransactionError::Limit)?;
    admit_staging(staging, limits)?;
    let mut kind = Vec::new();
    kind.try_reserve_exact(requested.len())
        .map_err(|_| StateTransactionError::Allocation)?;
    kind.extend_from_slice(requested);
    let _layout = tx.core_policy_layout()?;
    let old = tx.stored_policy_value(scope.tenant(), &kind, limits.schema, charge)?;
    let plan = decide_complete_core_policy_patch_point_fresh(
        fresh,
        CompleteCorePolicyPatchFreshView {
            workflow_type: &kind,
            current: Ok(old.as_ref()),
            active_migration: Ok(None),
            completed_migration: Ok(None),
            history_max_events_limit: Ok(tx.core_policy_history_max_events_limit()),
            outcome_retention_ms: Ok(limits.outcome_ttl_ms),
        },
        charge,
    )
    .map_err(|error| apply_error(error, limits))?;
    drop(kind);
    let response_validation = validation_bytes(plan.decision())?;
    let predecessor_validation = match old.as_ref() {
        Some(StoredPolicyValue::Original(raw)) => original_validation(raw)?,
        _ => 0,
    };
    let validation = response_validation.max(predecessor_validation);
    staging = staging
        .checked_add(predecessor_validation)
        .and_then(|n| n.checked_add(response_validation))
        .ok_or(StateTransactionError::Limit)?;
    admit_staging(staging, limits)?;
    let changes = GroupCorePolicyChanges::new(
        scope,
        expected,
        old.as_ref(),
        plan,
        (limits.schema, limits.codec.admission()),
        charge,
        validation,
    )?;
    let layout = tx.commit_core_policy(&changes, charge)?;
    let (_, _, control, decision) = changes.into_plan().into_parts();
    let DispatchDecision::Fresh((candidate, outcome)) = decision else {
        return Err(StateTransactionError::Corrupt.into());
    };
    drop(candidate);
    Ok((
        layout,
        RecordBatchTransactionOutcome::Retained {
            control: control.next(),
            outcome: Box::new(outcome),
        },
    ))
}
fn workspace(limits: RecordBatchTransactionLimits) -> Result<usize, CorePolicyTransactionError> {
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
        return Err(CorePolicyTransactionError::InvalidLimits);
    }
    let live = limits
        .maximum_read_bytes
        .checked_add(limits.maximum_staging_bytes)
        .and_then(|n| n.checked_add(limits.maximum_adapter_bytes))
        .ok_or(StateTransactionError::Limit)?;
    limits
        .workspace_bytes
        .checked_sub(live)
        .ok_or(CorePolicyTransactionError::Apply(
            CompleteCorePolicyPatchApplyError::Workspace {
                bytes: live,
                maximum: limits.workspace_bytes,
            },
        ))
}
fn admit_staging(
    n: usize,
    limits: RecordBatchTransactionLimits,
) -> Result<(), StateTransactionError> {
    if n > limits.maximum_staging_bytes {
        Err(StateTransactionError::Limit)
    } else {
        Ok(())
    }
}
fn validation_bytes(
    decision: &DispatchDecision<'_, wf_domain::policy::complete::CompleteCorePolicyPatchFresh>,
) -> Result<usize, StateTransactionError> {
    let outcome = match decision {
        DispatchDecision::Fresh((_, o)) => Some(o),
        DispatchDecision::Replay(o) => Some(*o),
        _ => None,
    };
    match outcome {
        Some(o) if o.response().starts_with(b"FWPV") => o
            .response()
            .len()
            .checked_mul(216)
            .and_then(|n| n.checked_add(2048))
            .ok_or(StateTransactionError::Limit),
        Some(o)
            if o.intent().version() == 2
                && o.response_version() == 2
                && o.response().first() == Some(&131) =>
        {
            original_validation(o.response())
        }
        _ => Ok(0),
    }
}
fn original_validation(raw: &[u8]) -> Result<usize, StateTransactionError> {
    if raw.len() > wf_domain::policy::installation::MAX_ENCODED_POLICY_BYTES {
        return Err(StateTransactionError::InvalidChanges);
    }
    raw.len()
        .checked_mul(256)
        .and_then(|n| n.checked_add(65_536))
        .ok_or(StateTransactionError::Limit)
}
fn nonfresh(
    plan: CompleteCorePolicyPatchRequestPlan<'_>,
) -> Result<(wf_domain::workflow::control::ControlState, u8), StateTransactionError> {
    let (_, _, control, decision) = plan.into_parts();
    let kind = match decision {
        DispatchDecision::Replay(_) => 0,
        DispatchDecision::IntentConflict => 1,
        DispatchDecision::Expired => 2,
        DispatchDecision::LogReplay => 3,
        DispatchDecision::Fresh(_) => return Err(StateTransactionError::Corrupt),
    };
    Ok((control.next(), kind))
}

fn apply_error(
    error: CompleteCorePolicyPatchApplyError,
    limits: RecordBatchTransactionLimits,
) -> CorePolicyTransactionError {
    if let CompleteCorePolicyPatchApplyError::Workspace { bytes, .. } = error {
        let whole = limits
            .maximum_read_bytes
            .checked_add(limits.maximum_staging_bytes)
            .and_then(|n| n.checked_add(limits.maximum_adapter_bytes))
            .and_then(|n| n.checked_add(bytes));
        match whole {
            Some(bytes) => {
                CorePolicyTransactionError::Apply(CompleteCorePolicyPatchApplyError::Workspace {
                    bytes,
                    maximum: limits.workspace_bytes,
                })
            }
            None => CorePolicyTransactionError::Transaction(StateTransactionError::Limit),
        }
    } else {
        CorePolicyTransactionError::Apply(error)
    }
}
