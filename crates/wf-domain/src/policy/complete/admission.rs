//! One immutable profile and a continuous logical occurrence/scratch account.

use std::collections::BTreeMap;

use crate::policy::installation::{PolicyError, PolicyLimits};
use crate::workflow::admission::AdmissionError;

use super::{
    CompletePolicyData, GovernanceData, GovernanceField, GovernanceRuleKind, PolicySchemaError,
};

mod owner;
pub use owner::complete_policy_value_owned_bytes;

/// Immutable normalized-content and represented-work admission profile.
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct PolicySchemaLimits {
    states: PolicyLimits,
    work_bytes: u64,
}

impl Default for PolicySchemaLimits {
    fn default() -> Self {
        Self {
            states: PolicyLimits::default(),
            work_bytes: 128 * 1_024 * 1_024,
        }
    }
}

impl PolicySchemaLimits {
    /// Selects a nonzero work profile without granting caller authority to raise it.
    ///
    /// # Errors
    /// Returns `InvalidLimits` for a zero represented-work maximum.
    pub fn new(states: PolicyLimits, work_bytes: u64) -> Result<Self, AdmissionError> {
        if work_bytes == 0 {
            return Err(AdmissionError::InvalidLimits);
        }
        Ok(Self { states, work_bytes })
    }

    /// Reads the selected existing core state profile.
    #[must_use]
    pub const fn policy_limits(self) -> PolicyLimits {
        self.states
    }

    /// Reads the immutable represented-work maximum.
    #[must_use]
    pub const fn max_work_bytes(self) -> u64 {
        self.work_bytes
    }
}

/// Continuous logical work and peak scratch for one parent-owned account.
///
/// Matching profiles do not prove accounting identity. The authoritative parent
/// must thread the same account through all operations; there is no reset/debit API.
/// Input construction, refused-owner destruction and native capacity/CPU/RSS are
/// outside this represented estimate and require bounded outer preparation.
pub struct PolicySchemaCharge {
    limits: PolicySchemaLimits,
    g: u64,
    p: u64,
}

impl PolicySchemaCharge {
    /// Begins one empty account under the immutable supplied profile.
    #[must_use]
    pub const fn new(limits: PolicySchemaLimits) -> Self {
        Self { limits, g: 0, p: 0 }
    }

    /// Reads admitted cumulative occurrence work.
    #[must_use]
    pub const fn g(&self) -> u64 {
        self.g
    }

    /// Reads admitted peak logical scratch, retained after temporary sets drop.
    #[must_use]
    pub const fn p(&self) -> u64 {
        self.p
    }

    /// Reads the sum whose nonoverflowing invariant each successful debit proves.
    #[must_use]
    pub const fn total(&self) -> u64 {
        self.g + self.p
    }

    pub(crate) const fn profile(&self) -> PolicySchemaLimits {
        self.limits
    }
}

pub(crate) fn charge_schema(
    charge: &mut PolicySchemaCharge,
    g: u64,
    scratch_p: u64,
) -> Result<(), PolicySchemaError> {
    let next_g = charge
        .g
        .checked_add(g)
        .ok_or(PolicySchemaError::LengthOverflow)?;
    let next_p = charge.p.max(scratch_p);
    let bytes = next_g
        .checked_add(next_p)
        .ok_or(PolicySchemaError::LengthOverflow)?;
    if bytes > charge.limits.work_bytes {
        return Err(PolicySchemaError::WorkBytes {
            bytes,
            maximum: charge.limits.work_bytes,
        });
    }
    charge.g = next_g;
    charge.p = next_p;
    Ok(())
}

pub(super) fn complete_metrics(
    data: &CompletePolicyData,
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<(u64, u64, u64), PolicySchemaError> {
    let start = charge.g();
    charge_schema(charge, 128, 0)?;
    if data.states.len() > limits.policy_limits().max_states() {
        return Err(PolicySchemaError::Policy(PolicyError::StateBudget));
    }
    charge_schema(charge, 128, 0)?;
    let mut bytes = 0_u64;
    let mut largest = 0_u64;
    for (name, state) in std::iter::once((&data.workflow_type, None))
        .chain(data.states.iter().map(|(name, state)| (name, Some(state))))
    {
        if state.is_some() {
            charge_schema(charge, 128, 0)?;
        }
        let (retry, retention, governance) = state.map_or(
            (
                data.retry.as_ref(),
                data.retention.as_ref(),
                data.governance.as_ref(),
            ),
            |state| {
                (
                    state.retry.as_ref(),
                    state.retention.as_ref(),
                    state.governance.as_ref(),
                )
            },
        );
        if let Some(retry) = retry {
            charge_schema(charge, 128, 0)?;
            if retry.backoff.is_some() {
                charge_schema(charge, 128, 0)?;
            }
        }
        if retention.is_some() {
            charge_schema(charge, 128, 0)?;
        }
        for leaf in std::iter::once(name).chain(retry.and_then(|r| r.exhausted_to.as_ref())) {
            let n = u64::try_from(leaf.len()).map_err(|_| PolicySchemaError::LengthOverflow)?;
            bytes = bytes
                .checked_add(n)
                .ok_or(PolicySchemaError::LengthOverflow)?;
            if bytes > 262_144 {
                return Err(PolicySchemaError::SnapshotBytes);
            }
            charge_schema(charge, n, 0)?;
        }
        if let Some(governance) = governance {
            let (_, n, list) = governance_metrics(governance, charge)?;
            bytes = bytes
                .checked_add(n)
                .ok_or(PolicySchemaError::LengthOverflow)?;
            largest = largest.max(list);
            if bytes > 262_144 {
                return Err(PolicySchemaError::SnapshotBytes);
            }
        }
    }
    if let Some(crate::policy::definition::PolicyVersion::Bytes(version)) = &data.version {
        let n = u64::try_from(version.len()).map_err(|_| PolicySchemaError::LengthOverflow)?;
        bytes = bytes
            .checked_add(n)
            .ok_or(PolicySchemaError::LengthOverflow)?;
        if bytes > 262_144 {
            return Err(PolicySchemaError::SnapshotBytes);
        }
        charge_schema(charge, n, 0)?;
    }
    largest = largest.max(indexed_metrics(data, &mut bytes, charge)?);
    Ok(((charge.g() - start - bytes) / 128, bytes, largest))
}

fn indexed_metrics(
    data: &CompletePolicyData,
    bytes: &mut u64,
    charge: &mut PolicySchemaCharge,
) -> Result<u64, PolicySchemaError> {
    let mut largest = 0_u64;
    for (names, attributes) in [
        (data.indexed_attributes.as_deref(), true),
        (
            data.indexed_state_meta.as_ref().map(std::slice::from_ref),
            false,
        ),
    ] {
        if let Some(names) = names {
            if names.len() > 3 {
                return Err(PolicySchemaError::InvalidIndexedAttributes);
            }
            if attributes {
                charge_schema(charge, 128, 0)?;
                largest = largest.max(
                    u64::try_from(names.len()).map_err(|_| PolicySchemaError::LengthOverflow)?,
                );
            }
            for name in names {
                charge_schema(charge, 128, 0)?;
                if name.len() > 64 {
                    return Err(if attributes {
                        PolicySchemaError::InvalidIndexedAttributes
                    } else {
                        PolicySchemaError::InvalidIndexedStateMeta
                    });
                }
                let n = u64::try_from(name.len()).map_err(|_| PolicySchemaError::LengthOverflow)?;
                *bytes = bytes
                    .checked_add(n)
                    .ok_or(PolicySchemaError::LengthOverflow)?;
                if *bytes > 262_144 {
                    return Err(PolicySchemaError::SnapshotBytes);
                }
                charge_schema(charge, n, 0)?;
            }
        }
    }
    Ok(largest)
}

fn governance_metrics(
    data: &GovernanceData,
    charge: &mut PolicySchemaCharge,
) -> Result<(u64, u64, u64), PolicySchemaError> {
    let start = charge.g();
    let mut bytes = 0_u64;
    let mut largest = 0_u64;
    charge_schema(charge, 128, 0)?;
    for present in [
        data.effects.is_some(),
        data.audit.is_some(),
        data.approvals.is_some(),
    ] {
        if present {
            charge_schema(charge, 128, 0)?;
        }
    }
    for (kind, count) in [
        (
            GovernanceRuleKind::Limits,
            data.limits.as_ref().map(BTreeMap::len),
        ),
        (
            GovernanceRuleKind::Budgets,
            data.budgets.as_ref().map(BTreeMap::len),
        ),
        (
            GovernanceRuleKind::Circuits,
            data.circuits.as_ref().map(BTreeMap::len),
        ),
    ] {
        if let Some(count) = count {
            charge_schema(charge, 128, 0)?;
            if count > 1_000 {
                return Err(PolicySchemaError::RuleCount(kind));
            }
        }
    }
    let entries = data
        .limits
        .iter()
        .flat_map(|map| map.keys())
        .map(|key| (GovernanceRuleKind::Limits, key, None, None))
        .chain(
            data.budgets
                .iter()
                .flat_map(|map| map.iter())
                .map(|(key, rule)| (GovernanceRuleKind::Budgets, key, Some(rule), None)),
        )
        .chain(
            data.circuits
                .iter()
                .flat_map(|map| map.iter())
                .map(|(key, rule)| (GovernanceRuleKind::Circuits, key, None, Some(rule))),
        );
    for (kind, key, budget, circuit) in entries {
        charge_schema(charge, 128, 0)?;
        if key.len() > 65_535 {
            return Err(PolicySchemaError::NameBytes(kind));
        }
        let unit = budget.and_then(|rule| rule.unit.as_ref());
        for leaf in std::iter::once(key).chain(unit) {
            if leaf.len() > 65_535 {
                return Err(PolicySchemaError::ListBytes(GovernanceField::Unit));
            }
            let n = u64::try_from(leaf.len()).map_err(|_| PolicySchemaError::LengthOverflow)?;
            bytes = bytes
                .checked_add(n)
                .ok_or(PolicySchemaError::LengthOverflow)?;
            if bytes > 1_048_576 {
                return Err(PolicySchemaError::GovernanceBytes);
            }
            charge_schema(charge, n, 0)?;
        }
        if let Some(classes) = circuit.and_then(|rule| rule.error_classes.as_deref()) {
            let count = list_metrics(classes, GovernanceField::ErrorClasses, &mut bytes, charge)?;
            largest = largest.max(count);
        }
    }
    largest = largest.max(governance_list_metrics(data, &mut bytes, charge)?);
    Ok(((charge.g() - start - bytes) / 128, bytes, largest))
}

fn governance_list_metrics(
    data: &GovernanceData,
    bytes: &mut u64,
    charge: &mut PolicySchemaCharge,
) -> Result<u64, PolicySchemaError> {
    let mut largest = 0_u64;
    for (field, names) in [
        (
            GovernanceField::Allowed,
            data.effects.as_ref().and_then(|d| d.allowed.as_deref()),
        ),
        (
            GovernanceField::Denied,
            data.effects.as_ref().and_then(|d| d.denied.as_deref()),
        ),
        (
            GovernanceField::ApprovalEffects,
            data.approvals.as_ref().and_then(|d| d.effects.as_deref()),
        ),
        (
            GovernanceField::ApprovalStates,
            data.approvals.as_ref().and_then(|d| d.states.as_deref()),
        ),
        (
            GovernanceField::Assignees,
            data.approvals.as_ref().and_then(|d| d.assignees.as_deref()),
        ),
    ] {
        if let Some(names) = names {
            largest = largest.max(list_metrics(names, field, bytes, charge)?);
        }
    }
    Ok(largest)
}

fn list_metrics(
    names: &[Vec<u8>],
    field: GovernanceField,
    bytes: &mut u64,
    charge: &mut PolicySchemaCharge,
) -> Result<u64, PolicySchemaError> {
    if names.len() > 1_000 {
        return Err(PolicySchemaError::ListCount(field));
    }
    charge_schema(charge, 128, 0)?;
    let count = u64::try_from(names.len()).map_err(|_| PolicySchemaError::LengthOverflow)?;
    let maximum = if field == GovernanceField::ErrorClasses {
        256
    } else {
        65_535
    };
    for name in names {
        charge_schema(charge, 128, 0)?;
        if name.len() > maximum {
            return Err(PolicySchemaError::ListBytes(field));
        }
        let n = u64::try_from(name.len()).map_err(|_| PolicySchemaError::LengthOverflow)?;
        *bytes = bytes
            .checked_add(n)
            .ok_or(PolicySchemaError::LengthOverflow)?;
        if *bytes > 1_048_576 {
            return Err(PolicySchemaError::GovernanceBytes);
        }
        charge_schema(charge, n, 0)?;
    }
    Ok(count)
}
