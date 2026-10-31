//! Explicit public funded parent; execution is separate from data preparation.
use super::atomic_claim::{begin, reply as original_reply};
use super::{AtomicClaimTransactionLimits, ClaimTransactionError, ClaimTransactionOutcome};
use crate::state::RedbStateStore;
use wf_domain::policy::complete::PolicySchemaCharge;
use wf_domain::workflow::request::RequestScope;
use wf_domain::workflow::request::gate::PointGateView;
use wf_ports::{
    log::{
        AtomicClaimInspection, RecordBatchApplyError, inspect_atomic_claim_point,
        prepare_public_claim_request,
    },
    state::{GroupApplyTransaction, StateTransactionError as S},
};
use wf_types::version::LogIdentity;
mod execute;
mod reply;

/// Whole original parent profiles; clients cannot raise any of these ceilings.
#[derive(Clone, Copy)]
pub struct PublicClaimTransactionLimits {
    /// Actual original atomic Claim, schema, value and per-plan credit profiles.
    pub base: AtomicClaimTransactionLimits,
    /// Cumulative represented credit work across all funding/refund/rejection steps.
    pub maximum_credit_work: u64,
    /// Prospective simultaneous credit plan, ID and encoding owner ceiling.
    pub maximum_credit_temporary_bytes: usize,
}

/// Applies public funding, Claim and original-ID refund under one original parent.
/// # Errors
/// Refuses unsupported current profiles, authority or operational resource failure.
pub fn apply_public_claim_transaction(
    store: &mut RedbStateStore,
    scope: &RequestScope,
    encoded: &[u8],
    log: LogIdentity,
    limits: PublicClaimTransactionLimits,
) -> Result<ClaimTransactionOutcome, ClaimTransactionError> {
    let base = limits.base.values.record_batch;
    let resources =
        super::claim::values::ClaimValueResources::new(limits.base.values, encoded.len())?;
    let available = workspace(limits)?;
    let public = prepare_public_claim_request(encoded, base.codec, limits.base.caller, available)
        .map_err(RecordBatchApplyError::Encoding)?;
    let prepared = public.prepared();
    if prepared.scope() != scope {
        return Err(S::InvalidChanges.into());
    }
    let reply_owned = available
        .checked_sub(prepared.owned_bytes())
        .ok_or(S::Limit)?;
    let tx = begin(store, scope, base)?;
    tx.require_governed_claim()?;
    let expected = tx.control()?;
    let retained_nonempty = tx.retained_nonempty(scope.tenant())?;
    let original = tx.outcome(scope.tenant(), prepared.request())?;
    let inspected = inspect_atomic_claim_point(
        prepared,
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
        let (control, kind) = original_reply::complete(tx, expected, prepared, gate)?;
        return original_reply::classify(control, kind, original);
    };
    drop(original);
    let mut schema = PolicySchemaCharge::new(base.schema);
    let mut work = 0;
    let (staged, funding) = crate::state::redb::start_public_claim(
        tx,
        expected,
        &fresh,
        reply_owned,
        limits,
        &mut schema,
        &mut work,
    )?;
    let (staged, rejection) =
        execute::run(&fresh, staged, &funding, limits, resources, &mut schema)?;
    if let Some(reason) = rejection {
        let usage = staged.usage();
        staged.abort()?;
        drop(funding);
        return reply::rejected(
            store,
            scope,
            expected,
            &fresh,
            limits,
            reply_owned,
            (reason, usage, &mut schema, &mut work),
        );
    }
    let (control, outcome) =
        crate::state::redb::finish_public_claim(staged, &fresh, funding, limits, None, &mut work)?;
    Ok(ClaimTransactionOutcome::Retained { control, outcome })
}

fn workspace(limits: PublicClaimTransactionLimits) -> Result<usize, ClaimTransactionError> {
    let base = limits.base.values.record_batch;
    if [
        base.maximum_read_steps,
        base.maximum_read_bytes,
        base.maximum_staging_bytes,
        base.maximum_adapter_bytes,
        limits.maximum_credit_temporary_bytes,
    ]
    .contains(&0)
        || base.outcome_ttl_ms == 0
        || limits.maximum_credit_work == 0
    {
        return Err(ClaimTransactionError::InvalidLimits);
    }
    let credit = limits
        .base
        .credit
        .max_owned_bytes()
        .checked_mul(2)
        .and_then(|n| n.checked_add(1_048_576))
        .ok_or(S::Limit)?;
    if credit > limits.maximum_credit_temporary_bytes {
        return Err(S::Limit.into());
    }
    let live = super::load::sum(&[
        base.maximum_read_bytes,
        base.maximum_staging_bytes,
        base.maximum_adapter_bytes,
        limits.base.values.maximum_value_temporary_bytes,
        limits.maximum_credit_temporary_bytes,
    ])?;
    Ok(base.workspace_bytes.checked_sub(live).ok_or(S::Limit)?)
}
