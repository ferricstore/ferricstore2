//! Repeated normalized-content passes sharing one already selected charge.

use std::collections::BTreeSet;

use crate::policy::definition::PolicyVersion;
use crate::policy::installation::{PolicyError, PolicyLimits};
use crate::retention::MAX_HISTORY_EVENTS;
use crate::value::{DomainError, InputField, RetryPolicyField};

use super::admission::{charge_schema, complete_metrics};
use super::{
    CompletePolicyData, GovernanceData, GovernanceField, GovernanceRuleKind, PolicySchemaCharge,
    PolicySchemaError, PolicySchemaLimits,
};

mod rules;

pub(super) fn admit_complete(
    data: &CompletePolicyData,
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
    canonical: bool,
) -> Result<(), PolicySchemaError> {
    let (rows, bytes, largest) = complete_metrics(data, limits, charge)?;
    let work = rows
        .checked_mul(128)
        .and_then(|n| n.checked_add(bytes))
        .ok_or(PolicySchemaError::LengthOverflow)?;
    charge_schema(charge, work, 0)?;
    validate_complete_core(data, limits.policy_limits())?;
    if canonical {
        charge_schema(
            charge,
            0,
            largest
                .checked_mul(64)
                .ok_or(PolicySchemaError::LengthOverflow)?,
        )?;
    }
    if let Some(governance) = &data.governance {
        admit_governance(governance, charge, canonical)?;
    }
    charge_schema(charge, 128, 0)?;
    for (name, state) in &data.states {
        let n = u64::try_from(name.len()).map_err(|_| PolicySchemaError::LengthOverflow)?;
        charge_schema(
            charge,
            n.checked_add(128)
                .ok_or(PolicySchemaError::LengthOverflow)?,
            0,
        )?;
        if let Some(governance) = &state.governance {
            admit_governance(governance, charge, canonical)?;
        }
    }
    validate_indexed(data, charge, canonical)
}

fn admit_governance(
    data: &GovernanceData,
    charge: &mut PolicySchemaCharge,
    canonical: bool,
) -> Result<(), PolicySchemaError> {
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
    let effects = data.effects.as_ref();
    let approvals = data.approvals.as_ref();
    for (field, names) in [
        (
            GovernanceField::Allowed,
            effects.and_then(|d| d.allowed.as_deref()),
        ),
        (
            GovernanceField::Denied,
            effects.and_then(|d| d.denied.as_deref()),
        ),
        (
            GovernanceField::ApprovalEffects,
            approvals.and_then(|d| d.effects.as_deref()),
        ),
        (
            GovernanceField::ApprovalStates,
            approvals.and_then(|d| d.states.as_deref()),
        ),
        (
            GovernanceField::Assignees,
            approvals.and_then(|d| d.assignees.as_deref()),
        ),
    ] {
        if let Some(names) = names {
            validate_names(names, field, charge, canonical)?;
        }
    }
    if data
        .approvals
        .as_ref()
        .and_then(|d| d.timeout_ms)
        .is_some_and(|timeout| timeout == 0 || timeout > 9_007_199_254_740_991)
    {
        return Err(PolicySchemaError::InvalidInteger(
            GovernanceField::ApprovalTimeout,
        ));
    }
    admit_governance_rules(data, charge, canonical)?;
    Ok(())
}

fn admit_governance_rules(
    data: &GovernanceData,
    charge: &mut PolicySchemaCharge,
    canonical: bool,
) -> Result<(), PolicySchemaError> {
    for present in [
        data.limits.is_some(),
        data.budgets.is_some(),
        data.circuits.is_some(),
    ] {
        if present {
            charge_schema(charge, 128, 0)?;
        }
    }
    let entries = data
        .limits
        .iter()
        .flat_map(|map| map.iter())
        .map(|(key, rule)| (GovernanceRuleKind::Limits, key, Some(rule), None, None))
        .chain(
            data.budgets
                .iter()
                .flat_map(|map| map.iter())
                .map(|(key, rule)| (GovernanceRuleKind::Budgets, key, None, Some(rule), None)),
        )
        .chain(
            data.circuits
                .iter()
                .flat_map(|map| map.iter())
                .map(|(key, rule)| (GovernanceRuleKind::Circuits, key, None, None, Some(rule))),
        );
    for (kind, key, limit, budget, circuit) in entries {
        let n = u64::try_from(key.len()).map_err(|_| PolicySchemaError::LengthOverflow)?;
        charge_schema(
            charge,
            n.checked_add(128)
                .ok_or(PolicySchemaError::LengthOverflow)?,
            0,
        )?;
        rules::validate_rule_name(key, kind)?;
        if let Some(rule) = limit {
            rules::validate_limit(rule)?;
        }
        if let Some(rule) = budget {
            if let Some(unit) = &rule.unit {
                charge_schema(
                    charge,
                    u64::try_from(unit.len()).map_err(|_| PolicySchemaError::LengthOverflow)?,
                    0,
                )?;
            }
            rules::validate_budget(rule)?;
        }
        if let Some(rule) = circuit {
            if let Some(classes) = &rule.error_classes {
                validate_names(classes, GovernanceField::ErrorClasses, charge, canonical)?;
            }
            rules::validate_circuit(rule)?;
        }
    }
    Ok(())
}

fn validate_complete_core(
    data: &CompletePolicyData,
    limits: PolicyLimits,
) -> Result<(), PolicySchemaError> {
    if data.workflow_type.is_empty() {
        return Err(PolicySchemaError::Policy(PolicyError::Domain(
            DomainError::EmptyValue(InputField::Type),
        )));
    }
    if matches!(&data.version, Some(PolicyVersion::Bytes(bytes)) if bytes.is_empty()) {
        return Err(PolicySchemaError::Policy(PolicyError::InvalidVersion));
    }
    if data
        .max_active_ms
        .is_some_and(|active| active == 0 || active > 31_536_000_000)
    {
        return Err(PolicySchemaError::Policy(PolicyError::InvalidMaxActive));
    }
    if data.states.len() > limits.max_states() {
        return Err(PolicySchemaError::Policy(PolicyError::StateBudget));
    }
    if let Some(retry) = &data.retry {
        retry
            .validate()
            .map_err(|error| PolicySchemaError::Policy(PolicyError::Domain(error)))?;
    }
    if let Some(retention) = data.retention {
        retention
            .validate(MAX_HISTORY_EVENTS)
            .map_err(|error| PolicySchemaError::Policy(PolicyError::Retention(error)))?;
    }
    for (state, policy) in &data.states {
        if state.is_empty() {
            return Err(PolicySchemaError::Policy(PolicyError::Domain(
                DomainError::InvalidRetryPolicy(RetryPolicyField::State),
            )));
        }
        if state == b"running" {
            return Err(PolicySchemaError::Policy(PolicyError::Domain(
                DomainError::InvalidRetryPolicy(RetryPolicyField::RunningState),
            )));
        }
        if let Some(retry) = &policy.retry {
            retry
                .validate()
                .map_err(|error| PolicySchemaError::Policy(PolicyError::Domain(error)))?;
        }
        if let Some(retention) = policy.retention {
            retention
                .validate(MAX_HISTORY_EVENTS)
                .map_err(|error| PolicySchemaError::Policy(PolicyError::Retention(error)))?;
        }
    }
    Ok(())
}

fn validate_names(
    names: &[Vec<u8>],
    field: GovernanceField,
    charge: &mut PolicySchemaCharge,
    canonical: bool,
) -> Result<(), PolicySchemaError> {
    if names.len() > 1_000 {
        return Err(PolicySchemaError::ListCount(field));
    }
    charge_schema(charge, 128, 0)?;
    let mut seen = canonical.then(BTreeSet::new);
    let maximum = if field == GovernanceField::ErrorClasses {
        256
    } else {
        65_535
    };
    for name in names {
        if name.len() > maximum {
            return Err(PolicySchemaError::ListBytes(field));
        }
        let n = u64::try_from(name.len()).map_err(|_| PolicySchemaError::LengthOverflow)?;
        charge_schema(
            charge,
            n.checked_add(128)
                .ok_or(PolicySchemaError::LengthOverflow)?,
            0,
        )?;
        if name.is_empty() {
            return Err(PolicySchemaError::InvalidListValue(field));
        }
        if let Some(seen) = &mut seen
            && !seen.insert(name.as_slice())
        {
            return Err(PolicySchemaError::NonCanonicalList(field));
        }
    }
    Ok(())
}

fn validate_indexed(
    data: &CompletePolicyData,
    charge: &mut PolicySchemaCharge,
    canonical: bool,
) -> Result<(), PolicySchemaError> {
    for (names, attributes) in [
        (data.indexed_attributes.as_deref(), true),
        (
            data.indexed_state_meta.as_ref().map(std::slice::from_ref),
            false,
        ),
    ] {
        let error = if attributes {
            PolicySchemaError::InvalidIndexedAttributes
        } else {
            PolicySchemaError::InvalidIndexedStateMeta
        };
        let Some(names) = names else { continue };
        if names.len() > if attributes { 3 } else { 1 } {
            return Err(error);
        }
        if attributes {
            charge_schema(charge, 128, 0)?;
        }
        let mut seen = (canonical && attributes).then(BTreeSet::new);
        for name in names {
            charge_schema(charge, 128, 0)?;
            if name.len() > 64 {
                return Err(error);
            }
            let n = u64::try_from(name.len()).map_err(|_| PolicySchemaError::LengthOverflow)?;
            let work = n
                .checked_add(128)
                .ok_or(PolicySchemaError::LengthOverflow)?;
            charge_schema(charge, work, 0)?;
            let text = std::str::from_utf8(name).map_err(|_| error)?;
            charge_schema(charge, work, 0)?;
            let trimmed = text.trim_matches(|ch| matches!(ch,
                '\u{0009}'..='\u{000d}' | '\u{0020}' | '\u{0085}' | '\u{00a0}' | '\u{1680}'
                | '\u{2000}'..='\u{200a}' | '\u{2028}' | '\u{2029}' | '\u{202f}' | '\u{205f}' | '\u{3000}'
            ));
            if trimmed.is_empty() || trimmed.len() != name.len() {
                return Err(error);
            }
            charge_schema(charge, work, 0)?;
            if name.starts_with(b"__") {
                return Err(error);
            }
            let duplicate = if let Some(seen) = &mut seen {
                charge_schema(charge, work, 0)?;
                !seen.insert(name.as_slice())
            } else {
                false
            };
            if duplicate {
                return Err(error);
            }
        }
    }
    Ok(())
}
