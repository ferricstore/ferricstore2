//! One original parent over an actual group writer; no leaf submissions.

use super::{ClaimTransactionError, ClaimTransactionOutcome, WorkflowValueTransactionLimits};
use crate::state::RedbStateStore;
use wf_domain::workflow::{batch::ApplyError, request::gate::PointGateView};
use wf_domain::{
    claim::{
        apply::ClaimWorkLimits, caller::ClaimCallerLimits, coordination::ClaimCoordinationLimits,
    },
    governance::credit::CreditLimits,
    workflow::request::RequestScope,
};
use wf_ports::{
    log::{
        AtomicClaimInspection, RecordBatchApplyError, inspect_atomic_claim_point,
        prepare_atomic_claim_request,
    },
    state::{GroupApplyStore, GroupApplyTransaction, StateTransactionError as S},
};
use wf_types::version::LogIdentity;
mod execute;
mod reply;

/// Immutable server profiles carried across every phase of one actual parent.
#[derive(Clone, Copy)]
pub struct AtomicClaimTransactionLimits {
    /// Actual value/history/read/staging/codec and outcome retention profile.
    pub values: WorkflowValueTransactionLimits,
    /// Whole-parent represented Claim work maximum.
    pub work: ClaimWorkLimits,
    /// Original caller normalization ownership profile.
    pub caller: ClaimCallerLimits,
    /// Whole original caller schedule profile.
    pub coordination: ClaimCoordinationLimits,
    /// Actual governing-credit owner/read profile.
    pub credit: CreditLimits,
}

/// Applies every scheduled phase under one parent and one final commit.
/// # Errors
/// Refuses unsupported profiles, actual authority or operational resource failure.
pub fn apply_atomic_claim_transaction(
    store: &mut RedbStateStore,
    scope: &RequestScope,
    encoded: &[u8],
    log: LogIdentity,
    limits: AtomicClaimTransactionLimits,
) -> Result<ClaimTransactionOutcome, ClaimTransactionError> {
    let base = limits.values.record_batch;
    let resources = super::claim::values::ClaimValueResources::new(limits.values, encoded.len())?;
    let live = super::load::sum(&[
        base.maximum_read_bytes,
        base.maximum_staging_bytes,
        base.maximum_adapter_bytes,
        limits.values.maximum_value_temporary_bytes,
    ])?;
    if base.maximum_read_steps == 0
        || base.maximum_read_bytes == 0
        || base.maximum_staging_bytes == 0
        || base.maximum_adapter_bytes == 0
        || base.outcome_ttl_ms == 0
    {
        return Err(ClaimTransactionError::InvalidLimits);
    }
    let available =
        base.workspace_bytes
            .checked_sub(live)
            .ok_or(RecordBatchApplyError::Workspace {
                attempted: live,
                maximum: base.workspace_bytes,
            })?;
    let prepared = prepare_atomic_claim_request(encoded, base.codec, limits.caller, available)
        .map_err(RecordBatchApplyError::Encoding)?;
    if prepared.scope() != scope {
        return Err(RecordBatchApplyError::Apply(ApplyError::InvalidAuthority).into());
    }
    let reply_owned =
        available
            .checked_sub(prepared.owned_bytes())
            .ok_or(RecordBatchApplyError::Workspace {
                attempted: prepared.owned_bytes(),
                maximum: available,
            })?;
    let tx = begin(store, scope, base)?;
    tx.require_governed_claim()?;
    let expected = tx.control()?;
    let retained_nonempty = tx.retained_nonempty(scope.tenant())?;
    let original = tx.outcome(scope.tenant(), prepared.request())?;
    let inspected = inspect_atomic_claim_point(
        &prepared,
        log,
        PointGateView {
            scope,
            control: expected,
            outcome: original.as_ref(),
            retained_nonempty,
        },
        base.codec,
    )?;
    let AtomicClaimInspection::Fresh(fresh) = inspected else {
        let AtomicClaimInspection::Complete(gate) = inspected else {
            return Err(S::Corrupt.into());
        };
        let (control, kind) = reply::complete(tx, expected, &prepared, gate)?;
        return reply::classify(control, kind, original);
    };
    let mut schema = wf_domain::policy::complete::PolicySchemaCharge::new(base.schema);
    let staged = crate::state::redb::StagedAtomicClaim::new(
        tx,
        expected,
        &fresh,
        reply_owned,
        limits,
        &mut schema,
    )?;
    let (staged, rejection) = execute::run(&fresh, staged, limits, resources, &mut schema)?;
    if let Some(reason) = rejection {
        let usage = staged.usage();
        staged.abort()?;
        return reply::rejected(
            store,
            scope,
            expected,
            &fresh,
            limits,
            reply_owned,
            (reason, usage, &mut schema),
        );
    }
    let (control, outcome) = staged.finish(base.outcome_ttl_ms, None, &mut schema)?;
    Ok(ClaimTransactionOutcome::Retained { control, outcome })
}

fn begin<'s>(
    store: &'s mut RedbStateStore,
    scope: &'s RequestScope,
    base: super::RecordBatchTransactionLimits,
) -> Result<crate::state::RedbStateTransaction<'s>, ClaimTransactionError> {
    Ok(store.begin_apply(
        scope.group(),
        scope.recovery(),
        base.codec.admission(),
        base.maximum_read_steps,
        base.maximum_read_bytes,
        base.maximum_adapter_bytes,
    )?)
}
