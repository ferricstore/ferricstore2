//! Borrowed known-content/default resolution without executing governance services.

use std::collections::BTreeMap;

use crate::policy::definition::PolicyVersion;
use crate::policy::installation::{
    MAX_ENCODED_POLICY_BYTES, MAX_POLICY_GENERATION, MAX_POLICY_SNAPSHOT_BYTES,
};

use super::admission::charge_schema;
use super::rules::{merge_budget, merge_circuit, merge_limit};
use super::validation::admit_complete;
use super::{
    CompletePolicyValue, DenialAuditMode, GovernanceData, GovernanceMode, LimitData,
    PolicySchemaCharge, PolicySchemaError, PolicySchemaLimits, ResolvedApprovals, ResolvedAudit,
    ResolvedBudget, ResolvedCircuit, ResolvedEffects,
};

/// Borrowed known governance after complete current-profile admission.
///
/// This value supplies no current installed/remote credit/effect authority or
/// source ETF/hash identity. Nonempty leaves borrow the original complete owner.
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct ResolvedGovernance<'v> {
    type_governance: Option<&'v GovernanceData>,
    state_governance: Option<&'v GovernanceData>,
    customer_version: Option<&'v PolicyVersion>,
    limits: PolicySchemaLimits,
}

/// Re-admits the entire immutable value before the optional bounded state lookup.
///
/// Empty or unknown states retain type/default fallback. No current content is
/// skipped by an absent query, absent governance or a default/no-rule branch.
///
/// # Errors
/// Keeps profile/work/checked-length diagnoses distinct from current content or
/// queried-input `InvalidAuthority`, retaining all earlier successful work.
pub fn resolve_governance<'v>(
    value: &'v CompletePolicyValue,
    state: Option<&[u8]>,
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<ResolvedGovernance<'v>, PolicySchemaError> {
    if charge.profile() != limits {
        return Err(PolicySchemaError::ProfileMismatch);
    }
    if value.generation() == 0
        || value.generation() > MAX_POLICY_GENERATION
        || value.encoded().len() > MAX_ENCODED_POLICY_BYTES
    {
        return Err(PolicySchemaError::InvalidAuthority);
    }
    admit_complete(value.data(), limits, charge, false).map_err(|error| match error {
        error @ (PolicySchemaError::ProfileMismatch
        | PolicySchemaError::WorkBytes { .. }
        | PolicySchemaError::LengthOverflow) => error,
        _ => PolicySchemaError::InvalidAuthority,
    })?;
    let state_governance = if let Some(state) = state {
        if state.len() > MAX_POLICY_SNAPSHOT_BYTES {
            return Err(PolicySchemaError::InvalidAuthority);
        }
        let n = u64::try_from(state.len()).map_err(|_| PolicySchemaError::LengthOverflow)?;
        charge_schema(
            charge,
            n.checked_add(128)
                .ok_or(PolicySchemaError::LengthOverflow)?,
            0,
        )?;
        value
            .data()
            .states
            .get(state)
            .and_then(|state| state.governance.as_ref())
    } else {
        None
    };
    Ok(ResolvedGovernance {
        type_governance: value.data().governance.as_ref(),
        state_governance,
        customer_version: value.data().version.as_ref(),
        limits,
    })
}

impl<'v> ResolvedGovernance<'v> {
    /// Reads state/type mode or the minimal default.
    #[must_use]
    pub fn mode(self) -> GovernanceMode {
        self.state_governance
            .and_then(|g| g.mode)
            .or_else(|| self.type_governance.and_then(|g| g.mode))
            .unwrap_or(GovernanceMode::Minimal)
    }

    /// Returns original allowed/denied lists and the resolved key requirement.
    #[must_use]
    pub fn effects(self) -> ResolvedEffects<'v> {
        let type_effects = self.type_governance.and_then(|g| g.effects.as_ref());
        let state_effects = self.state_governance.and_then(|g| g.effects.as_ref());
        ResolvedEffects {
            allowed: select_list(
                type_effects.and_then(|g| g.allowed.as_deref()),
                state_effects.and_then(|g| g.allowed.as_deref()),
            ),
            denied: select_list(
                type_effects.and_then(|g| g.denied.as_deref()),
                state_effects.and_then(|g| g.denied.as_deref()),
            ),
            require_idempotency_key: state_effects
                .and_then(|g| g.require_idempotency_key)
                .or_else(|| type_effects.and_then(|g| g.require_idempotency_key))
                .unwrap_or(true),
        }
    }

    /// Returns state/type audit fields or their source defaults.
    #[must_use]
    pub fn audit(self) -> ResolvedAudit {
        let type_audit = self.type_governance.and_then(|g| g.audit.as_ref());
        let state_audit = self.state_governance.and_then(|g| g.audit.as_ref());
        ResolvedAudit {
            effect_events: state_audit
                .and_then(|g| g.effect_events)
                .or_else(|| type_audit.and_then(|g| g.effect_events))
                .unwrap_or(true),
            denials: state_audit
                .and_then(|g| g.denials)
                .or_else(|| type_audit.and_then(|g| g.denials))
                .unwrap_or(DenialAuditMode::Sampled),
        }
    }

    /// Returns original approval lists and an optional positive timeout.
    #[must_use]
    pub fn approvals(self) -> ResolvedApprovals<'v> {
        let type_approvals = self.type_governance.and_then(|g| g.approvals.as_ref());
        let state_approvals = self.state_governance.and_then(|g| g.approvals.as_ref());
        ResolvedApprovals {
            effects: select_list(
                type_approvals.and_then(|g| g.effects.as_deref()),
                state_approvals.and_then(|g| g.effects.as_deref()),
            ),
            states: select_list(
                type_approvals.and_then(|g| g.states.as_deref()),
                state_approvals.and_then(|g| g.states.as_deref()),
            ),
            assignees: select_list(
                type_approvals.and_then(|g| g.assignees.as_deref()),
                state_approvals.and_then(|g| g.assignees.as_deref()),
            ),
            timeout_ms: state_approvals
                .and_then(|g| g.timeout_ms)
                .or_else(|| type_approvals.and_then(|g| g.timeout_ms)),
        }
    }

    /// Borrows a customer label or exact-range numeric resolved version.
    ///
    /// The stored customer's whole `u64` range remains valid, but values above the
    /// source exact bound and historical governance-version fallback are not invented.
    #[must_use]
    pub fn policy_version(self) -> Option<&'v PolicyVersion> {
        self.customer_version.filter(
            |version| !matches!(version, PolicyVersion::Number(n) if *n > 9_007_199_254_740_991),
        )
    }

    /// Resolves one limit after charged type/state requested-key lookups.
    ///
    /// # Errors
    /// Returns profile, bounded-query or continuous work/length rejection.
    pub fn limit(
        self,
        name: &[u8],
        charge: &mut PolicySchemaCharge,
    ) -> Result<Option<LimitData>, PolicySchemaError> {
        if charge.profile() != self.limits {
            return Err(PolicySchemaError::ProfileMismatch);
        }
        let (type_rule, state_rule) = rule_lookup(
            self.type_governance.and_then(|g| g.limits.as_ref()),
            self.state_governance.and_then(|g| g.limits.as_ref()),
            name,
            charge,
        )?;
        for _ in [type_rule, state_rule].into_iter().flatten() {
            charge_schema(charge, 128, 0)?;
        }
        Ok(merge_limit(type_rule, state_rule))
    }

    /// Resolves one budget while borrowing original units, without spending.
    ///
    /// # Errors
    /// Returns profile, bounded-query or continuous work/length rejection.
    pub fn budget(
        self,
        name: &[u8],
        charge: &mut PolicySchemaCharge,
    ) -> Result<Option<ResolvedBudget<'v>>, PolicySchemaError> {
        if charge.profile() != self.limits {
            return Err(PolicySchemaError::ProfileMismatch);
        }
        let (type_rule, state_rule) = rule_lookup(
            self.type_governance.and_then(|g| g.budgets.as_ref()),
            self.state_governance.and_then(|g| g.budgets.as_ref()),
            name,
            charge,
        )?;
        for rule in [type_rule, state_rule].into_iter().flatten() {
            charge_schema(charge, 128, 0)?;
            if let Some(unit) = &rule.unit {
                charge_schema(
                    charge,
                    u64::try_from(unit.len()).map_err(|_| PolicySchemaError::LengthOverflow)?,
                    0,
                )?;
            }
        }
        Ok(merge_budget(type_rule, state_rule))
    }

    /// Resolves one circuit while borrowing original class lists, without effects.
    ///
    /// # Errors
    /// Returns profile, bounded-query or continuous work/length rejection.
    pub fn circuit(
        self,
        name: &[u8],
        charge: &mut PolicySchemaCharge,
    ) -> Result<Option<ResolvedCircuit<'v>>, PolicySchemaError> {
        if charge.profile() != self.limits {
            return Err(PolicySchemaError::ProfileMismatch);
        }
        let (type_rule, state_rule) = rule_lookup(
            self.type_governance.and_then(|g| g.circuits.as_ref()),
            self.state_governance.and_then(|g| g.circuits.as_ref()),
            name,
            charge,
        )?;
        for rule in [type_rule, state_rule].into_iter().flatten() {
            charge_schema(charge, 128, 0)?;
            let Some(classes) = &rule.error_classes else {
                continue;
            };
            charge_schema(charge, 128, 0)?;
            for class in classes {
                let n =
                    u64::try_from(class.len()).map_err(|_| PolicySchemaError::LengthOverflow)?;
                charge_schema(
                    charge,
                    n.checked_add(128)
                        .ok_or(PolicySchemaError::LengthOverflow)?,
                    0,
                )?;
            }
        }
        Ok(merge_circuit(type_rule, state_rule))
    }
}

fn select_list<'v>(
    type_list: Option<&'v [Vec<u8>]>,
    state_list: Option<&'v [Vec<u8>]>,
) -> &'v [Vec<u8>] {
    state_list.or(type_list).unwrap_or(&[])
}

fn rule_lookup<'v, T>(
    type_rules: Option<&'v BTreeMap<Vec<u8>, T>>,
    state_rules: Option<&'v BTreeMap<Vec<u8>, T>>,
    name: &[u8],
    charge: &mut PolicySchemaCharge,
) -> Result<(Option<&'v T>, Option<&'v T>), PolicySchemaError> {
    if name.len() > 65_535 {
        return Err(PolicySchemaError::InvalidAuthority);
    }
    let n = u64::try_from(name.len()).map_err(|_| PolicySchemaError::LengthOverflow)?;
    let work = n
        .checked_add(128)
        .ok_or(PolicySchemaError::LengthOverflow)?;
    let type_rule = if let Some(rules) = type_rules {
        charge_schema(charge, work, 0)?;
        rules.get(name)
    } else {
        None
    };
    let state_rule = if let Some(rules) = state_rules {
        charge_schema(charge, work, 0)?;
        rules.get(name)
    } else {
        None
    };
    Ok((type_rule, state_rule))
}
