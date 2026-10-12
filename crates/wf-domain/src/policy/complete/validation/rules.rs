//! Scalar and opaque rule-name checks after their caller's occurrence admission.

use super::super::{
    BudgetData, CircuitData, CircuitTrackingFailure, GovernanceField, GovernanceRuleKind,
    LimitData, PolicySchemaError,
};

pub(super) fn validate_rule_name(
    name: &[u8],
    kind: GovernanceRuleKind,
) -> Result<(), PolicySchemaError> {
    if name.is_empty() {
        return Err(PolicySchemaError::InvalidName(kind));
    }
    if name.len() > 65_535 {
        return Err(PolicySchemaError::NameBytes(kind));
    }
    Ok(())
}

pub(super) fn validate_limit(rule: &LimitData) -> Result<(), PolicySchemaError> {
    if rule.limit > 9_007_199_254_740_991 {
        return Err(PolicySchemaError::InvalidInteger(GovernanceField::Limit));
    }
    if rule
        .lease_size
        .is_some_and(|size| size == 0 || size > 9_007_199_254_740_991)
    {
        return Err(PolicySchemaError::InvalidInteger(
            GovernanceField::LeaseSize,
        ));
    }
    Ok(())
}

pub(super) fn validate_budget(rule: &BudgetData) -> Result<(), PolicySchemaError> {
    if rule.limit > 9_007_199_254_740_991 {
        return Err(PolicySchemaError::InvalidInteger(GovernanceField::Limit));
    }
    if rule.window_ms == 0 || rule.window_ms > 9_007_199_254_740_991 {
        return Err(PolicySchemaError::InvalidInteger(
            GovernanceField::WindowMillis,
        ));
    }
    if rule
        .lease_size
        .is_some_and(|size| size == 0 || size > 9_007_199_254_740_991)
    {
        return Err(PolicySchemaError::InvalidInteger(
            GovernanceField::LeaseSize,
        ));
    }
    if let Some(unit) = &rule.unit {
        if unit.is_empty() {
            return Err(PolicySchemaError::InvalidListValue(GovernanceField::Unit));
        }
        if unit.len() > 65_535 {
            return Err(PolicySchemaError::ListBytes(GovernanceField::Unit));
        }
    }
    Ok(())
}

pub(super) fn validate_circuit(rule: &CircuitData) -> Result<(), PolicySchemaError> {
    for (field, value) in [
        (
            GovernanceField::FailureThreshold,
            Some(rule.failure_threshold),
        ),
        (GovernanceField::OpenMillis, Some(rule.open_ms)),
        (GovernanceField::WindowMillis, rule.window_ms),
        (GovernanceField::MinCalls, rule.min_calls),
        (GovernanceField::LatencyMillis, rule.latency_threshold_ms),
        (GovernanceField::HalfOpenProbes, rule.half_open_max_probes),
        (
            GovernanceField::HalfOpenSuccesses,
            rule.half_open_success_threshold,
        ),
    ] {
        if value.is_some_and(|value| value == 0 || value > 9_007_199_254_740_991) {
            return Err(PolicySchemaError::InvalidInteger(field));
        }
    }
    if rule
        .failure_rate_pct
        .is_some_and(|rate| !(1..=100).contains(&rate))
    {
        return Err(PolicySchemaError::InvalidPercent);
    }
    if rule.min_calls.is_some_and(|calls| calls > 64) {
        return Err(PolicySchemaError::CircuitTracking(
            CircuitTrackingFailure::MinCallsAbove64,
        ));
    }
    if rule.failure_threshold > 64 {
        if rule.failure_rate_pct.is_none() {
            return Err(PolicySchemaError::CircuitTracking(
                CircuitTrackingFailure::ThresholdAbove64WithoutRate,
            ));
        }
        if rule.min_calls.is_none() {
            return Err(PolicySchemaError::CircuitTracking(
                CircuitTrackingFailure::MissingMinCallsForLargeThreshold,
            ));
        }
    }
    Ok(())
}
