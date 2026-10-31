//! Original caller demand over actual complete policy content, without I/O.

use super::super::{
    ClaimCoordinationError, ClaimCoordinationLimits, RunningCreditDemand, RunningCreditDemandKind,
};
use super::resolve::admit_running_call;
use crate::claim::caller::NormalizedClaimDue;
use crate::policy::complete::resolve_stored_running_claim_policy;
use crate::policy::complete::{
    CompletePolicyEncodingError, PolicySchemaCharge, PolicySchemaLimits, StoredPolicyValue,
};
use crate::workflow::active::LimitEnforcement;
use std::fmt;
use wf_types::version::TimestampMillis;

/// Preserves normalized-caller failures separately from stored-content refusal.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CurrentRunningCreditError {
    /// Caller, time, route or credit quantity is invalid.
    Coordination(ClaimCoordinationError),
    /// Complete actual policy backing or its continuous schema account refused.
    Policy(CompletePolicyEncodingError),
}

impl fmt::Display for CurrentRunningCreditError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Coordination(error) => fmt::Display::fmt(error, formatter),
            Self::Policy(error) => fmt::Display::fmt(error, formatter),
        }
    }
}
impl std::error::Error for CurrentRunningCreditError {}
impl From<ClaimCoordinationError> for CurrentRunningCreditError {
    fn from(error: ClaimCoordinationError) -> Self {
        Self::Coordination(error)
    }
}
impl From<CompletePolicyEncodingError> for CurrentRunningCreditError {
    fn from(error: CompletePolicyEncodingError) -> Self {
        Self::Policy(error)
    }
}

/// Resolves an original caller using actual stored content or a valid explicit pair.
/// This functional boundary supplies no current read or external credit authority.
/// # Errors
/// Rejects caller/time/pair before unused policy, then complete policy or work errors.
pub fn resolve_current_running_credit<'v, 'a>(
    call: &NormalizedClaimDue<'a>,
    stored: Option<&'v StoredPolicyValue>,
    trusted_now: TimestampMillis,
    shard_count: u64,
    schema: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
    limits: ClaimCoordinationLimits,
) -> Result<Option<RunningCreditDemand<'v, 'a>>, CurrentRunningCreditError> {
    let mut work = 0;
    let explicit = admit_running_call(call, trusted_now, shard_count, limits, &mut work)?;
    let (kind, limit, lease_size, enforcement, auto_lease, config_version, policy_version) =
        if let Some((scope, shard_id)) = explicit {
            (
                RunningCreditDemandKind::Explicit { scope, shard_id },
                call.limit(),
                call.limit(),
                LimitEnforcement::ApproximateGlobal,
                false,
                None,
                None,
            )
        } else {
            let Some(stored) = stored else {
                return Ok(None);
            };
            let (any, names) = call.states();
            let rule = resolve_stored_running_claim_policy(
                stored,
                call.workflow_type(),
                (!any).then_some(names),
                call.limit(),
                schema,
                charge,
            )?;
            let Some(rule) = rule else {
                return Ok(None);
            };
            (
                RunningCreditDemandKind::PolicyDerived {
                    workflow_type: call.workflow_type(),
                    worker: call.worker(),
                },
                rule.limit(),
                rule.lease_size(),
                rule.enforcement(),
                true,
                rule.config_version(),
                rule.config_version().and(rule.policy_version()),
            )
        };
    if call.limit().max(lease_size) > 1000 {
        return Err(ClaimCoordinationError::InvalidLimit.into());
    }
    Ok(Some(RunningCreditDemand {
        kind,
        amount: call.limit(),
        limit,
        lease_size,
        enforcement,
        auto_lease,
        ttl_ms: call.lease_ms(),
        trusted_now,
        config_version,
        policy_version,
    }))
}
