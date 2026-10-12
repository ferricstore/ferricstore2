//! Source-ordered normalized replacement diagnostics; no retained authority.

use std::collections::BTreeMap;

use wf_domain::policy::complete::{
    ApprovalsData, AuditData, CircuitTrackingFailure, CompletePolicyData,
    CompletePolicyReplaceRejection, CompletePolicyReplaceRejectionKind as Kind,
    CompleteStatePolicyData, DenialAuditMode, EffectsData, GovernanceData, GovernanceField,
    GovernanceMode, GovernanceRuleKind, PolicySchemaCharge, PolicySchemaError, PolicySchemaLimits,
    check_complete_policy_replace_content, encode_complete_policy_replace_rejection_body,
};
use wf_domain::policy::definition::{PolicyVersion, StateMode};
use wf_domain::policy::installation::PolicyError;
use wf_domain::policy::{Backoff, BackoffKind, RetryOverride};
use wf_domain::retention::{RetentionError, RetentionOverride};
use wf_domain::value::{DomainError, InputField, RetryPolicyField};

use super::{TestResult, data, limits};

mod governance;
mod refusals;

use governance::{
    assert_rule_priority, assert_top_level_priority, circuit_rule, list_input, rule_input,
};

#[test]
fn replace_content_preserves_normalized_presence_and_numeric_boundaries() -> TestResult {
    let profile = limits(128 * 1_024 * 1_024)?;
    let mut charge = PolicySchemaCharge::new(profile);
    assert_core_boundaries(profile, &mut charge)?;
    let absent = minimal();
    assert!(check_complete_policy_replace_content(&absent, 17, profile, &mut charge)?.is_none());
    assert!(absent == minimal());
    for mode in [
        GovernanceMode::Minimal,
        GovernanceMode::Ledger,
        GovernanceMode::Full,
    ] {
        let mut supplied = presence_data();
        let mut expected = presence_data();
        supplied.governance.as_mut().ok_or("governance")?.mode = Some(mode);
        expected.governance.as_mut().ok_or("governance")?.mode = Some(mode);
        let owner = supplied.workflow_type.as_ptr();
        assert!(
            check_complete_policy_replace_content(&supplied, 17, profile, &mut charge)?.is_none()
        );
        assert!(supplied == expected);
        assert_eq!(supplied.workflow_type.as_ptr(), owner);
    }
    for kind in [
        BackoffKind::None,
        BackoffKind::Fixed,
        BackoffKind::Linear,
        BackoffKind::Exponential,
    ] {
        for (base_ms, max_ms, jitter_pct, max_retries) in
            [(0, 0, 0, 0), (2_592_000_000, 2_592_000_000, 100, 1_000)]
        {
            let retry = RetryOverride {
                max_retries: Some(max_retries),
                backoff: Some(Backoff {
                    kind,
                    base_ms,
                    max_ms,
                    jitter_pct,
                }),
                exhausted_to: Some(vec![0, 255]),
            };
            let mut supplied = minimal();
            supplied.retry = Some(retry.clone());
            supplied
                .states
                .insert(b"s".to_vec(), state(Some(retry), None));
            assert!(
                check_complete_policy_replace_content(&supplied, 17, profile, &mut charge)?
                    .is_none()
            );
            assert_eq!(
                supplied
                    .retry
                    .as_ref()
                    .and_then(|r| r.backoff)
                    .map(|b| b.kind),
                Some(kind)
            );
            assert_eq!(supplied.states[b"s".as_slice()].retry, supplied.retry);
        }
    }
    for version in [
        PolicyVersion::Number(0),
        PolicyVersion::Number(u64::MAX),
        PolicyVersion::Bytes(vec![0, 255]),
    ] {
        let supplied = data(vec![0, 255], Some(version.clone()), BTreeMap::new());
        assert!(
            check_complete_policy_replace_content(&supplied, 17, profile, &mut charge)?.is_none()
        );
        assert_eq!(supplied.version, Some(version));
        assert_eq!(supplied.workflow_type, [0, 255]);
    }
    Ok(())
}

#[test]
fn replace_content_captures_history_limit_without_expanding_defaults() -> TestResult {
    let profile = limits(128 * 1_024 * 1_024)?;
    let mut charge = PolicySchemaCharge::new(profile);
    for (maximum, body) in [
        (
            1,
            b"ERR flow retention history_max_events must be between 1 and 1".as_slice(),
        ),
        (
            17,
            b"ERR flow retention history_max_events must be between 1 and 17".as_slice(),
        ),
        (
            1_000_000,
            b"ERR flow retention history_max_events must be between 1 and 1000000".as_slice(),
        ),
    ] {
        for in_state in [false, true] {
            let accepted = history_data(maximum, in_state);
            assert!(
                check_complete_policy_replace_content(&accepted, maximum, profile, &mut charge)?
                    .is_none()
            );
            for value in [0, maximum + 1] {
                let supplied = history_data(value, in_state);
                assert_rejection(
                    &supplied,
                    maximum,
                    (
                        Kind::RetentionHistory { maximum },
                        PolicySchemaError::Policy(PolicyError::Retention(RetentionError::History)),
                        body,
                    ),
                    profile,
                    &mut charge,
                )?;
                assert!(supplied == history_data(value, in_state));
            }
        }
    }
    let empty = presence_data();
    assert!(check_complete_policy_replace_content(&empty, 1, profile, &mut charge)?.is_none());
    assert!(empty == presence_data());
    let supplied = history_data(18, false);
    let rejection = check_complete_policy_replace_content(&supplied, 17, profile, &mut charge)?
        .ok_or("history rejection")?;
    assert!(check_complete_policy_replace_content(&supplied, 18, profile, &mut charge)?.is_none());
    let expected = b"ERR flow retention history_max_events must be between 1 and 17";
    let before = charge.g();
    let first = encode_complete_policy_replace_rejection_body(rejection, profile, &mut charge)?;
    let second = encode_complete_policy_replace_rejection_body(rejection, profile, &mut charge)?;
    assert_eq!(first, expected);
    assert_eq!(second, expected);
    assert_eq!(rejection.history_max_events_limit(), Some(17));
    assert_eq!(rejection.kind(), Kind::RetentionHistory { maximum: 17 });
    assert_eq!(charge.g(), before + 640);
    assert!(supplied == history_data(18, false));
    for in_state in [false, true] {
        let mut supplied = history_data(18, in_state);
        let retention = RetentionOverride {
            ttl_ms: Some(0),
            history_max_events: Some(18),
        };
        if in_state {
            supplied
                .states
                .get_mut(b"s".as_slice())
                .ok_or("state")?
                .retention = Some(retention);
        } else {
            supplied.retention = Some(retention);
        }
        let expected: (Kind, PolicySchemaError, &[u8]) = (
            Kind::RetentionTtl,
            PolicySchemaError::Policy(PolicyError::Retention(RetentionError::Ttl)),
            b"ERR flow retention ttl_ms must be between 1 and 31536000000",
        );
        assert_rejection(&supplied, 17, expected, profile, &mut charge)?;
    }
    Ok(())
}

fn minimal() -> CompletePolicyData {
    data(b"r".to_vec(), None, BTreeMap::new())
}

fn state(
    retry: Option<RetryOverride>,
    retention: Option<RetentionOverride>,
) -> CompleteStatePolicyData {
    CompleteStatePolicyData {
        mode: None,
        retry,
        retention,
        governance: None,
    }
}

fn empty_governance() -> GovernanceData {
    GovernanceData {
        mode: None,
        effects: None,
        audit: None,
        limits: None,
        budgets: None,
        circuits: None,
        approvals: None,
    }
}

fn presence_data() -> CompletePolicyData {
    let mut supplied = minimal();
    supplied.retry = Some(RetryOverride::default());
    supplied.retention = Some(RetentionOverride::default());
    supplied.indexed_attributes = Some(Vec::new());
    supplied.governance = Some(GovernanceData {
        mode: None,
        effects: Some(EffectsData {
            allowed: Some(Vec::new()),
            denied: Some(Vec::new()),
            require_idempotency_key: Some(false),
        }),
        audit: Some(AuditData {
            effect_events: Some(false),
            denials: Some(DenialAuditMode::Off),
        }),
        limits: Some(BTreeMap::new()),
        budgets: Some(BTreeMap::new()),
        circuits: Some(BTreeMap::new()),
        approvals: Some(ApprovalsData {
            effects: Some(Vec::new()),
            states: Some(Vec::new()),
            assignees: Some(Vec::new()),
            timeout_ms: None,
        }),
    });
    for (name, mode) in [
        (b"a".as_slice(), StateMode::Parallel),
        (b"z", StateMode::Fifo),
    ] {
        let mut value = state(
            Some(RetryOverride::default()),
            Some(RetentionOverride::default()),
        );
        value.mode = Some(mode);
        value.governance = Some(empty_governance());
        supplied.states.insert(name.to_vec(), value);
    }
    supplied
}

fn history_data(history: u32, in_state: bool) -> CompletePolicyData {
    let mut supplied = minimal();
    let retention = RetentionOverride {
        ttl_ms: None,
        history_max_events: Some(history),
    };
    if in_state {
        supplied
            .states
            .insert(b"s".to_vec(), state(None, Some(retention)));
    } else {
        supplied.retention = Some(retention);
    }
    supplied
}

fn assert_rejection(
    supplied: &CompletePolicyData,
    maximum: u32,
    expected: (Kind, PolicySchemaError, &[u8]),
    profile: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> TestResult<CompletePolicyReplaceRejection> {
    let rejection = check_complete_policy_replace_content(supplied, maximum, profile, charge)?
        .ok_or("expected content rejection")?;
    assert_eq!(rejection.kind(), expected.0);
    assert_eq!(rejection.schema_error(), expected.1);
    assert_eq!(
        rejection.history_max_events_limit(),
        match expected.0 {
            Kind::RetentionHistory { maximum } => Some(maximum),
            _ => None,
        }
    );
    let digits = if matches!(expected.0, Kind::RetentionHistory { .. }) {
        expected.2.len() - 60
    } else {
        0
    };
    let expected_work = 256 + u64::try_from(expected.2.len() + digits)?;
    let before_g = charge.g();
    let body = encode_complete_policy_replace_rejection_body(rejection, profile, charge)?;
    assert_eq!(body, expected.2);
    assert_eq!(charge.g(), before_g + expected_work);
    Ok(rejection)
}

fn assert_core_boundaries(
    profile: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> TestResult {
    let rows: [(usize, Kind, &[u8]); 12] = [
        (
            0,
            Kind::EmptyType,
            b"ERR flow type must be a non-empty string",
        ),
        (
            1,
            Kind::InvalidVersion,
            b"ERR flow policy version must be a non-empty string or non-negative integer",
        ),
        (
            2,
            Kind::InvalidMaxActive,
            b"ERR flow max_active_ms must be between 1 and 31536000000 or infinity",
        ),
        (
            3,
            Kind::RetryMaxRetries,
            b"ERR flow retry max_retries must be between 0 and 1000",
        ),
        (
            4,
            Kind::RetryBaseMillis,
            b"ERR flow retry base_ms must be between 0 and 2592000000",
        ),
        (
            5,
            Kind::RetryMaxMillis,
            b"ERR flow retry max_ms must be between 0 and 2592000000",
        ),
        (
            6,
            Kind::RetryJitterPercent,
            b"ERR flow retry jitter_pct must be between 0 and 100",
        ),
        (
            7,
            Kind::RetryExhaustedTo,
            b"ERR flow retry exhausted_to must be a non-empty string",
        ),
        (
            8,
            Kind::RetryRunningExhaustion,
            b"ERR flow retry exhausted_to cannot be running",
        ),
        (
            9,
            Kind::EmptyState,
            b"ERR flow policy state must be a non-empty string",
        ),
        (
            10,
            Kind::RunningState,
            b"ERR flow policy state cannot be running",
        ),
        (
            11,
            Kind::RetentionTtl,
            b"ERR flow retention ttl_ms must be between 1 and 31536000000",
        ),
    ];
    for (case, kind, body) in rows {
        let bad = core_input(case, true)?;
        let owner = bad.workflow_type.as_ptr();
        assert_rejection(&bad, 17, (kind, core_error(case)?, body), profile, charge)?;
        assert!(bad == core_input(case, true)?);
        assert_eq!(bad.workflow_type.as_ptr(), owner);
        let good = core_input(case, false)?;
        assert!(check_complete_policy_replace_content(&good, 17, profile, charge)?.is_none());
        assert!(good == core_input(case, false)?);
        if (3..=8).contains(&case) {
            let mut state_bad = minimal();
            state_bad
                .states
                .insert(b"s".to_vec(), state(bad.retry.clone(), None));
            assert_rejection(
                &state_bad,
                17,
                (kind, core_error(case)?, body),
                profile,
                charge,
            )?;
        }
    }
    assert_core_companion_bounds(profile, charge)
}

fn assert_core_companion_bounds(
    profile: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> TestResult {
    for active in [1, 31_536_000_000] {
        let mut supplied = minimal();
        supplied.max_active_ms = Some(active);
        assert!(check_complete_policy_replace_content(&supplied, 17, profile, charge)?.is_none());
    }
    for ttl in [1, 31_536_000_000] {
        let mut supplied = minimal();
        supplied.retention = Some(RetentionOverride {
            ttl_ms: Some(ttl),
            history_max_events: None,
        });
        assert!(check_complete_policy_replace_content(&supplied, 17, profile, charge)?.is_none());
    }
    for (active, ttl) in [(31_536_000_001, None), (1, Some(31_536_000_001))] {
        let mut supplied = minimal();
        supplied.max_active_ms = Some(active);
        supplied.retention = ttl.map(|ttl_ms| RetentionOverride {
            ttl_ms: Some(ttl_ms),
            history_max_events: None,
        });
        let case = if ttl.is_some() { 11 } else { 2 };
        let (kind, body) = if case == 11 {
            (
                Kind::RetentionTtl,
                b"ERR flow retention ttl_ms must be between 1 and 31536000000".as_slice(),
            )
        } else {
            (
                Kind::InvalidMaxActive,
                b"ERR flow max_active_ms must be between 1 and 31536000000 or infinity".as_slice(),
            )
        };
        assert_rejection(
            &supplied,
            17,
            (kind, core_error(case)?, body),
            profile,
            charge,
        )?;
    }
    Ok(())
}

fn core_input(case: usize, invalid: bool) -> TestResult<CompletePolicyData> {
    let mut supplied = minimal();
    let mut retry = RetryOverride::default();
    match case {
        0 => supplied.workflow_type = if invalid { Vec::new() } else { b"r".to_vec() },
        1 => {
            supplied.version = Some(PolicyVersion::Bytes(if invalid {
                Vec::new()
            } else {
                b"v".to_vec()
            }));
        }
        2 => supplied.max_active_ms = Some(u64::from(!invalid)),
        3 => retry.max_retries = Some(if invalid { 1_001 } else { 1_000 }),
        4..=6 => {
            let mut backoff = Backoff::default();
            match case {
                4 => {
                    backoff.base_ms = if invalid {
                        2_592_000_001
                    } else {
                        2_592_000_000
                    };
                }
                5 => {
                    backoff.max_ms = if invalid {
                        2_592_000_001
                    } else {
                        2_592_000_000
                    };
                }
                _ => backoff.jitter_pct = if invalid { 101 } else { 100 },
            }
            retry.backoff = Some(backoff);
        }
        7 => {
            retry.exhausted_to = Some(if invalid {
                Vec::new()
            } else {
                b"dead".to_vec()
            });
        }
        8 => {
            retry.exhausted_to = Some(if invalid {
                b"running".to_vec()
            } else {
                b"dead".to_vec()
            });
        }
        9 | 10 => {
            let name = if !invalid {
                b"s".to_vec()
            } else if case == 9 {
                Vec::new()
            } else {
                b"running".to_vec()
            };
            supplied.states.insert(name, state(None, None));
        }
        11 => {
            supplied.retention = Some(RetentionOverride {
                ttl_ms: Some(u64::from(!invalid)),
                history_max_events: None,
            });
        }
        _ => return Err("unknown core fixture".into()),
    }
    if (3..=8).contains(&case) {
        supplied.retry = Some(retry);
    }
    Ok(supplied)
}

fn core_error(case: usize) -> TestResult<PolicySchemaError> {
    let error = match case {
        0 => PolicyError::Domain(DomainError::EmptyValue(InputField::Type)),
        1 => PolicyError::InvalidVersion,
        2 => PolicyError::InvalidMaxActive,
        3..=10 => PolicyError::Domain(DomainError::InvalidRetryPolicy(match case {
            3 => RetryPolicyField::MaxRetries,
            4 => RetryPolicyField::BaseMillis,
            5 => RetryPolicyField::MaxMillis,
            6 => RetryPolicyField::JitterPercent,
            7 => RetryPolicyField::ExhaustedTo,
            8 => RetryPolicyField::RunningExhaustion,
            9 => RetryPolicyField::State,
            _ => RetryPolicyField::RunningState,
        })),
        11 => PolicyError::Retention(RetentionError::Ttl),
        _ => return Err("unknown core error fixture".into()),
    };
    Ok(PolicySchemaError::Policy(error))
}

#[test]
fn replace_content_keeps_source_field_and_circuit_error_priority() -> TestResult {
    let profile = limits(128 * 1_024 * 1_024)?;
    let mut charge = PolicySchemaCharge::new(profile);
    assert_top_level_priority(profile, &mut charge)?;
    assert_rule_priority(profile, &mut charge)?;
    let mut supplied = rule_input(GovernanceRuleKind::Circuits, b"c");
    let rule = circuit_rule(&mut supplied)?;
    rule.min_calls = Some(65);
    rule.failure_threshold = 65;
    rule.failure_rate_pct = None;
    let expected: (Kind, PolicySchemaError, &[u8]) = (
        Kind::GovernanceTracking(CircuitTrackingFailure::MinCallsAbove64),
        PolicySchemaError::CircuitTracking(CircuitTrackingFailure::MinCallsAbove64),
        b"ERR flow governance circuit min_calls must be <= 64",
    );
    assert_rejection(&supplied, 17, expected, profile, &mut charge)?;
    let mut supplied = rule_input(GovernanceRuleKind::Circuits, b"c");
    let rule = circuit_rule(&mut supplied)?;
    rule.failure_threshold = 65;
    rule.min_calls = None;
    let expected: (Kind, PolicySchemaError, &[u8]) = (
        Kind::GovernanceTracking(CircuitTrackingFailure::ThresholdAbove64WithoutRate),
        PolicySchemaError::CircuitTracking(CircuitTrackingFailure::ThresholdAbove64WithoutRate),
        b"ERR flow governance circuit failure_threshold must be <= 64 without failure_rate_pct",
    );
    assert_rejection(&supplied, 17, expected, profile, &mut charge)?;
    Ok(())
}

fn assert_list_bodies(profile: PolicySchemaLimits, charge: &mut PolicySchemaCharge) -> TestResult {
    for field in [
        GovernanceField::Allowed,
        GovernanceField::Denied,
        GovernanceField::ApprovalEffects,
        GovernanceField::ApprovalStates,
        GovernanceField::Assignees,
        GovernanceField::ErrorClasses,
    ] {
        let supplied = list_input(field, vec![Vec::new()])?;
        let expected: (Kind, PolicySchemaError, &[u8]) = (
            Kind::GovernanceEmptyList(field),
            PolicySchemaError::InvalidListValue(field),
            b"ERR flow governance list values must be non-empty strings",
        );
        assert_rejection(&supplied, 17, expected, profile, charge)?;
        assert!(supplied == list_input(field, vec![Vec::new()])?);
        let mut scoped = minimal();
        let mut child = state(None, None);
        child.governance = list_input(field, vec![Vec::new()])?.governance;
        scoped.states.insert(b"s".to_vec(), child);
        assert_rejection(&scoped, 17, expected, profile, charge)?;
        for names in [Vec::new(), vec![vec![255], vec![0]]] {
            let supplied = list_input(field, names.clone())?;
            assert!(
                check_complete_policy_replace_content(&supplied, 17, profile, charge)?.is_none()
            );
            assert!(supplied == list_input(field, names)?);
        }
    }
    Ok(())
}
