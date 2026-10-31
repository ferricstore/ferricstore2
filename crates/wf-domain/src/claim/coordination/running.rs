//! Borrowed running-policy observations, without governance byte/owner proof.

use super::super::caller::NormalizedClaimDue;
use super::admission::{ClaimCoordinationError, ClaimCoordinationLimits, charge_coordination_work};
use super::{ResolvedRunningLimit, RunningLimitRule};
use crate::policy::definition::PolicyVersion;
use crate::policy::installation::{PolicyLimits, PolicyValue, validate_stored_policy};
use crate::workflow::MAX_EXACT_MILLIS;
use crate::workflow::active::LimitEnforcement;
use std::collections::BTreeMap;

mod current;
mod resolve;
pub use current::{CurrentRunningCreditError, resolve_current_running_credit};
pub use resolve::resolve_running_credit;

/// Structurally coherent borrowed catalogue, not authoritative governance bytes.
pub struct RunningPolicyProjection<'v> {
    installed: &'v PolicyValue,
    type_rule: Option<RunningLimitRule>,
    states: &'v BTreeMap<Vec<u8>, Option<RunningLimitRule>>,
}
impl<'v> RunningPolicyProjection<'v> {
    /// Original immutable installed owner.
    #[must_use]
    pub const fn installed(&self) -> &'v PolicyValue {
        self.installed
    }
    /// Optional scalar type rule.
    #[must_use]
    pub const fn type_rule(&self) -> Option<RunningLimitRule> {
        self.type_rule
    }
    /// Complete original state catalogue, including absent rules.
    #[must_use]
    pub const fn states(&self) -> &'v BTreeMap<Vec<u8>, Option<RunningLimitRule>> {
        self.states
    }
}
impl RunningLimitRule {
    /// Validates scalar quantities without certifying policy provenance.
    ///
    /// # Errors
    /// Limit must be exact/nonnegative; optional credit quantity exact/positive.
    pub fn new(
        limit: u64,
        enforcement: Option<LimitEnforcement>,
        lease_size: Option<u64>,
    ) -> Result<Self, ClaimCoordinationError> {
        if limit > MAX_EXACT_MILLIS {
            return Err(ClaimCoordinationError::InvalidLimit);
        }
        if lease_size.is_some_and(|quantity| quantity == 0 || quantity > MAX_EXACT_MILLIS) {
            return Err(ClaimCoordinationError::InvalidLease);
        }
        Ok(Self {
            limit,
            enforcement,
            lease_size,
        })
    }
}
/// Admits complete typed catalogue coherence, borrowing every supplied owner.
///
/// # Errors
/// Rejects current policy/profile/type/catalogue violations before comparisons.
pub fn admit_running_projection<'v>(
    installed: &'v PolicyValue,
    type_rule: Option<RunningLimitRule>,
    states: &'v BTreeMap<Vec<u8>, Option<RunningLimitRule>>,
    policy_limits: PolicyLimits,
    limits: ClaimCoordinationLimits,
) -> Result<RunningPolicyProjection<'v>, ClaimCoordinationError> {
    let mut work = 0;
    admit_projection_catalog(
        installed,
        type_rule,
        states,
        policy_limits,
        limits,
        &mut work,
    )?;
    Ok(RunningPolicyProjection {
        installed,
        type_rule,
        states,
    })
}

/// Generation takes precedence; zero permits only a legacy numeric observation.
#[must_use]
pub const fn running_config_version(
    generation: u64,
    version: Option<&PolicyVersion>,
) -> Option<u64> {
    if generation > 0 {
        Some(generation)
    } else if let Some(PolicyVersion::Number(number)) = version {
        Some(*number)
    } else {
        None
    }
}

fn admit_projection_catalog(
    installed: &PolicyValue,
    _type_rule: Option<RunningLimitRule>,
    states: &BTreeMap<Vec<u8>, Option<RunningLimitRule>>,
    policy_limits: PolicyLimits,
    limits: ClaimCoordinationLimits,
    work: &mut u64,
) -> Result<(), ClaimCoordinationError> {
    let policy = installed.policy();
    if policy.states().len() > policy_limits.max_states()
        || states.len() > policy_limits.max_states()
        || states.len() != policy.states().len()
    {
        return Err(ClaimCoordinationError::InvalidPolicyAuthority);
    }
    let rows = u64::try_from(states.len()).map_err(|_| ClaimCoordinationError::WorkBudget)?;
    charge_coordination_work(
        work,
        rows.checked_mul(128)
            .ok_or(ClaimCoordinationError::WorkBudget)?,
        limits,
    )?;
    let version = match policy.version() {
        Some(PolicyVersion::Bytes(bytes)) => Some(bytes.as_slice()),
        _ => None,
    };
    let mut stored_bytes = 0_u64;
    for value in std::iter::once(policy.workflow_type())
        .chain(version)
        .chain(policy.type_retry().exhausted_to.as_deref())
        .chain(policy.states().iter().flat_map(|(name, state)| {
            [Some(name.as_slice()), state.retry.exhausted_to.as_deref()]
                .into_iter()
                .flatten()
        }))
    {
        let bytes = u64::try_from(value.len()).map_err(|_| ClaimCoordinationError::WorkBudget)?;
        charge_coordination_work(work, bytes, limits)?;
        stored_bytes = stored_bytes
            .checked_add(bytes)
            .ok_or(ClaimCoordinationError::WorkBudget)?;
    }
    for name in states.keys() {
        charge_coordination_work(
            work,
            u64::try_from(name.len()).map_err(|_| ClaimCoordinationError::WorkBudget)?,
            limits,
        )?;
    }
    charge_coordination_work(
        work,
        stored_bytes
            .checked_add(
                rows.checked_mul(64)
                    .ok_or(ClaimCoordinationError::WorkBudget)?,
            )
            .ok_or(ClaimCoordinationError::WorkBudget)?,
        limits,
    )?;
    validate_stored_policy(installed, policy.workflow_type(), policy_limits)
        .map_err(|_| ClaimCoordinationError::InvalidPolicyAuthority)?;
    charge_coordination_work(
        work,
        rows.checked_mul(64)
            .ok_or(ClaimCoordinationError::WorkBudget)?,
        limits,
    )?;
    for ((stored, _), projected) in policy.states().iter().zip(states.keys()) {
        let bytes = stored
            .len()
            .checked_add(projected.len())
            .ok_or(ClaimCoordinationError::WorkBudget)?;
        charge_coordination_work(
            work,
            u64::try_from(bytes).map_err(|_| ClaimCoordinationError::WorkBudget)?,
            limits,
        )?;
    }
    if policy
        .states()
        .keys()
        .zip(states.keys())
        .any(|(stored, projected)| stored != projected)
    {
        return Err(ClaimCoordinationError::InvalidPolicyAuthority);
    }
    Ok(())
}
fn merge_running_rule(
    base: Option<RunningLimitRule>,
    state: Option<RunningLimitRule>,
) -> Option<RunningLimitRule> {
    state
        .map(|state| RunningLimitRule {
            limit: state.limit,
            enforcement: state.enforcement.or(base.and_then(|rule| rule.enforcement)),
            lease_size: state.lease_size.or(base.and_then(|rule| rule.lease_size)),
        })
        .or(base)
}
fn aggregate_running_rule(
    result: &mut Option<ResolvedRunningLimit>,
    rule: Option<RunningLimitRule>,
    amount: u64,
) -> Result<(), ClaimCoordinationError> {
    let Some(rule) = rule else {
        return Ok(());
    };
    let next = ResolvedRunningLimit {
        limit: rule.limit,
        enforcement: rule.enforcement.unwrap_or(LimitEnforcement::StrictGlobal),
        lease_size: rule.lease_size.unwrap_or(amount),
    };
    if next.lease_size == 0 || next.lease_size > MAX_EXACT_MILLIS {
        return Err(ClaimCoordinationError::InvalidLease);
    }
    *result = Some(match *result {
        None => next,
        Some(old) => ResolvedRunningLimit {
            limit: old.limit.min(next.limit),
            enforcement: if old.enforcement == LimitEnforcement::StrictGlobal
                || next.enforcement == LimitEnforcement::StrictGlobal
            {
                LimitEnforcement::StrictGlobal
            } else {
                LimitEnforcement::ApproximateGlobal
            },
            lease_size: old.lease_size.min(next.lease_size),
        },
    });
    Ok(())
}
fn admit_explicit_credit_pair<'a>(
    call: &NormalizedClaimDue<'a>,
    shard_count: u64,
) -> Result<Option<(&'a [u8], u64)>, ClaimCoordinationError> {
    match (call.governance_limit_scope(), call.governance_shard_id()) {
        (None, None) => Ok(None),
        (Some(scope), Some(shard)) => {
            if scope.is_empty() || scope.len() > 65_535 {
                return Err(ClaimCoordinationError::InvalidGovernancePair);
            }
            if shard >= shard_count || shard > MAX_EXACT_MILLIS {
                return Err(ClaimCoordinationError::InvalidGovernanceShard);
            }
            Ok(Some((scope, shard)))
        }
        _ => Err(ClaimCoordinationError::InvalidGovernancePair),
    }
}
