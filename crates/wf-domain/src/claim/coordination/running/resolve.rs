//! Ordinary caller resolution with a single operation-local work account.

use super::super::admission::{
    ClaimCoordinationError, ClaimCoordinationLimits, charge_coordination_work,
};
use super::super::schedule::{ScheduleCalls, admit_schedule_calls};
use super::super::{ResolvedRunningLimit, RunningCreditDemand, RunningCreditDemandKind};
use super::{
    RunningPolicyProjection, admit_explicit_credit_pair, admit_projection_catalog,
    aggregate_running_rule, merge_running_rule, running_config_version,
};
use crate::claim::caller::NormalizedClaimDue;
use crate::policy::installation::PolicyLimits;
use crate::policy::{complete::RunningClaimPolicyVersion, definition::PolicyVersion};
use crate::workflow::MAX_EXACT_MILLIS;
use crate::workflow::active::LimitEnforcement;
use wf_types::version::TimestampMillis;

/// Resolves one original caller's demand, re-admitting the current whole profile.
///
/// TTL is caller `lease_ms`; credit `lease_size` is a separate quantity. A valid
/// explicit pair skips only unused policy, and `requested_now` is identity-only.
///
/// # Errors
/// Rejects invalid caller/time/pair, current projection or governed quantity.
pub fn resolve_running_credit<'v, 'a>(
    call: &NormalizedClaimDue<'a>,
    projection: Option<&RunningPolicyProjection<'v>>,
    trusted_now: TimestampMillis,
    shard_count: u64,
    policy_limits: PolicyLimits,
    limits: ClaimCoordinationLimits,
) -> Result<Option<RunningCreditDemand<'v, 'a>>, ClaimCoordinationError> {
    let mut work = 0;
    let explicit = admit_running_call(call, trusted_now, shard_count, limits, &mut work)?;
    let (kind, resolved, auto_lease, config_version, policy_version) =
        if let Some((scope, shard_id)) = explicit {
            (
                RunningCreditDemandKind::Explicit { scope, shard_id },
                ResolvedRunningLimit {
                    limit: call.limit(),
                    enforcement: LimitEnforcement::ApproximateGlobal,
                    lease_size: call.limit(),
                },
                false,
                None,
                None,
            )
        } else {
            let Some(projection) = projection else {
                return Ok(None);
            };
            let Some(result) =
                resolve_projected_rule(call, projection, policy_limits, limits, &mut work)?
            else {
                return Ok(None);
            };
            let policy = projection.installed.policy();
            (
                RunningCreditDemandKind::PolicyDerived {
                    workflow_type: call.workflow_type(),
                    worker: call.worker(),
                },
                result,
                true,
                running_config_version(projection.installed.generation(), policy.version()),
                policy.version().and_then(|version| match version {
                    PolicyVersion::Number(n) if *n <= MAX_EXACT_MILLIS => {
                        Some(RunningClaimPolicyVersion::Number(*n))
                    }
                    PolicyVersion::Bytes(bytes) if !bytes.is_empty() => {
                        Some(RunningClaimPolicyVersion::Bytes(bytes))
                    }
                    _ => None,
                }),
            )
        };
    if call.limit() > 1000 || call.limit().max(resolved.lease_size) > 1000 {
        return Err(ClaimCoordinationError::InvalidLimit);
    }
    Ok(Some(RunningCreditDemand {
        kind,
        amount: call.limit(),
        limit: resolved.limit,
        lease_size: resolved.lease_size,
        enforcement: resolved.enforcement,
        auto_lease,
        ttl_ms: call.lease_ms(),
        trusted_now,
        config_version,
        policy_version,
    }))
}

pub(super) fn admit_running_call<'a>(
    call: &NormalizedClaimDue<'a>,
    trusted_now: TimestampMillis,
    shard_count: u64,
    limits: ClaimCoordinationLimits,
    work: &mut u64,
) -> Result<Option<(&'a [u8], u64)>, ClaimCoordinationError> {
    admit_schedule_calls(&ScheduleCalls::Ordinary(call), limits, work)?;
    if trusted_now.get() > MAX_EXACT_MILLIS {
        return Err(ClaimCoordinationError::InvalidClock);
    }
    if trusted_now
        .get()
        .checked_add(call.lease_ms())
        .is_none_or(|deadline| deadline > MAX_EXACT_MILLIS)
    {
        return Err(ClaimCoordinationError::DeadlineOverflow);
    }
    admit_explicit_credit_pair(call, shard_count)
}

fn resolve_projected_rule(
    call: &NormalizedClaimDue<'_>,
    projection: &RunningPolicyProjection<'_>,
    policy_limits: PolicyLimits,
    limits: ClaimCoordinationLimits,
    work: &mut u64,
) -> Result<Option<ResolvedRunningLimit>, ClaimCoordinationError> {
    admit_projection_catalog(
        projection.installed,
        projection.type_rule,
        projection.states,
        policy_limits,
        limits,
        work,
    )?;
    let policy = projection.installed.policy();
    let type_bytes = call
        .workflow_type()
        .len()
        .checked_add(policy.workflow_type().len())
        .ok_or(ClaimCoordinationError::WorkBudget)?;
    charge_coordination_work(
        work,
        u64::try_from(type_bytes).map_err(|_| ClaimCoordinationError::WorkBudget)?,
        limits,
    )?;
    if call.workflow_type() != policy.workflow_type() {
        return Err(ClaimCoordinationError::InvalidPolicyAuthority);
    }
    let (any, names) = call.states();
    let count = if any {
        projection.states.len().checked_add(1)
    } else {
        Some(names.len())
    }
    .ok_or(ClaimCoordinationError::WorkBudget)?;
    charge_coordination_work(
        work,
        u64::try_from(count)
            .map_err(|_| ClaimCoordinationError::WorkBudget)?
            .checked_mul(64)
            .ok_or(ClaimCoordinationError::WorkBudget)?,
        limits,
    )?;
    let mut result = None;
    if any {
        aggregate_running_rule(&mut result, projection.type_rule, call.limit())?;
        for (name, rule) in projection.states {
            charge_coordination_work(
                work,
                u64::try_from(name.len()).map_err(|_| ClaimCoordinationError::WorkBudget)?,
                limits,
            )?;
            aggregate_running_rule(
                &mut result,
                merge_running_rule(projection.type_rule, *rule),
                call.limit(),
            )?;
        }
    } else {
        for name in names {
            charge_coordination_work(
                work,
                u64::try_from(name.len()).map_err(|_| ClaimCoordinationError::WorkBudget)?,
                limits,
            )?;
            let state = projection.states.get(*name).copied().flatten();
            aggregate_running_rule(
                &mut result,
                merge_running_rule(projection.type_rule, state),
                call.limit(),
            )?;
        }
    }
    Ok(result)
}
