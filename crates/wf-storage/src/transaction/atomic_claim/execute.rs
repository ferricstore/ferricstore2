//! Actual current frontiers see every prior uncommitted phase in the same writer.
use super::{
    ApplyError, AtomicClaimTransactionLimits, ClaimTransactionError, RecordBatchApplyError, S,
};
use crate::{
    state::redb::StagedAtomicClaim,
    transaction::claim::{
        load::LoadedClaim,
        observations,
        values::{ClaimValueResources, build_phase},
    },
};
use wf_domain::{
    claim::{
        apply::{ClaimDecision, ClaimRejection, preflight_observed_claim_query},
        coordination::{ClaimPhaseCompletion, advance_claim_schedule, plan_claim_schedule},
    },
    policy::complete::PolicySchemaCharge,
};
use wf_ports::log::FreshAtomicClaim;

pub(super) fn run<'s, 'p, 'input>(
    fresh: &FreshAtomicClaim<'p, 'input>,
    mut staged: StagedAtomicClaim<'s, 'p, 'input>,
    limits: AtomicClaimTransactionLimits,
    resources: ClaimValueResources,
    schema: &mut PolicySchemaCharge,
) -> Result<(StagedAtomicClaim<'s, 'p, 'input>, Option<ClaimRejection>), ClaimTransactionError> {
    let base = limits.values.record_batch;
    let call = fresh
        .prepared()
        .caller(limits.caller)
        .map_err(RecordBatchApplyError::Encoding)?;
    if let Some(reason) = staged.initial_rejection() {
        return Ok((staged, Some(reason)));
    }
    let mut schedule = plan_claim_schedule(&call, limits.coordination)
        .map_err(|_| RecordBatchApplyError::Apply(ApplyError::InvalidAuthority))?;
    let phase_limits = super::super::ClaimTransactionLimits {
        base,
        work: limits.work,
    };
    let (mut work, mut staging, mut ordinals) = (0_u64, 0, 0_u32);
    let mut resources = Some(resources);
    while let Some(phase) = schedule.phase().map_err(|_| S::InvalidChanges)? {
        let (scanned, heads, _) = staged.progress();
        let envelope = match fresh
            .envelope(&phase, (scanned, heads, ordinals), base.codec)
            .map_err(RecordBatchApplyError::Encoding)?
        {
            Ok(envelope) => envelope,
            Err(reason) => return Ok((staged, Some(reason))),
        };
        let query_work = match preflight_observed_claim_query(
            &envelope,
            fresh.control().execution_time(),
            base.codec.admission(),
            limits.work,
        )
        .map_err(RecordBatchApplyError::Apply)?
        {
            Ok((_, work)) => work,
            Err(reason) => return Ok((staged, Some(reason))),
        };
        let prior = work.checked_add(query_work).ok_or(S::Limit)?;
        let mut loaded = LoadedClaim::load_atomic(
            staged.tx(),
            fresh.prepared().scope(),
            fresh,
            &envelope,
            phase_limits,
            schema,
            staging,
        )?;
        let decision = observations::decide_atomic(
            fresh.prepared().scope(),
            &mut loaded,
            &envelope,
            fresh.control().execution_time(),
            phase_limits,
            prior,
        )?;
        let ClaimDecision::Accepted(plan) = &decision else {
            let ClaimDecision::Rejected(reason) = decision else {
                return Err(S::Corrupt.into());
            };
            return Ok((staged, Some(reason)));
        };
        work = plan.work_bytes();
        let count = plan.batch().map_or(0, |batch| batch.batch().items().len());
        ordinals = ordinals
            .checked_add(plan.batch().map_or(
                0,
                wf_domain::workflow::indexed::IndexedBatchPlan::consumed_ordinals,
            ))
            .ok_or(S::Limit)?;
        let mut values = build_phase(
            staged.tx(),
            fresh.prepared().scope(),
            fresh.prepared().digest(),
            plan.batch(),
            resources.take().ok_or(S::Corrupt)?,
        )?;
        staged.stage(phase.kind(), &decision, &mut values)?;
        resources = Some(values.into_resources(limits.values));
        drop(decision);
        staging = loaded.staging_bytes();
        advance_claim_schedule(
            &mut schedule,
            ClaimPhaseCompletion::Accepted {
                phase: phase.index(),
                claimed: u64::try_from(count).map_err(|_| S::Limit)?,
            },
        )
        .map_err(|_| S::InvalidChanges)?;
    }
    Ok((staged, None))
}
