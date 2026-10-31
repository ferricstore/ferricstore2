//! Running Claim content from complete stored backing, without live authority.

use super::{
    CompletePolicyEncodingError as Error, LimitData, PolicySchemaCharge, PolicySchemaError,
    PolicySchemaLimits, StoredPolicyValue, charge_schema,
};
use crate::workflow::active::LimitEnforcement;

mod lookup;
mod original;
mod typed;

/// Source-compatible customer version, borrowing an original binary label.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RunningClaimPolicyVersion<'v> {
    /// Nonnegative exact numeric customer version.
    Number(u64),
    /// Nonempty original customer label.
    Bytes(&'v [u8]),
}

/// Resolved running-rule content; it certifies no installed or credit ownership.
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct ResolvedRunningClaimPolicy<'v> {
    limit: u64,
    lease_size: u64,
    enforcement: LimitEnforcement,
    config_version: Option<u64>,
    policy_version: Option<RunningClaimPolicyVersion<'v>>,
}

impl<'v> ResolvedRunningClaimPolicy<'v> {
    /// Minimum applicable running limit, preserving zero.
    #[must_use]
    pub const fn limit(self) -> u64 {
        self.limit
    }
    /// Minimum applicable credit quantity, independent of caller lease duration.
    #[must_use]
    pub const fn lease_size(self) -> u64 {
        self.lease_size
    }
    /// Strict whenever any applicable rule requires strict enforcement.
    #[must_use]
    pub const fn enforcement(self) -> LimitEnforcement {
        self.enforcement
    }
    /// Positive stored generation, otherwise the legacy numeric customer version.
    #[must_use]
    pub const fn config_version(self) -> Option<u64> {
        self.config_version
    }
    /// Top-level source customer version, with borrowed original label bytes.
    #[must_use]
    pub const fn policy_version(self) -> Option<RunningClaimPolicyVersion<'v>> {
        self.policy_version
    }
}

/// Resolves actual complete stored content for one original Claim state filter.
/// `None` states means any configured state plus type fallback. An empty slice
/// means no selected state. This pure function establishes no read freshness.
/// # Errors
/// Refuses inconsistent backing/type/profile, malformed input or work exhaustion.
pub fn resolve_stored_running_claim_policy<'v>(
    value: &'v StoredPolicyValue,
    workflow_type: &[u8],
    states: Option<&[&[u8]]>,
    amount: u64,
    schema: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<Option<ResolvedRunningClaimPolicy<'v>>, Error> {
    if charge.profile() != schema {
        return Err(Error::Schema(PolicySchemaError::ProfileMismatch));
    }
    if amount == 0 || amount > crate::workflow::MAX_EXACT_MILLIS || workflow_type.len() > 65_535 {
        return Err(Error::Schema(PolicySchemaError::InvalidAuthority));
    }
    pay(charge, 128)?;
    if let Some(states) = states {
        if states.len() > schema.policy_limits().max_states() {
            return Err(Error::ItemLimit);
        }
        for name in states {
            if name.len() > 65_535 {
                return Err(Error::ByteLimit);
            }
            pay(charge, name.len().checked_add(128).ok_or_else(overflow)?)?;
        }
    }
    match value {
        StoredPolicyValue::Complete(value) => {
            typed::resolve(value, workflow_type, states, amount, schema, charge)
        }
        StoredPolicyValue::Original(raw) => {
            original::resolve(raw, workflow_type, states, amount, schema, charge)
        }
    }
}

fn aggregate(
    result: &mut Option<ResolvedRunningClaimPolicy<'_>>,
    rule: Option<LimitData>,
    amount: u64,
) -> Result<(), Error> {
    let Some(rule) = rule else {
        return Ok(());
    };
    let lease_size = rule.lease_size.unwrap_or(amount);
    if lease_size == 0 || lease_size > crate::workflow::MAX_EXACT_MILLIS {
        return Err(Error::UnsupportedPolicyProfile);
    }
    let enforcement = rule.enforcement.unwrap_or(LimitEnforcement::StrictGlobal);
    match result {
        Some(old) => {
            old.limit = old.limit.min(rule.limit);
            old.lease_size = old.lease_size.min(lease_size);
            if enforcement == LimitEnforcement::StrictGlobal {
                old.enforcement = enforcement;
            }
        }
        None => {
            *result = Some(ResolvedRunningClaimPolicy {
                limit: rule.limit,
                lease_size,
                enforcement,
                config_version: None,
                policy_version: None,
            });
        }
    }
    Ok(())
}

fn pay(charge: &mut PolicySchemaCharge, amount: usize) -> Result<(), Error> {
    charge_schema(charge, u64::try_from(amount).map_err(|_| overflow())?, 0).map_err(Error::Schema)
}

fn overflow() -> Error {
    Error::Schema(PolicySchemaError::LengthOverflow)
}
