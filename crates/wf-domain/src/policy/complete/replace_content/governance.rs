//! One reached governance walk in source field, rule and circuit priority.

use std::collections::{BTreeMap, BTreeSet};

use super::super::{
    BudgetData, CircuitData, CircuitTrackingFailure, GovernanceData, GovernanceField,
    GovernanceRuleKind, LimitData, PolicySchemaCharge, PolicySchemaError,
};
use super::{CompletePolicyReplaceRejectionKind as Kind, charge_schema, charge_visit};

const MAX_EXACT_INTEGER: u64 = 9_007_199_254_740_991;

pub(super) fn check_replace_governance(
    data: &GovernanceData,
    charge: &mut PolicySchemaCharge,
) -> Result<Option<Kind>, PolicySchemaError> {
    charge_schema(charge, 128, 0)?;
    // Governance mode, booleans, audit tags and enforcement are valid typed enums.
    if let Some(effects) = &data.effects {
        charge_schema(charge, 128, 0)?;
        for (field, names) in [
            (GovernanceField::Allowed, effects.allowed.as_deref()),
            (GovernanceField::Denied, effects.denied.as_deref()),
        ] {
            if let Some(kind) = check_replace_names(names, field, charge)? {
                return Ok(Some(kind));
            }
        }
    }
    if data.audit.is_some() {
        charge_schema(charge, 128, 0)?;
    }
    if let Some(kind) = check_replace_rules(
        data.limits.as_ref(),
        GovernanceRuleKind::Limits,
        charge,
        |rule, _| Ok(check_replace_limit(rule)),
    )? {
        return Ok(Some(kind));
    }
    if let Some(kind) = check_replace_rules(
        data.budgets.as_ref(),
        GovernanceRuleKind::Budgets,
        charge,
        check_replace_budget,
    )? {
        return Ok(Some(kind));
    }
    if let Some(kind) = check_replace_rules(
        data.circuits.as_ref(),
        GovernanceRuleKind::Circuits,
        charge,
        check_replace_circuit,
    )? {
        return Ok(Some(kind));
    }
    if let Some(approvals) = &data.approvals {
        charge_schema(charge, 128, 0)?;
        for (field, names) in [
            (
                GovernanceField::ApprovalEffects,
                approvals.effects.as_deref(),
            ),
            (GovernanceField::ApprovalStates, approvals.states.as_deref()),
            (GovernanceField::Assignees, approvals.assignees.as_deref()),
        ] {
            if let Some(kind) = check_replace_names(names, field, charge)? {
                return Ok(Some(kind));
            }
        }
        return Ok(check_positive(
            approvals.timeout_ms,
            GovernanceField::ApprovalTimeout,
        ));
    }
    Ok(None)
}

fn check_replace_rules<T>(
    rules: Option<&BTreeMap<Vec<u8>, T>>,
    kind: GovernanceRuleKind,
    charge: &mut PolicySchemaCharge,
    check: impl Fn(&T, &mut PolicySchemaCharge) -> Result<Option<Kind>, PolicySchemaError>,
) -> Result<Option<Kind>, PolicySchemaError> {
    let Some(rules) = rules else {
        return Ok(None);
    };
    charge_schema(charge, 128, 0)?;
    for (name, rule) in rules {
        charge_visit(charge, name.len())?;
        if name.is_empty() {
            return Ok(Some(Kind::GovernanceEmptyName(kind)));
        }
        if let Some(kind) = check(rule, charge)? {
            return Ok(Some(kind));
        }
    }
    Ok(None)
}

fn check_replace_names(
    names: Option<&[Vec<u8>]>,
    field: GovernanceField,
    charge: &mut PolicySchemaCharge,
) -> Result<Option<Kind>, PolicySchemaError> {
    let Some(names) = names else {
        return Ok(None);
    };
    charge_schema(charge, 128, 0)?;
    // The entry admitted measured peak scratch before any borrowed-set insertion.
    let mut seen = BTreeSet::new();
    for name in names {
        charge_visit(charge, name.len())?;
        if name.is_empty() {
            return Ok(Some(Kind::GovernanceEmptyList(field)));
        }
        if !seen.insert(name.as_slice()) {
            return Err(PolicySchemaError::NonCanonicalList(field));
        }
    }
    Ok(None)
}

fn check_replace_limit(rule: &LimitData) -> Option<Kind> {
    if rule.limit > MAX_EXACT_INTEGER {
        return Some(Kind::GovernanceLimit);
    }
    check_positive(rule.lease_size, GovernanceField::LeaseSize)
}

fn check_replace_budget(
    rule: &BudgetData,
    charge: &mut PolicySchemaCharge,
) -> Result<Option<Kind>, PolicySchemaError> {
    if rule.limit > MAX_EXACT_INTEGER {
        return Ok(Some(Kind::GovernanceLimit));
    }
    for (field, value) in [
        (GovernanceField::WindowMillis, Some(rule.window_ms)),
        (GovernanceField::LeaseSize, rule.lease_size),
    ] {
        if let Some(kind) = check_positive(value, field) {
            return Ok(Some(kind));
        }
    }
    if let Some(unit) = &rule.unit {
        let bytes = u64::try_from(unit.len()).map_err(|_| PolicySchemaError::LengthOverflow)?;
        charge_schema(charge, bytes, 0)?;
        if unit.is_empty() {
            return Ok(Some(Kind::GovernanceUnit));
        }
    }
    Ok(None)
}

fn check_replace_circuit(
    rule: &CircuitData,
    charge: &mut PolicySchemaCharge,
) -> Result<Option<Kind>, PolicySchemaError> {
    for (field, value) in [
        (
            GovernanceField::FailureThreshold,
            Some(rule.failure_threshold),
        ),
        (GovernanceField::OpenMillis, Some(rule.open_ms)),
        (GovernanceField::WindowMillis, rule.window_ms),
        (GovernanceField::MinCalls, rule.min_calls),
    ] {
        if let Some(kind) = check_positive(value, field) {
            return Ok(Some(kind));
        }
    }
    if rule
        .failure_rate_pct
        .is_some_and(|rate| !(1..=100).contains(&rate))
    {
        return Ok(Some(Kind::GovernancePercent));
    }
    if let Some(kind) = check_positive(rule.latency_threshold_ms, GovernanceField::LatencyMillis) {
        return Ok(Some(kind));
    }
    if let Some(kind) = check_replace_names(
        rule.error_classes.as_deref(),
        GovernanceField::ErrorClasses,
        charge,
    )? {
        return Ok(Some(kind));
    }
    for (field, value) in [
        (GovernanceField::HalfOpenProbes, rule.half_open_max_probes),
        (
            GovernanceField::HalfOpenSuccesses,
            rule.half_open_success_threshold,
        ),
    ] {
        if let Some(kind) = check_positive(value, field) {
            return Ok(Some(kind));
        }
    }
    let tracking = if rule.min_calls.is_some_and(|calls| calls > 64) {
        Some(CircuitTrackingFailure::MinCallsAbove64)
    } else if rule.failure_threshold > 64 && rule.failure_rate_pct.is_none() {
        Some(CircuitTrackingFailure::ThresholdAbove64WithoutRate)
    } else if rule.failure_threshold > 64 && rule.min_calls.is_none() {
        Some(CircuitTrackingFailure::MissingMinCallsForLargeThreshold)
    } else {
        None
    };
    Ok(tracking.map(Kind::GovernanceTracking))
}

fn check_positive(value: Option<u64>, field: GovernanceField) -> Option<Kind> {
    value
        .is_some_and(|value| value == 0 || value > MAX_EXACT_INTEGER)
        .then_some(Kind::GovernancePositive(field))
}
