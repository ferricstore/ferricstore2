//! One physical Claim phase, one retained gate and one consumed writer commit.

use super::{RecordBatchTransactionError, RecordBatchTransactionLimits};
use crate::state::RedbStateStore;
use std::fmt;
use wf_domain::claim::apply::ClaimWorkLimits;
use wf_domain::policy::complete::CompletePolicyEncodingError;
use wf_domain::workflow::batch::ApplyError;
use wf_domain::workflow::control::ControlState;
use wf_domain::workflow::request::gate::PointGateView;
use wf_domain::workflow::request::{RequestScope, RetainedOutcome};
use wf_ports::log::{
    ClaimApplyDecision, ClaimApplyPlan, ClaimPhaseInspection, RecordBatchApplyError,
    decide_fresh_claim_phase_observed, inspect_claim_phase_point, prepare_claim_phase,
};
use wf_ports::state::{
    GroupApplyStore, GroupApplyTransaction, GroupClaimChanges, StateTransactionError,
};
use wf_types::version::LogIdentity;

mod governed;
mod load;
mod observations;
pub use governed::apply_governed_claim_transaction;
pub(crate) mod values;

/// Immutable server profile; entry budgets never raise its operational limits.
#[derive(Clone, Copy)]
pub struct ClaimTransactionLimits {
    /// Existing explicit codec/schema/steps/R/S/C/W/retention profile.
    pub base: RecordBatchTransactionLimits,
    /// Continuous represented domain Claim work allowance.
    pub work: ClaimWorkLimits,
}

/// Operational refusal; none of these errors is a retained business result.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClaimTransactionError {
    /// A required positive configured limit is zero.
    InvalidLimits,
    /// Preparation, pure authority, retention or workspace refusal.
    Apply(RecordBatchApplyError),
    /// Backend/ownership/admission or unproved durable completion.
    Transaction(StateTransactionError),
    /// Complete installed-policy validation/projection refused.
    Policy(CompletePolicyEncodingError),
    /// This internal leaf cannot execute current governance.
    UnsupportedGovernance,
}

impl fmt::Display for ClaimTransactionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidLimits => formatter.write_str("invalid claim transaction limits"),
            Self::Apply(error) => fmt::Display::fmt(error, formatter),
            Self::Transaction(error) => fmt::Display::fmt(error, formatter),
            Self::Policy(error) => fmt::Display::fmt(error, formatter),
            Self::UnsupportedGovernance => {
                formatter.write_str("claim phase requires governing owner")
            }
        }
    }
}
impl std::error::Error for ClaimTransactionError {}
impl From<RecordBatchApplyError> for ClaimTransactionError {
    fn from(error: RecordBatchApplyError) -> Self {
        Self::Apply(error)
    }
}
impl From<StateTransactionError> for ClaimTransactionError {
    fn from(error: StateTransactionError) -> Self {
        Self::Transaction(error)
    }
}
impl From<CompletePolicyEncodingError> for ClaimTransactionError {
    fn from(error: CompletePolicyEncodingError) -> Self {
        Self::Policy(error)
    }
}
impl From<RecordBatchTransactionError> for ClaimTransactionError {
    fn from(error: RecordBatchTransactionError) -> Self {
        match error {
            RecordBatchTransactionError::InvalidLimits => Self::InvalidLimits,
            RecordBatchTransactionError::Apply(error) => Self::Apply(error),
            RecordBatchTransactionError::Transaction(error) => Self::Transaction(error),
            RecordBatchTransactionError::Policy(error) => Self::Policy(error),
        }
    }
}

/// Owned original response, returned only after consumed durable completion.
#[derive(Debug, Eq, PartialEq)]
pub enum ClaimTransactionOutcome {
    /// Fresh accepted/rejected or moved original replay receipt.
    Retained {
        /// Current processing checkpoint, independent of original reply time.
        control: ControlState,
        /// Complete original response and metadata without payload cloning.
        outcome: Box<RetainedOutcome>,
    },
    /// The original ID has a different trusted parent intent.
    IntentConflict(ControlState),
    /// Original guarantee expired; execution is never repeated.
    Expired(ControlState),
    /// Exact last log entry; no durable write.
    LogReplay(ControlState),
}

/// Applies a restricted trusted-parent op3 phase on the real group writer.
/// This leaf is not the complete public Claim/governance protocol.
/// # Errors
/// Refuses decoding, authority, budgets, current governance or unproved commit.
pub fn apply_claim_transaction(
    store: &mut RedbStateStore,
    expected_scope: &RequestScope,
    encoded: &[u8],
    log: LogIdentity,
    limits: ClaimTransactionLimits,
) -> Result<ClaimTransactionOutcome, ClaimTransactionError> {
    apply(store, expected_scope, encoded, log, limits, None)
}

fn apply(
    store: &mut RedbStateStore,
    expected_scope: &RequestScope,
    encoded: &[u8],
    log: LogIdentity,
    limits: ClaimTransactionLimits,
    value_resources: Option<values::ClaimValueResources>,
) -> Result<ClaimTransactionOutcome, ClaimTransactionError> {
    let live = super::load::sum(&[
        live_bytes(limits.base)?,
        value_resources.as_ref().map_or(0, |resources| {
            resources.limits.maximum_value_temporary_bytes
        }),
    ])?;
    let available =
        limits
            .base
            .workspace_bytes
            .checked_sub(live)
            .ok_or(RecordBatchApplyError::Workspace {
                attempted: live,
                maximum: limits.base.workspace_bytes,
            })?;
    let decoded = prepare_claim_phase(encoded, limits.base.codec, available)?;
    if &decoded.request().scope != expected_scope {
        return Err(RecordBatchApplyError::Apply(ApplyError::InvalidAuthority).into());
    }
    let tx = store.begin_apply(
        expected_scope.group(),
        expected_scope.recovery(),
        limits.base.codec.admission(),
        limits.base.maximum_read_steps,
        limits.base.maximum_read_bytes,
        limits.base.maximum_adapter_bytes,
    )?;
    if value_resources.is_some() && tx.value_manifest()?.layout != 8 {
        return Err(StateTransactionError::UnsupportedLayout.into());
    }
    let predecessor = tx.control()?;
    let retained_nonempty = tx.retained_nonempty(expected_scope.tenant())?;
    let original = tx.outcome(expected_scope.tenant(), &decoded.request().request)?;
    let inspection = inspect_claim_phase_point(
        decoded,
        log,
        PointGateView {
            scope: expected_scope,
            control: predecessor,
            outcome: original.as_ref(),
            retained_nonempty,
        },
        limits.base.codec,
        limits.base.workspace_bytes,
        limits.work,
    )?;
    match inspection {
        ClaimPhaseInspection::Complete(plan) => {
            let changes = GroupClaimChanges::new(
                expected_scope,
                predecessor,
                plan,
                limits.base.codec.admission(),
            )?;
            values::commit(tx, &changes, value_resources)?;
            let (control, kind) = complete_kind(changes.into_plan())?;
            Ok(match kind {
                0 => ClaimTransactionOutcome::Retained {
                    control,
                    outcome: Box::new(original.ok_or(StateTransactionError::Corrupt)?),
                },
                1 => ClaimTransactionOutcome::IntentConflict(control),
                2 => ClaimTransactionOutcome::Expired(control),
                _ => ClaimTransactionOutcome::LogReplay(control),
            })
        }
        ClaimPhaseInspection::Fresh(fresh) => {
            if fresh.query_preflight().is_err() {
                let plan = decide_fresh_claim_phase_observed(
                    fresh,
                    None,
                    live,
                    limits.base.outcome_ttl_ms,
                )?;
                let changes = GroupClaimChanges::new(
                    expected_scope,
                    predecessor,
                    plan,
                    limits.base.codec.admission(),
                )?;
                values::commit(tx, &changes, value_resources)?;
                return fresh_reply(changes.into_plan());
            }
            let mut loaded = load::LoadedClaim::load(&tx, expected_scope, &fresh, limits)?;
            let plan = observations::decide(fresh, expected_scope, &mut loaded, limits, live)?;
            let changes = GroupClaimChanges::new(
                expected_scope,
                predecessor,
                plan,
                limits.base.codec.admission(),
            )?;
            values::commit(tx, &changes, value_resources)?;
            fresh_reply(changes.into_plan())
        }
    }
}

/// Applies named Claim with captured value bindings on a full-history owner.
/// # Errors
/// Refuses unsupported layout, authority, budgets, governance or unproved commit.
pub fn apply_workflow_value_claim_transaction(
    store: &mut RedbStateStore,
    scope: &RequestScope,
    encoded: &[u8],
    log: LogIdentity,
    limits: super::WorkflowValueTransactionLimits,
    work: ClaimWorkLimits,
) -> Result<ClaimTransactionOutcome, ClaimTransactionError> {
    let resources = values::ClaimValueResources::new(limits, encoded.len())?;
    apply(
        store,
        scope,
        encoded,
        log,
        ClaimTransactionLimits {
            base: limits.record_batch,
            work,
        },
        Some(resources),
    )
}

fn live_bytes(limits: RecordBatchTransactionLimits) -> Result<usize, ClaimTransactionError> {
    if limits.maximum_read_steps == 0
        || limits.maximum_read_bytes == 0
        || limits.maximum_staging_bytes == 0
        || limits.maximum_adapter_bytes == 0
        || limits.workspace_bytes == 0
        || limits.outcome_ttl_ms == 0
    {
        return Err(ClaimTransactionError::InvalidLimits);
    }
    Ok(super::load::sum(&[
        limits.maximum_read_bytes,
        limits.maximum_staging_bytes,
        limits.maximum_adapter_bytes,
    ])?)
}

fn complete_kind(plan: ClaimApplyPlan<'_>) -> Result<(ControlState, u8), ClaimTransactionError> {
    let (control, decision) = plan.into_parts();
    let kind = match decision {
        ClaimApplyDecision::Replay(_) => 0,
        ClaimApplyDecision::IntentConflict => 1,
        ClaimApplyDecision::Expired => 2,
        ClaimApplyDecision::LogReplay => 3,
        ClaimApplyDecision::Fresh { .. } => return Err(StateTransactionError::Corrupt.into()),
    };
    Ok((control.next(), kind))
}

fn fresh_reply(plan: ClaimApplyPlan<'_>) -> Result<ClaimTransactionOutcome, ClaimTransactionError> {
    let (control, decision) = plan.into_parts();
    let ClaimApplyDecision::Fresh { operation, outcome } = decision else {
        return Err(StateTransactionError::Corrupt.into());
    };
    drop(operation);
    Ok(ClaimTransactionOutcome::Retained {
        control: control.next(),
        outcome,
    })
}
