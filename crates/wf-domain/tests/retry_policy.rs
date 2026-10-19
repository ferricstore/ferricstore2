//! Pinned retry-policy timing, precedence and source admission observations.

#![forbid(unsafe_code)]

use std::collections::BTreeMap;

use wf_domain::policy::{
    Backoff, BackoffKind, InstalledRetryPolicy, RetryOverride, RetryPolicy, resolve,
};
use wf_domain::retry::JitterSample;
use wf_domain::workflow::MAX_EXACT_MILLIS;
use wf_types::version::TimestampMillis;

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn configured(kind: BackoffKind, base_ms: u64, max_ms: u64, jitter_pct: u8) -> RetryOverride {
    RetryOverride {
        backoff: Some(Backoff {
            kind,
            base_ms,
            max_ms,
            jitter_pct,
        }),
        ..RetryOverride::default()
    }
}

fn policy(override_: &RetryOverride) -> Result<RetryPolicy, wf_domain::workflow::DomainError> {
    resolve(None, b"type", b"queued", override_)
}

// Frozen declaration bd7d579285676091, policy section lines 483-531.
#[test]
fn flow_retry_policy_computes_standard_backoff_timing() -> TestResult {
    let now = TimestampMillis::new(1_000);
    let fixed = policy(&configured(BackoffKind::Fixed, 100, 1_000, 0))?;
    assert_eq!(
        fixed.next_run_at(now, 1, JitterSample::new(0))?.get(),
        1_100
    );
    assert_eq!(
        fixed.next_run_at(now, 3, JitterSample::new(0))?.get(),
        1_100
    );
    let linear = policy(&configured(BackoffKind::Linear, 100, 1_000, 0))?;
    assert_eq!(
        linear.next_run_at(now, 1, JitterSample::new(0))?.get(),
        1_100
    );
    assert_eq!(
        linear.next_run_at(now, 2, JitterSample::new(0))?.get(),
        1_200
    );
    assert_eq!(
        linear.next_run_at(now, 3, JitterSample::new(0))?.get(),
        1_300
    );
    let exponential = policy(&configured(BackoffKind::Exponential, 100, 250, 0))?;
    assert_eq!(
        exponential.next_run_at(now, 1, JitterSample::new(0))?.get(),
        1_100
    );
    assert_eq!(
        exponential.next_run_at(now, 2, JitterSample::new(0))?.get(),
        1_200
    );
    assert_eq!(
        exponential.next_run_at(now, 3, JitterSample::new(0))?.get(),
        1_250
    );
    let jittered = policy(&configured(BackoffKind::Exponential, 100, 250, 100))?;
    let next_run = jittered.next_run_at(now, 3, JitterSample::new(123_456))?;
    assert!(next_run.get() >= 1_000);
    assert!(next_run.get() <= 1_250);
    Ok(())
}

// Frozen declaration 999b558a2dc276d2, policy section lines 466-481.
#[test]
fn flow_retry_policy_accepts_thirty_day_backoff_cap() -> TestResult {
    let input = RetryOverride {
        max_retries: Some(1_000),
        exhausted_to: Some(b"needs_review".to_vec()),
        ..configured(BackoffKind::Exponential, 2_592_000_000, 2_592_000_000, 100)
    };
    let resolved = policy(&input)?;
    assert_eq!(resolved.backoff.base_ms, 2_592_000_000);
    assert_eq!(resolved.backoff.max_ms, 2_592_000_000);
    Ok(())
}

// Frozen declaration ecaed3fd34f388ec, policy section lines 533-581.
#[test]
fn flow_retry_policy_keeps_generated_backoff_values_bounded_and_deterministic() -> TestResult {
    let now = TimestampMillis::new(123_456);
    for kind in [
        BackoffKind::None,
        BackoffKind::Fixed,
        BackoffKind::Linear,
        BackoffKind::Exponential,
    ] {
        for base_ms in [0, 1, 100, 60_000, 2_592_000_000] {
            for max_ms in [0, 1, 50, 1_000, 2_592_000_000] {
                for jitter_pct in [0, 25, 100] {
                    for attempt in [0, 1, 2, 3, 10, 1_000] {
                        let resolved = policy(&RetryOverride {
                            max_retries: Some(1_000),
                            ..configured(kind, base_ms, max_ms, jitter_pct)
                        })?;
                        let sample = JitterSample::new(42_731_729);
                        let first = resolved.next_run_at(now, attempt, sample)?;
                        let second = resolved.next_run_at(now, attempt, sample)?;
                        assert_eq!(first, second);
                        assert!(first >= now);
                        assert!(first.get() <= now.get() + max_ms);
                    }
                }
            }
        }
    }
    assert!(RetryPolicy::default().attempt_allowed(3));
    assert!(!RetryPolicy::default().attempt_allowed(4));
    let no_retries = policy(&RetryOverride {
        max_retries: Some(0),
        ..RetryOverride::default()
    })?;
    assert!(!no_retries.attempt_allowed(1));
    Ok(())
}

#[test]
fn defaults_type_state_command_precedence_preserves_normalized_backoff_replacement() -> TestResult {
    let type_retry = RetryOverride {
        max_retries: Some(5),
        ..configured(BackoffKind::Fixed, 10_000, 30_000, 0)
    };
    let state_retry = RetryOverride {
        max_retries: Some(2),
        exhausted_to: Some(b"payment_failed".to_vec()),
        ..RetryOverride::default()
    };
    let installed = InstalledRetryPolicy::new(
        b"payment".to_vec(),
        type_retry,
        BTreeMap::from([(b"charge_card".to_vec(), state_retry)]),
    )?;
    let state = resolve(
        Some(&installed),
        b"payment",
        b"charge_card",
        &RetryOverride::default(),
    )?;
    assert_eq!(state.max_retries, 2);
    assert_eq!(state.exhausted_to, b"payment_failed");
    assert_eq!(state.backoff.base_ms, 10_000);
    let other_state = resolve(
        Some(&installed),
        b"payment",
        b"review",
        &RetryOverride::default(),
    )?;
    assert_eq!(other_state.max_retries, 5);
    assert_eq!(other_state.exhausted_to, b"failed");
    let command = RetryOverride {
        max_retries: Some(7),
        backoff: Some(Backoff {
            kind: BackoffKind::Linear,
            ..Backoff::default()
        }),
        ..RetryOverride::default()
    };
    let resolved = resolve(Some(&installed), b"payment", b"charge_card", &command)?;
    assert_eq!(resolved.max_retries, 7);
    assert_eq!(resolved.exhausted_to, b"payment_failed");
    assert_eq!(
        resolved.backoff,
        Backoff {
            kind: BackoffKind::Linear,
            ..Backoff::default()
        }
    );
    Ok(())
}

#[test]
fn source_policy_numeric_bounds_and_exhaustion_targets_reject_exactly() -> TestResult {
    let cases = [
        (
            RetryOverride {
                max_retries: Some(1_001),
                ..RetryOverride::default()
            },
            "ERR flow retry max_retries must be between 0 and 1000",
        ),
        (
            configured(BackoffKind::Fixed, 2_592_000_001, 100, 0),
            "ERR flow retry base_ms must be between 0 and 2592000000",
        ),
        (
            configured(BackoffKind::Fixed, 100, 2_592_000_001, 0),
            "ERR flow retry max_ms must be between 0 and 2592000000",
        ),
        (
            configured(BackoffKind::Fixed, 100, 100, 101),
            "ERR flow retry jitter_pct must be between 0 and 100",
        ),
        (
            RetryOverride {
                exhausted_to: Some(Vec::new()),
                ..RetryOverride::default()
            },
            "ERR flow retry exhausted_to must be a non-empty string",
        ),
        (
            RetryOverride {
                exhausted_to: Some(b"running".to_vec()),
                ..RetryOverride::default()
            },
            "ERR flow retry exhausted_to cannot be running",
        ),
    ];
    for (input, expected) in cases {
        let before = input.clone();
        let error = policy(&input).err().ok_or("invalid policy was accepted")?;
        assert_eq!(error.to_string(), expected);
        assert_eq!(input, before);
    }
    Ok(())
}

#[test]
fn committed_jitter_uses_source_spread_modulo_and_kind_specific_cap_order() -> TestResult {
    let now = TimestampMillis::new(1_000);
    let fixed = policy(&configured(BackoffKind::Fixed, 200, 150, 100))?;
    assert_eq!(
        fixed.next_run_at(now, 1, JitterSample::new(0))?.get(),
        1_000
    );
    assert_eq!(
        fixed.next_run_at(now, 1, JitterSample::new(400))?.get(),
        1_150
    );
    let linear = policy(&configured(BackoffKind::Linear, 200, 150, 100))?;
    assert_eq!(
        linear.next_run_at(now, 1, JitterSample::new(400))?.get(),
        1_099
    );
    let rounded = policy(&configured(BackoffKind::Fixed, 1, 100, 25))?;
    assert_eq!(
        rounded
            .next_run_at(now, 1, JitterSample::new(u32::MAX))?
            .get(),
        1_001
    );
    let zero = policy(&configured(BackoffKind::Exponential, 0, 100, 100))?;
    assert_eq!(
        zero.next_run_at(now, u64::MAX, JitterSample::new(u32::MAX))?,
        now
    );
    let huge = policy(&configured(BackoffKind::Exponential, 1, 100, 0))?;
    assert_eq!(
        huge.next_run_at(now, u64::MAX, JitterSample::new(0))?.get(),
        1_100
    );
    Ok(())
}

#[test]
fn computed_time_distinguishes_input_ceiling_from_checked_representation_overflow() -> TestResult {
    let fixed = policy(&configured(BackoffKind::Fixed, 100, 100, 0))?;
    assert_eq!(
        fixed
            .next_run_at(
                TimestampMillis::new(MAX_EXACT_MILLIS),
                1,
                JitterSample::new(0)
            )?
            .get(),
        MAX_EXACT_MILLIS + 100
    );
    let error = fixed
        .next_run_at(TimestampMillis::new(u64::MAX), 1, JitterSample::new(0))
        .err()
        .ok_or("computed timestamp wrapped")?;
    assert_eq!(error.to_string(), "ERR flow retry deadline overflow");
    Ok(())
}

#[test]
fn installed_view_rejects_invalid_logical_states_and_preserves_opaque_binary_names() -> TestResult {
    for (state, expected) in [
        (
            Vec::new(),
            "ERR flow policy state must be a non-empty string",
        ),
        (
            b"running".to_vec(),
            "ERR flow policy state cannot be running",
        ),
    ] {
        let error = InstalledRetryPolicy::new(
            b"payment".to_vec(),
            RetryOverride::default(),
            BTreeMap::from([(state, RetryOverride::default())]),
        )
        .err()
        .ok_or("invalid installed state accepted")?;
        assert_eq!(error.to_string(), expected);
    }
    let state = vec![0, 255];
    let workflow_type = vec![255, 0];
    let installed = InstalledRetryPolicy::new(
        workflow_type.clone(),
        RetryOverride::default(),
        BTreeMap::from([(
            state.clone(),
            RetryOverride {
                max_retries: Some(0),
                ..RetryOverride::default()
            },
        )]),
    )?;
    assert_eq!(installed.workflow_type(), workflow_type);
    assert_eq!(
        resolve(
            Some(&installed),
            &workflow_type,
            &state,
            &RetryOverride::default()
        )?
        .max_retries,
        0
    );
    Ok(())
}

// Frozen declaration deb8dc5136bd2e4e, direct RetryPolicyTest lines 6-16.
#[test]
fn original_flow_policy_rejects_both_malformed_state_policy_recipes() -> TestResult {
    use wf_domain::policy::complete::{
        FlowPolicyInputRejection, PolicySchemaCharge, PolicySchemaLimits, RawFlowPolicyValue,
        encode_original_complete_policy_input_rejection_body,
        normalize_original_complete_policy_input,
    };

    let limits = PolicySchemaLimits::default();
    for (workflow_type, value) in [
        (
            b"invalid-state-policy".as_slice(),
            RawFlowPolicyValue::Integer(42),
        ),
        (
            b"invalid-state-policy-list".as_slice(),
            RawFlowPolicyValue::List(&[RawFlowPolicyValue::Atom("not_a_keyword_pair")]),
        ),
    ] {
        let states = [(RawFlowPolicyValue::Binary(b"queued"), value)];
        let option = [
            RawFlowPolicyValue::Atom("states"),
            RawFlowPolicyValue::Map(&states),
        ];
        let opts = [RawFlowPolicyValue::Tuple(&option)];
        let mut charge = PolicySchemaCharge::new(limits);
        assert_eq!(
            normalize_original_complete_policy_input(
                &RawFlowPolicyValue::Binary(workflow_type),
                &RawFlowPolicyValue::List(&opts),
                1_000_000,
                limits,
                &mut charge,
            )?,
            Err(FlowPolicyInputRejection::StatePolicyShape)
        );
        assert_eq!(
            encode_original_complete_policy_input_rejection_body(
                FlowPolicyInputRejection::StatePolicyShape,
                limits,
                &mut charge,
            )?,
            b"ERR flow state policy must be a map or keyword list"
        );
    }
    Ok(())
}

// Frozen declaration 9e5df11d9c3d3842, direct RetryPolicyTest lines 18-25.
#[test]
fn original_flow_policy_omits_absent_fields_and_preserves_the_exact_empty_state() -> TestResult {
    use wf_domain::policy::complete::{
        PolicySchemaCharge, PolicySchemaLimits, RawFlowPolicyValue,
        normalize_original_complete_policy_input,
    };

    let limits = PolicySchemaLimits::default();
    let states = [(
        RawFlowPolicyValue::Binary(b"queued"),
        RawFlowPolicyValue::Map(&[]),
    )];
    let option = [
        RawFlowPolicyValue::Atom("states"),
        RawFlowPolicyValue::Map(&states),
    ];
    let opts = [RawFlowPolicyValue::Tuple(&option)];
    let mut charge = PolicySchemaCharge::new(limits);
    let actual = normalize_original_complete_policy_input(
        &RawFlowPolicyValue::Binary(b"compact-policy"),
        &RawFlowPolicyValue::List(&opts),
        1_000_000,
        limits,
        &mut charge,
    )?;
    // Complete canonical ETF: only states={queued=>{}} and type are present.
    let expected = b"\x83t\0\0\0\x02w\x06statest\0\0\0\x01m\0\0\0\x06queuedt\0\0\0\0w\x04typem\0\0\0\x0ecompact-policy";
    assert_eq!(actual, Ok(expected.to_vec()));
    Ok(())
}
