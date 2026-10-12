//! Fixed governance/indexed source bodies and deliberately different priorities.

use std::collections::BTreeMap;

use wf_domain::policy::complete::{
    ApprovalsData, BudgetData, CircuitData, CircuitTrackingFailure, CompletePolicyData,
    CompletePolicyReplaceRejectionKind as Kind, EffectsData, GovernanceField, GovernanceRuleKind,
    LimitData, PolicySchemaCharge, PolicySchemaError, PolicySchemaLimits,
    check_complete_policy_replace_content,
};
use wf_domain::policy::definition::PolicyVersion;
use wf_domain::policy::installation::PolicyError;
use wf_domain::policy::{Backoff, RetryOverride};
use wf_domain::retention::{RetentionError, RetentionOverride};
use wf_domain::value::{DomainError, RetryPolicyField};

use super::refusals::assert_indexed_bodies;
use super::{
    TestResult, assert_list_bodies, assert_rejection, empty_governance, limits, minimal, state,
};

#[test]
fn replace_content_returns_exact_reachable_governance_and_indexed_bodies() -> TestResult {
    let profile = limits(128 * 1_024 * 1_024)?;
    let mut charge = PolicySchemaCharge::new(profile);
    assert_rule_names_and_quantities(profile, &mut charge)?;
    assert_positive_bodies(profile, &mut charge)?;
    assert_list_bodies(profile, &mut charge)?;
    assert_unit_rate_and_tracking(profile, &mut charge)?;
    assert_indexed_bodies(profile, &mut charge)?;
    Ok(())
}

fn circuit() -> CircuitData {
    CircuitData {
        failure_threshold: 1,
        open_ms: 1,
        window_ms: None,
        min_calls: None,
        failure_rate_pct: None,
        latency_threshold_ms: None,
        half_open_max_probes: None,
        half_open_success_threshold: None,
        error_classes: None,
    }
}

pub(super) fn rule_input(kind: GovernanceRuleKind, name: &[u8]) -> CompletePolicyData {
    let mut supplied = minimal();
    let mut governance = empty_governance();
    match kind {
        GovernanceRuleKind::Limits => {
            governance.limits = Some(BTreeMap::from([(
                name.to_vec(),
                LimitData {
                    limit: 0,
                    enforcement: None,
                    lease_size: None,
                },
            )]));
        }
        GovernanceRuleKind::Budgets => {
            governance.budgets = Some(BTreeMap::from([(
                name.to_vec(),
                BudgetData {
                    limit: 0,
                    window_ms: 1,
                    enforcement: None,
                    lease_size: None,
                    unit: None,
                },
            )]));
        }
        GovernanceRuleKind::Circuits => {
            governance.circuits = Some(BTreeMap::from([(name.to_vec(), circuit())]));
        }
    }
    supplied.governance = Some(governance);
    supplied
}

fn assert_rule_names_and_quantities(
    profile: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> TestResult {
    for (kind, body) in [
        (
            GovernanceRuleKind::Limits,
            b"ERR flow governance limits names must be non-empty strings".as_slice(),
        ),
        (
            GovernanceRuleKind::Budgets,
            b"ERR flow governance budgets names must be non-empty strings".as_slice(),
        ),
        (
            GovernanceRuleKind::Circuits,
            b"ERR flow governance circuits names must be non-empty strings".as_slice(),
        ),
    ] {
        let supplied = rule_input(kind, b"");
        let expected: (Kind, PolicySchemaError, &[u8]) = (
            Kind::GovernanceEmptyName(kind),
            PolicySchemaError::InvalidName(kind),
            body,
        );
        assert_rejection(&supplied, 17, expected, profile, charge)?;
        assert!(supplied == rule_input(kind, b""));
        let opaque = rule_input(kind, &[0, 255]);
        assert!(check_complete_policy_replace_content(&opaque, 17, profile, charge)?.is_none());
        assert!(opaque == rule_input(kind, &[0, 255]));
    }
    for kind in [GovernanceRuleKind::Limits, GovernanceRuleKind::Budgets] {
        for quantity in [0, 1, 9_007_199_254_740_991, 9_007_199_254_740_992, u64::MAX] {
            let mut supplied = rule_input(kind, b"q");
            let governance = supplied.governance.as_mut().ok_or("governance")?;
            match kind {
                GovernanceRuleKind::Limits => {
                    governance
                        .limits
                        .as_mut()
                        .ok_or("limits")?
                        .get_mut(b"q".as_slice())
                        .ok_or("rule")?
                        .limit = quantity;
                }
                GovernanceRuleKind::Budgets => {
                    governance
                        .budgets
                        .as_mut()
                        .ok_or("budgets")?
                        .get_mut(b"q".as_slice())
                        .ok_or("rule")?
                        .limit = quantity;
                }
                GovernanceRuleKind::Circuits => return Err("quantity fixture".into()),
            }
            if quantity <= 9_007_199_254_740_991 {
                assert!(
                    check_complete_policy_replace_content(&supplied, 17, profile, charge)?
                        .is_none()
                );
            } else {
                let expected: (Kind, PolicySchemaError, &[u8]) = (
                    Kind::GovernanceLimit,
                    PolicySchemaError::InvalidInteger(GovernanceField::Limit),
                    b"ERR flow governance limit must be a non-negative integer",
                );
                assert_rejection(&supplied, 17, expected, profile, charge)?;
            }
        }
    }
    Ok(())
}

fn positive_input(field: GovernanceField, value: u64) -> TestResult<CompletePolicyData> {
    let mut supplied = minimal();
    let mut governance = empty_governance();
    match field {
        GovernanceField::LeaseSize => {
            governance.limits = Some(BTreeMap::from([(
                b"q".to_vec(),
                LimitData {
                    limit: 0,
                    enforcement: None,
                    lease_size: Some(value),
                },
            )]));
        }
        GovernanceField::ApprovalTimeout => {
            governance.approvals = Some(ApprovalsData {
                effects: None,
                states: None,
                assignees: None,
                timeout_ms: Some(value),
            });
        }
        _ => {
            let mut rule = circuit();
            match field {
                GovernanceField::WindowMillis => rule.window_ms = Some(value),
                GovernanceField::FailureThreshold => {
                    rule.failure_threshold = value;
                    rule.failure_rate_pct = Some(1);
                    rule.min_calls = Some(1);
                }
                GovernanceField::OpenMillis => rule.open_ms = value,
                GovernanceField::MinCalls => rule.min_calls = Some(value),
                GovernanceField::LatencyMillis => rule.latency_threshold_ms = Some(value),
                GovernanceField::HalfOpenProbes => rule.half_open_max_probes = Some(value),
                GovernanceField::HalfOpenSuccesses => {
                    rule.half_open_success_threshold = Some(value);
                }
                _ => return Err("unknown positive fixture".into()),
            }
            governance.circuits = Some(BTreeMap::from([(b"q".to_vec(), rule)]));
        }
    }
    supplied.governance = Some(governance);
    Ok(supplied)
}

fn assert_positive_bodies(
    profile: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> TestResult {
    let rows = [
        (
            GovernanceField::LeaseSize,
            b"ERR flow governance lease_size must be a positive integer".as_slice(),
        ),
        (
            GovernanceField::WindowMillis,
            b"ERR flow governance window_ms must be a positive integer".as_slice(),
        ),
        (
            GovernanceField::FailureThreshold,
            b"ERR flow governance failure_threshold must be a positive integer".as_slice(),
        ),
        (
            GovernanceField::OpenMillis,
            b"ERR flow governance open_ms must be a positive integer".as_slice(),
        ),
        (
            GovernanceField::MinCalls,
            b"ERR flow governance min_calls must be a positive integer".as_slice(),
        ),
        (
            GovernanceField::LatencyMillis,
            b"ERR flow governance latency_threshold_ms must be a positive integer".as_slice(),
        ),
        (
            GovernanceField::HalfOpenProbes,
            b"ERR flow governance half_open_max_probes must be a positive integer".as_slice(),
        ),
        (
            GovernanceField::HalfOpenSuccesses,
            b"ERR flow governance half_open_success_threshold must be a positive integer"
                .as_slice(),
        ),
        (
            GovernanceField::ApprovalTimeout,
            b"ERR flow governance timeout_ms must be a positive integer".as_slice(),
        ),
    ];
    for (field, body) in rows {
        for value in [0, 9_007_199_254_740_992, u64::MAX] {
            let supplied = positive_input(field, value)?;
            let expected: (Kind, PolicySchemaError, &[u8]) = (
                Kind::GovernancePositive(field),
                PolicySchemaError::InvalidInteger(field),
                body,
            );
            assert_rejection(&supplied, 17, expected, profile, charge)?;
            assert!(supplied == positive_input(field, value)?);
        }
        let maximum = if field == GovernanceField::MinCalls {
            64
        } else {
            9_007_199_254_740_991
        };
        for value in [1, maximum] {
            let supplied = positive_input(field, value)?;
            assert!(
                check_complete_policy_replace_content(&supplied, 17, profile, charge)?.is_none()
            );
            assert!(supplied == positive_input(field, value)?);
        }
    }
    for (field, value) in [
        (GovernanceField::WindowMillis, 0),
        (GovernanceField::LeaseSize, 0),
    ] {
        let mut supplied = rule_input(GovernanceRuleKind::Budgets, b"q");
        let rule = budget_rule(&mut supplied)?;
        if field == GovernanceField::WindowMillis {
            rule.window_ms = value;
        } else {
            rule.lease_size = Some(value);
        }
        let body = if field == GovernanceField::WindowMillis {
            b"ERR flow governance window_ms must be a positive integer".as_slice()
        } else {
            b"ERR flow governance lease_size must be a positive integer".as_slice()
        };
        let expected: (Kind, PolicySchemaError, &[u8]) = (
            Kind::GovernancePositive(field),
            PolicySchemaError::InvalidInteger(field),
            body,
        );
        assert_rejection(&supplied, 17, expected, profile, charge)?;
    }
    Ok(())
}

pub(super) fn list_input(
    field: GovernanceField,
    names: Vec<Vec<u8>>,
) -> TestResult<CompletePolicyData> {
    let mut supplied = minimal();
    let mut governance = empty_governance();
    match field {
        GovernanceField::Allowed | GovernanceField::Denied => {
            governance.effects = Some(EffectsData {
                allowed: (field == GovernanceField::Allowed).then_some(names.clone()),
                denied: (field == GovernanceField::Denied).then_some(names),
                require_idempotency_key: Some(false),
            });
        }
        GovernanceField::ApprovalEffects
        | GovernanceField::ApprovalStates
        | GovernanceField::Assignees => {
            governance.approvals = Some(ApprovalsData {
                effects: (field == GovernanceField::ApprovalEffects).then_some(names.clone()),
                states: (field == GovernanceField::ApprovalStates).then_some(names.clone()),
                assignees: (field == GovernanceField::Assignees).then_some(names),
                timeout_ms: None,
            });
        }
        GovernanceField::ErrorClasses => {
            let mut rule = circuit();
            rule.error_classes = Some(names);
            governance.circuits = Some(BTreeMap::from([(b"q".to_vec(), rule)]));
        }
        _ => return Err("unknown list fixture".into()),
    }
    supplied.governance = Some(governance);
    Ok(supplied)
}

fn assert_unit_rate_and_tracking(
    profile: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> TestResult {
    for unit in [Vec::new(), vec![255], vec![0; 65_535]] {
        let mut supplied = rule_input(GovernanceRuleKind::Budgets, b"q");
        budget_rule(&mut supplied)?.unit = Some(unit.clone());
        if unit.is_empty() {
            let expected: (Kind, PolicySchemaError, &[u8]) = (
                Kind::GovernanceUnit,
                PolicySchemaError::InvalidListValue(GovernanceField::Unit),
                b"ERR flow governance unit must be a non-empty string of at most 65535 bytes",
            );
            assert_rejection(&supplied, 17, expected, profile, charge)?;
        } else {
            assert!(
                check_complete_policy_replace_content(&supplied, 17, profile, charge)?.is_none()
            );
        }
        assert_eq!(
            supplied
                .governance
                .as_ref()
                .ok_or("governance")?
                .budgets
                .as_ref()
                .ok_or("budgets")?[b"q".as_slice()]
            .unit,
            Some(unit)
        );
    }
    for rate in [0, 1, 100, 101, u8::MAX] {
        let mut supplied = rule_input(GovernanceRuleKind::Circuits, b"q");
        circuit_rule(&mut supplied)?.failure_rate_pct = Some(rate);
        if (1..=100).contains(&rate) {
            assert!(
                check_complete_policy_replace_content(&supplied, 17, profile, charge)?.is_none()
            );
        } else {
            let expected: (Kind, PolicySchemaError, &[u8]) = (
                Kind::GovernancePercent,
                PolicySchemaError::InvalidPercent,
                b"ERR flow governance failure_rate_pct must be an integer from 1 to 100",
            );
            assert_rejection(&supplied, 17, expected, profile, charge)?;
        }
    }
    assert_tracking_bodies(profile, charge)
}

fn assert_tracking_bodies(
    profile: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> TestResult {
    let rows = [
        (
            64,
            Some(65),
            Some(1),
            CircuitTrackingFailure::MinCallsAbove64,
            b"ERR flow governance circuit min_calls must be <= 64".as_slice(),
        ),
        (
            65,
            Some(64),
            None,
            CircuitTrackingFailure::ThresholdAbove64WithoutRate,
            b"ERR flow governance circuit failure_threshold must be <= 64 without failure_rate_pct"
                .as_slice(),
        ),
        (
            65,
            None,
            Some(1),
            CircuitTrackingFailure::MissingMinCallsForLargeThreshold,
            b"ERR flow governance circuit min_calls is required when failure_threshold exceeds 64"
                .as_slice(),
        ),
    ];
    for (threshold, calls, rate, reason, body) in rows {
        let mut supplied = rule_input(GovernanceRuleKind::Circuits, b"q");
        let rule = circuit_rule(&mut supplied)?;
        rule.failure_threshold = threshold;
        rule.min_calls = calls;
        rule.failure_rate_pct = rate;
        let expected: (Kind, PolicySchemaError, &[u8]) = (
            Kind::GovernanceTracking(reason),
            PolicySchemaError::CircuitTracking(reason),
            body,
        );
        assert_rejection(&supplied, 17, expected, profile, charge)?;
    }
    for (threshold, calls, rate) in [(64, None, None), (65, Some(64), Some(1))] {
        let mut supplied = rule_input(GovernanceRuleKind::Circuits, b"q");
        let rule = circuit_rule(&mut supplied)?;
        rule.failure_threshold = threshold;
        rule.min_calls = calls;
        rule.failure_rate_pct = rate;
        assert!(check_complete_policy_replace_content(&supplied, 17, profile, charge)?.is_none());
    }
    Ok(())
}

pub(super) fn assert_top_level_priority(
    profile: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> TestResult {
    let mut supplied = rule_input(GovernanceRuleKind::Limits, b"");
    supplied
        .states
        .insert(b"running".to_vec(), state(None, None));
    supplied.indexed_state_meta = Some(b"__meta".to_vec());
    supplied.indexed_attributes = Some(vec![Vec::new()]);
    supplied.retention = Some(RetentionOverride {
        ttl_ms: Some(0),
        history_max_events: Some(18),
    });
    supplied.retry = Some(RetryOverride {
        max_retries: Some(1_001),
        backoff: Some(Backoff {
            base_ms: 2_592_000_001,
            ..Backoff::default()
        }),
        exhausted_to: Some(Vec::new()),
    });
    supplied.max_active_ms = Some(0);
    supplied.version = Some(PolicyVersion::Bytes(Vec::new()));
    for (kind, error, body) in [
        (
            Kind::InvalidVersion,
            PolicySchemaError::Policy(PolicyError::InvalidVersion),
            b"ERR flow policy version must be a non-empty string or non-negative integer"
                .as_slice(),
        ),
        (
            Kind::InvalidMaxActive,
            PolicySchemaError::Policy(PolicyError::InvalidMaxActive),
            b"ERR flow max_active_ms must be between 1 and 31536000000 or infinity".as_slice(),
        ),
        (
            Kind::RetryMaxRetries,
            PolicySchemaError::Policy(PolicyError::Domain(DomainError::InvalidRetryPolicy(
                RetryPolicyField::MaxRetries,
            ))),
            b"ERR flow retry max_retries must be between 0 and 1000".as_slice(),
        ),
        (
            Kind::RetentionTtl,
            PolicySchemaError::Policy(PolicyError::Retention(RetentionError::Ttl)),
            b"ERR flow retention ttl_ms must be between 1 and 31536000000".as_slice(),
        ),
        (
            Kind::IndexedAttributeEmpty,
            PolicySchemaError::InvalidIndexedAttributes,
            b"ERR flow attribute key must not be empty".as_slice(),
        ),
        (
            Kind::IndexedStateMetaReserved,
            PolicySchemaError::InvalidIndexedStateMeta,
            b"ERR flow state_meta key is reserved".as_slice(),
        ),
        (
            Kind::GovernanceEmptyName(GovernanceRuleKind::Limits),
            PolicySchemaError::InvalidName(GovernanceRuleKind::Limits),
            b"ERR flow governance limits names must be non-empty strings".as_slice(),
        ),
        (
            Kind::RunningState,
            PolicySchemaError::Policy(PolicyError::Domain(DomainError::InvalidRetryPolicy(
                RetryPolicyField::RunningState,
            ))),
            b"ERR flow policy state cannot be running".as_slice(),
        ),
    ] {
        assert_rejection(&supplied, 17, (kind, error, body), profile, charge)?;
        match kind {
            Kind::InvalidVersion => supplied.version = None,
            Kind::InvalidMaxActive => supplied.max_active_ms = None,
            Kind::RetryMaxRetries => supplied.retry = None,
            Kind::RetentionTtl => supplied.retention = None,
            Kind::IndexedAttributeEmpty => supplied.indexed_attributes = None,
            Kind::IndexedStateMetaReserved => supplied.indexed_state_meta = None,
            Kind::GovernanceEmptyName(_) => supplied.governance = None,
            Kind::RunningState => supplied.states.clear(),
            _ => return Err("priority fixture".into()),
        }
    }
    Ok(())
}

pub(super) fn assert_rule_priority(
    profile: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> TestResult {
    let mut supplied = rule_input(GovernanceRuleKind::Limits, b"q");
    let governance = supplied.governance.as_mut().ok_or("governance")?;
    governance
        .limits
        .as_mut()
        .ok_or("limits")?
        .get_mut(b"q".as_slice())
        .ok_or("rule")?
        .limit = 9_007_199_254_740_992;
    governance.approvals = Some(ApprovalsData {
        effects: Some(vec![Vec::new()]),
        states: None,
        assignees: None,
        timeout_ms: Some(0),
    });
    let expected: (Kind, PolicySchemaError, &[u8]) = (
        Kind::GovernanceLimit,
        PolicySchemaError::InvalidInteger(GovernanceField::Limit),
        b"ERR flow governance limit must be a non-negative integer",
    );
    assert_rejection(&supplied, 17, expected, profile, charge)?;
    let mut supplied = rule_input(GovernanceRuleKind::Circuits, b"q");
    let rule = circuit_rule(&mut supplied)?;
    rule.failure_threshold = 0;
    rule.error_classes = Some(vec![Vec::new()]);
    let expected: (Kind, PolicySchemaError, &[u8]) = (
        Kind::GovernancePositive(GovernanceField::FailureThreshold),
        PolicySchemaError::InvalidInteger(GovernanceField::FailureThreshold),
        b"ERR flow governance failure_threshold must be a positive integer",
    );
    assert_rejection(&supplied, 17, expected, profile, charge)?;
    let rule = circuit_rule(&mut supplied)?;
    rule.failure_threshold = 1;
    rule.failure_rate_pct = Some(0);
    rule.latency_threshold_ms = Some(0);
    let expected: (Kind, PolicySchemaError, &[u8]) = (
        Kind::GovernancePercent,
        PolicySchemaError::InvalidPercent,
        b"ERR flow governance failure_rate_pct must be an integer from 1 to 100",
    );
    assert_rejection(&supplied, 17, expected, profile, charge)?;
    Ok(())
}

pub(super) fn circuit_rule(supplied: &mut CompletePolicyData) -> TestResult<&mut CircuitData> {
    supplied
        .governance
        .as_mut()
        .ok_or("governance")?
        .circuits
        .as_mut()
        .ok_or("circuits")?
        .values_mut()
        .next()
        .ok_or_else(|| "rule".into())
}

fn budget_rule(supplied: &mut CompletePolicyData) -> TestResult<&mut BudgetData> {
    supplied
        .governance
        .as_mut()
        .ok_or("governance")?
        .budgets
        .as_mut()
        .ok_or("budgets")?
        .values_mut()
        .next()
        .ok_or_else(|| "rule".into())
}
