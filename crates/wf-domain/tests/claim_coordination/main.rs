//! Pure scheduling and identified-credit values, without apply or owner authority.
#![forbid(unsafe_code)]

use std::collections::BTreeMap;

use wf_domain::claim::caller::{
    ClaimCallerLimits, ClaimDueInput, NormalizedClaimDue, normalize_claim_due,
};
use wf_domain::claim::coordination::{
    ClaimCoordinationLimits, ClaimCreditBatch, ClaimPhase, ClaimPhaseCompletion, ClaimSchedule,
    RunningCreditDemand, admit_claim_credit_batch, advance_claim_schedule,
};
use wf_domain::policy::RetryOverride;
use wf_domain::policy::definition::{PolicyVersion, StatePolicy};
use wf_domain::policy::installation::{PolicyLimits, PolicyValue};
use wf_domain::policy::restoration::{PolicySnapshotData, restore_candidate};
use wf_domain::retention::RetentionOverride;

mod credit;
mod running;
mod schedule;

type TestError = Box<dyn std::error::Error>;
type TestResult<T = ()> = Result<T, TestError>;

fn caller(input: ClaimDueInput<'_>) -> TestResult<NormalizedClaimDue<'_>> {
    Ok(normalize_claim_due(input, ClaimCallerLimits::default())?)
}

fn policy_value(
    workflow_type: Vec<u8>,
    version: Option<PolicyVersion>,
    states: BTreeMap<Vec<u8>, StatePolicy>,
) -> TestResult<PolicyValue> {
    let candidate = restore_candidate(
        1,
        PolicySnapshotData {
            workflow_type,
            version,
            max_active_ms: None,
            retry: RetryOverride::default(),
            retention: RetentionOverride::default(),
            states,
        },
        PolicyLimits::default(),
    )?;
    // Length-only value fixture; these bytes certify no codec or installation.
    Ok(candidate.bind_bytes(&[1], vec![1], PolicyLimits::default())?)
}

fn phases(
    schedule: &mut ClaimSchedule<'_, '_>,
    observations: &[u64],
) -> TestResult<Vec<(ClaimPhase, u64)>> {
    let mut actual = Vec::new();
    for &claimed in observations {
        let phase = schedule.phase()?.ok_or("missing observed phase")?;
        actual.push((phase.kind(), phase.quota()));
        advance_claim_schedule(
            schedule,
            ClaimPhaseCompletion::Accepted {
                phase: phase.index(),
                claimed,
            },
        )?;
    }
    Ok(actual)
}

fn credit_batch<'a>(
    demand: &RunningCreditDemand<'_, '_>,
    scope: &'a [u8],
    ids: &'a [&'a [u8]],
) -> TestResult<ClaimCreditBatch<'a>> {
    Ok(admit_claim_credit_batch(
        demand,
        scope,
        0,
        1,
        ids,
        ClaimCoordinationLimits::default(),
    )?)
}
