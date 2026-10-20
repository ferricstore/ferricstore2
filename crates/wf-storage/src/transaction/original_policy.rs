//! Original public allocator: one paid point gate and one actual consumed writer.

use super::{RecordBatchTransactionLimits, RecordBatchTransactionOutcome};
use crate::state::{RedbStateStore, RedbStateTransaction};
use std::fmt;
use wf_domain::policy::complete::{
    CompletePolicyEncodingError, OriginalCompletePolicyReplaceApplyError as RawError,
    OriginalPolicyReadValue, OriginalPolicyReplyError, OriginalPolicySetFreshView,
    PolicySchemaCharge,
};
use wf_domain::workflow::request::gate::PointGateView;
use wf_domain::workflow::request::{DispatchDecision, RequestScope};
use wf_ports::log::{
    FreshOriginalPolicySetFrame, OriginalPolicySetPointInspection, RecordBatchApplyError,
    decide_original_policy_set_point_fresh_bounded,
    inspect_original_policy_set_point_frame_bounded,
    prepare_original_policy_set_point_frame_bounded,
};
use wf_ports::state::{
    GroupApplyStore, GroupApplyTransaction, GroupCorePolicyTransaction, GroupOriginalPolicyChanges,
    GroupOriginalPolicyTransaction, StateTransactionError as StateError,
};
use wf_types::version::LogIdentity;

mod get;
pub use get::with_original_policy_get_reply;

/// Stage-specific original allocator refusal, never a retained caller diagnosis.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OriginalPolicyTransactionError {
    /// A required positive immutable limit is invalid.
    InvalidLimits,
    /// Shared command/frame admission failed.
    Apply(RecordBatchApplyError),
    /// Actual storage authority, cap, I/O or uncertain completion failed.
    Transaction(StateError),
    /// A reached direct policy/projection/schema operation failed.
    Policy(CompletePolicyEncodingError),
    /// Original pure preparation or bounded frame refused.
    Original(RawError),
    /// GET presentation or the caller's visitor refused.
    Reply(OriginalPolicyReplyError),
}
impl fmt::Display for OriginalPolicyTransactionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidLimits => f.write_str("invalid original policy transaction limits"),
            Self::Apply(e) => fmt::Display::fmt(e, f),
            Self::Transaction(e) => fmt::Display::fmt(e, f),
            Self::Policy(e) => fmt::Display::fmt(e, f),
            Self::Original(e) => fmt::Display::fmt(e, f),
            Self::Reply(e) => fmt::Display::fmt(e, f),
        }
    }
}
impl std::error::Error for OriginalPolicyTransactionError {}
impl From<StateError> for OriginalPolicyTransactionError {
    fn from(e: StateError) -> Self {
        Self::Transaction(e)
    }
}
impl From<CompletePolicyEncodingError> for OriginalPolicyTransactionError {
    fn from(e: CompletePolicyEncodingError) -> Self {
        Self::Policy(e)
    }
}
impl From<OriginalPolicyReplyError> for OriginalPolicyTransactionError {
    fn from(e: OriginalPolicyReplyError) -> Self {
        Self::Reply(e)
    }
}
impl From<RawError> for OriginalPolicyTransactionError {
    fn from(e: RawError) -> Self {
        Self::Original(e)
    }
}
impl From<RecordBatchApplyError> for OriginalPolicyTransactionError {
    fn from(e: RecordBatchApplyError) -> Self {
        Self::Apply(e)
    }
}

/// Applies the original committed SET with full raw authority and an owned receipt.
/// # Errors
/// Preserves ordered raw/input/profile/workspace and actual durable-state origins.
pub fn apply_original_policy_set_transaction(
    store: &mut RedbStateStore,
    scope: &RequestScope,
    encoded: &[u8],
    log: LogIdentity,
    limits: RecordBatchTransactionLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<RecordBatchTransactionOutcome, OriginalPolicyTransactionError> {
    let available = workspace(limits)?;
    let verified = prepare_original_policy_set_point_frame_bounded(
        encoded,
        limits.schema,
        limits.codec.admission(),
        charge,
        available,
    )
    .map_err(|e| original_error(e, limits))?;
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
        return Err(StateError::InvalidChanges.into());
    }
    let expected = tx.control()?;
    let header = verified.header(log);
    if header.scope != scope {
        return Err(StateError::InvalidChanges.into());
    }
    let nonempty = tx.retained_nonempty(scope.tenant())?;
    let original = tx.core_policy_outcome(scope.tenant(), header.request, limits.schema, charge)?;
    let inspected = inspect_original_policy_set_point_frame_bounded(
        PointGateView {
            scope,
            control: expected,
            retained_nonempty: nonempty,
            outcome: original.as_ref(),
        },
        verified,
        log,
        limits.schema,
        limits.codec.admission(),
        charge,
        available,
    )
    .map_err(|e| original_error(e, limits))?;
    admit_staging(1024, limits)?;
    let result = match inspected {
        OriginalPolicySetPointInspection::Complete(plan) => {
            let validation = match plan.decision() {
                DispatchDecision::Replay(o) => validation(o.response())?,
                _ => 0,
            };
            admit_staging(
                1024_usize
                    .checked_add(validation)
                    .ok_or(StateError::Limit)?,
                limits,
            )?;
            let changes = GroupOriginalPolicyChanges::new(
                scope,
                expected,
                None,
                *plan,
                (limits.codec.admission(), limits.schema),
                charge,
                validation,
            )?;
            let layout = tx.commit_original_policy_borrowed(&changes, limits.schema, charge)?;
            let (_, _, control, decision) = changes.into_plan().into_parts();
            let outcome = match decision {
                DispatchDecision::Replay(_) => RecordBatchTransactionOutcome::Retained {
                    control: control.next(),
                    outcome: Box::new(original.ok_or(StateError::Corrupt)?),
                },
                DispatchDecision::IntentConflict => {
                    RecordBatchTransactionOutcome::IntentConflict(control.next())
                }
                DispatchDecision::Expired => RecordBatchTransactionOutcome::Expired(control.next()),
                DispatchDecision::LogReplay => {
                    RecordBatchTransactionOutcome::LogReplay(control.next())
                }
                DispatchDecision::Fresh(_) => return Err(StateError::Corrupt.into()),
            };
            (layout, outcome)
        }
        OriginalPolicySetPointInspection::Fresh(fresh) => {
            apply_fresh(tx, scope, *fresh, expected, limits, charge, available)?
        }
    };
    store.committed_core_policy_layout(result.0);
    Ok(result.1)
}

fn apply_fresh(
    tx: RedbStateTransaction<'_>,
    scope: &RequestScope,
    fresh: FreshOriginalPolicySetFrame<'_>,
    expected: wf_domain::workflow::control::ControlState,
    limits: RecordBatchTransactionLimits,
    charge: &mut PolicySchemaCharge,
    available: usize,
) -> Result<(u16, RecordBatchTransactionOutcome), OriginalPolicyTransactionError> {
    let kind = fresh
        .verified()
        .policy_key()
        .strip_prefix(b"f:{f}:policy:")
        .filter(|s| !s.is_empty())
        .ok_or(StateError::InvalidChanges)?;
    let staging = 1536_usize
        .checked_add(kind.len())
        .ok_or(StateError::Limit)?;
    admit_staging(staging, limits)?;
    let mut query = Vec::new();
    query
        .try_reserve_exact(kind.len())
        .map_err(|_| StateError::Allocation)?;
    query.extend_from_slice(kind);
    if tx.original_policy_profile()? != 1 {
        return Err(StateError::UnsupportedPolicyProfile.into());
    }
    let current = tx.original_policy_current(scope.tenant(), &query, limits.schema, charge)?;
    if current.as_ref().is_some_and(|raw| raw.starts_with(b"FWPV")) {
        return Err(StateError::UnsupportedPolicyFormat.into());
    }
    let plan = decide_original_policy_set_point_fresh_bounded(
        fresh,
        OriginalPolicySetFreshView {
            current: current.as_deref().map_or(
                OriginalPolicyReadValue::Absent,
                OriginalPolicyReadValue::Binary,
            ),
            active_migration: Ok(0),
            completed_migration: Ok(0),
            history_max_events_limit: Ok(tx.core_policy_history_max_events_limit()),
            outcome_retention_ms: Ok(limits.outcome_ttl_ms),
        },
        limits.schema,
        limits.codec.admission(),
        charge,
        available,
    )
    .map_err(|e| original_error(e, limits))?;
    drop(query);
    let amount = match plan.decision() {
        DispatchDecision::Fresh((_, o)) => validation(o.response())?,
        _ => return Err(StateError::Corrupt.into()),
    };
    admit_staging(
        staging.checked_add(amount).ok_or(StateError::Limit)?,
        limits,
    )?;
    let changes = GroupOriginalPolicyChanges::new(
        scope,
        expected,
        current.as_deref(),
        plan,
        (limits.codec.admission(), limits.schema),
        charge,
        amount,
    )?;
    let layout = tx.commit_original_policy_borrowed(&changes, limits.schema, charge)?;
    let (_, _, control, decision) = changes.into_plan().into_parts();
    let DispatchDecision::Fresh((install, outcome)) = decision else {
        return Err(StateError::Corrupt.into());
    };
    drop(install);
    Ok((
        layout,
        RecordBatchTransactionOutcome::Retained {
            control: control.next(),
            outcome: Box::new(outcome),
        },
    ))
}
fn validation(bytes: &[u8]) -> Result<usize, StateError> {
    if bytes.first() == Some(&131) {
        bytes
            .len()
            .checked_mul(256)
            .and_then(|n| n.checked_add(65_536))
            .ok_or(StateError::Limit)
    } else {
        Ok(0)
    }
}
fn workspace(
    limits: RecordBatchTransactionLimits,
) -> Result<usize, OriginalPolicyTransactionError> {
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
        return Err(OriginalPolicyTransactionError::InvalidLimits);
    }
    let live = limits
        .maximum_read_bytes
        .checked_add(limits.maximum_staging_bytes)
        .and_then(|n| n.checked_add(limits.maximum_adapter_bytes))
        .ok_or(StateError::Limit)?;
    limits
        .workspace_bytes
        .checked_sub(live)
        .ok_or(OriginalPolicyTransactionError::Original(
            RawError::Workspace {
                bytes: live,
                maximum: limits.workspace_bytes,
            },
        ))
}
fn admit_staging(bytes: usize, limits: RecordBatchTransactionLimits) -> Result<(), StateError> {
    if bytes > limits.maximum_staging_bytes {
        Err(StateError::Limit)
    } else {
        Ok(())
    }
}
fn original_error(
    error: RawError,
    limits: RecordBatchTransactionLimits,
) -> OriginalPolicyTransactionError {
    if let RawError::Workspace { bytes, .. } = error {
        match limits
            .maximum_read_bytes
            .checked_add(limits.maximum_staging_bytes)
            .and_then(|n| n.checked_add(limits.maximum_adapter_bytes))
            .and_then(|n| n.checked_add(bytes))
        {
            Some(bytes) => OriginalPolicyTransactionError::Original(RawError::Workspace {
                bytes,
                maximum: limits.workspace_bytes,
            }),
            None => OriginalPolicyTransactionError::Transaction(StateError::Limit),
        }
    } else {
        OriginalPolicyTransactionError::Original(error)
    }
}
