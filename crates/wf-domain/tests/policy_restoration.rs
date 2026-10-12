//! Approved A-N7/A-N8 normalized value restoration and byte-owner binding controls.
#![forbid(unsafe_code)]

use std::collections::BTreeMap;

use wf_domain::policy::definition::{PolicyVersion, StateMode, StatePolicy};
use wf_domain::policy::installation::{
    MAX_ENCODED_POLICY_BYTES, MAX_POLICY_GENERATION, MAX_POLICY_SNAPSHOT_BYTES, PolicyError,
    PolicyLimits,
};
use wf_domain::policy::restoration::{PolicySnapshotData, restore_candidate};
use wf_domain::policy::{Backoff, BackoffKind, RetryOverride};
use wf_domain::retention::{
    MAX_HISTORY_EVENTS, MAX_RETENTION_MILLIS, RetentionError, RetentionOverride,
};
use wf_domain::value::{DomainError, InputField, RetryPolicyField};

type TestError = Box<dyn std::error::Error>;
type TestResult<T = ()> = Result<T, TestError>;

fn empty() -> PolicySnapshotData {
    PolicySnapshotData {
        workflow_type: b"job".to_vec(),
        version: None,
        max_active_ms: None,
        retry: RetryOverride::default(),
        retention: RetentionOverride::default(),
        states: BTreeMap::new(),
    }
}

fn state(name: &[u8], policy: StatePolicy) -> PolicySnapshotData {
    PolicySnapshotData {
        states: BTreeMap::from([(name.to_vec(), policy)]),
        ..empty()
    }
}

fn assert_error(data: PolicySnapshotData, expected: PolicyError) {
    assert_eq!(
        restore_candidate(1, data, PolicyLimits::default()),
        Err(expected)
    );
}

#[test]
fn installed_generation_accepts_exact_extremes_without_allocating_a_new_generation() -> TestResult {
    for generation in [1, MAX_POLICY_GENERATION] {
        let candidate = restore_candidate(generation, empty(), PolicyLimits::default())?;
        assert_eq!(candidate.generation(), generation);
    }
    for generation in [0, MAX_POLICY_GENERATION + 1, u64::MAX] {
        assert_eq!(
            restore_candidate(generation, empty(), PolicyLimits::default()),
            Err(PolicyError::InvalidGeneration)
        );
    }
    Ok(())
}

#[test]
fn normalized_fields_and_opaque_vec_owners_are_moved_without_patch_normalization() -> TestResult {
    let mut workflow_type = Vec::with_capacity(64);
    workflow_type.extend_from_slice(&[0xff, 0]);
    let version = vec![0xfe, 0];
    let target = vec![0xfd, 0];
    let state_name = vec![0xfc, 0];
    let state_target = vec![0xfb, 0];
    let pointers = [
        workflow_type.as_ptr(),
        version.as_ptr(),
        target.as_ptr(),
        state_name.as_ptr(),
        state_target.as_ptr(),
    ];
    let backoff = Backoff {
        kind: BackoffKind::None,
        base_ms: 0,
        max_ms: 0,
        jitter_pct: 0,
    };
    let retention = RetentionOverride {
        ttl_ms: Some(1),
        history_max_events: Some(1),
    };
    let data = PolicySnapshotData {
        workflow_type,
        version: Some(PolicyVersion::Bytes(version)),
        max_active_ms: Some(MAX_RETENTION_MILLIS),
        retry: RetryOverride {
            max_retries: Some(0),
            backoff: Some(backoff),
            exhausted_to: Some(target),
        },
        retention,
        states: BTreeMap::from([(
            state_name,
            StatePolicy {
                mode: Some(StateMode::Fifo),
                retry: RetryOverride {
                    max_retries: Some(1_000),
                    backoff: Some(Backoff {
                        kind: BackoffKind::Exponential,
                        base_ms: 2_592_000_000,
                        max_ms: 2_592_000_000,
                        jitter_pct: 100,
                    }),
                    exhausted_to: Some(state_target),
                },
                retention: RetentionOverride {
                    ttl_ms: Some(MAX_RETENTION_MILLIS),
                    history_max_events: Some(MAX_HISTORY_EVENTS),
                },
            },
        )]),
    };
    let candidate = restore_candidate(MAX_POLICY_GENERATION, data, PolicyLimits::new(1)?)?;
    let policy = candidate.policy();
    assert_eq!(policy.workflow_type(), [0xff, 0]);
    assert_eq!(policy.workflow_type().as_ptr(), pointers[0]);
    let Some(PolicyVersion::Bytes(version)) = policy.version() else {
        return Err("binary version absent".into());
    };
    assert_eq!(version.as_slice(), [0xfe, 0]);
    assert_eq!(version.as_ptr(), pointers[1]);
    let retry = policy.type_retry();
    assert_eq!(retry.max_retries, Some(0));
    assert_eq!(retry.backoff, Some(backoff));
    let type_target = retry.exhausted_to.as_deref().ok_or("type target absent")?;
    assert_eq!(type_target, [0xfd, 0]);
    assert_eq!(type_target.as_ptr(), pointers[2]);
    assert_eq!(policy.type_retention(), retention);
    assert_eq!(policy.max_active_ms(), Some(MAX_RETENTION_MILLIS));
    let (name, state) = policy.states().first_key_value().ok_or("state absent")?;
    assert_eq!(name.as_slice(), [0xfc, 0]);
    assert_eq!(name.as_ptr(), pointers[3]);
    assert_eq!(state.mode, Some(StateMode::Fifo));
    assert_eq!(state.retry.max_retries, Some(1_000));
    assert_eq!(
        state.retry.backoff,
        Some(Backoff {
            kind: BackoffKind::Exponential,
            base_ms: 2_592_000_000,
            max_ms: 2_592_000_000,
            jitter_pct: 100
        })
    );
    let state_target = state
        .retry
        .exhausted_to
        .as_deref()
        .ok_or("state target absent")?;
    assert_eq!(state_target, [0xfb, 0]);
    assert_eq!(state_target.as_ptr(), pointers[4]);
    assert_eq!(state.retention.ttl_ms, Some(MAX_RETENTION_MILLIS));
    assert_eq!(state.retention.history_max_events, Some(MAX_HISTORY_EVENTS));
    Ok(())
}

#[test]
fn none_numeric_zero_max_and_absent_or_explicit_parallel_mode_remain_distinct() -> TestResult {
    for version in [
        None,
        Some(PolicyVersion::Number(0)),
        Some(PolicyVersion::Number(u64::MAX)),
    ] {
        for max_active_ms in [None, Some(1), Some(MAX_RETENTION_MILLIS)] {
            let data = PolicySnapshotData {
                version: version.clone(),
                max_active_ms,
                states: BTreeMap::from([
                    (b"a".to_vec(), StatePolicy::default()),
                    (
                        b"b".to_vec(),
                        StatePolicy {
                            mode: Some(StateMode::Parallel),
                            ..StatePolicy::default()
                        },
                    ),
                ]),
                ..empty()
            };
            let candidate = restore_candidate(1, data, PolicyLimits::new(2)?)?;
            let policy = candidate.policy();
            assert_eq!(policy.version(), version.as_ref());
            assert_eq!(policy.max_active_ms(), max_active_ms);
            assert_eq!(policy.type_retry(), &RetryOverride::default());
            assert_eq!(policy.type_retention(), RetentionOverride::default());
            assert_eq!(policy.states()[b"a".as_slice()].mode, None);
            assert_eq!(
                policy.states()[b"b".as_slice()].mode,
                Some(StateMode::Parallel)
            );
        }
    }
    Ok(())
}

#[test]
fn malformed_type_version_active_and_state_names_are_rejected() {
    assert_error(
        PolicySnapshotData {
            workflow_type: vec![],
            ..empty()
        },
        PolicyError::Domain(DomainError::EmptyValue(InputField::Type)),
    );
    assert_error(
        PolicySnapshotData {
            version: Some(PolicyVersion::Bytes(vec![])),
            ..empty()
        },
        PolicyError::InvalidVersion,
    );
    for max_active_ms in [0, MAX_RETENTION_MILLIS + 1] {
        assert_error(
            PolicySnapshotData {
                max_active_ms: Some(max_active_ms),
                ..empty()
            },
            PolicyError::InvalidMaxActive,
        );
    }
    assert_error(
        state(b"", StatePolicy::default()),
        PolicyError::Domain(DomainError::InvalidRetryPolicy(RetryPolicyField::State)),
    );
    assert_error(
        state(b"running", StatePolicy::default()),
        PolicyError::Domain(DomainError::InvalidRetryPolicy(
            RetryPolicyField::RunningState,
        )),
    );
}

#[test]
fn invalid_normalized_retry_fields_are_checked_at_both_type_and_state() {
    let cases = [
        (
            RetryOverride {
                max_retries: Some(1_001),
                ..RetryOverride::default()
            },
            RetryPolicyField::MaxRetries,
        ),
        (
            RetryOverride {
                exhausted_to: Some(vec![]),
                ..RetryOverride::default()
            },
            RetryPolicyField::ExhaustedTo,
        ),
        (
            RetryOverride {
                exhausted_to: Some(b"running".to_vec()),
                ..RetryOverride::default()
            },
            RetryPolicyField::RunningExhaustion,
        ),
        (
            RetryOverride {
                backoff: Some(Backoff {
                    base_ms: 2_592_000_001,
                    ..Backoff::default()
                }),
                ..RetryOverride::default()
            },
            RetryPolicyField::BaseMillis,
        ),
        (
            RetryOverride {
                backoff: Some(Backoff {
                    max_ms: 2_592_000_001,
                    ..Backoff::default()
                }),
                ..RetryOverride::default()
            },
            RetryPolicyField::MaxMillis,
        ),
        (
            RetryOverride {
                backoff: Some(Backoff {
                    jitter_pct: 101,
                    ..Backoff::default()
                }),
                ..RetryOverride::default()
            },
            RetryPolicyField::JitterPercent,
        ),
    ];
    for (retry, field) in cases {
        let error = PolicyError::Domain(DomainError::InvalidRetryPolicy(field));
        assert_error(
            PolicySnapshotData {
                retry: retry.clone(),
                ..empty()
            },
            error,
        );
        assert_error(
            state(
                b"queued",
                StatePolicy {
                    retry,
                    ..StatePolicy::default()
                },
            ),
            error,
        );
    }
}

#[test]
fn invalid_retention_fields_are_checked_at_both_type_and_state() {
    let cases = [
        (
            RetentionOverride {
                ttl_ms: Some(0),
                history_max_events: None,
            },
            RetentionError::Ttl,
        ),
        (
            RetentionOverride {
                ttl_ms: Some(MAX_RETENTION_MILLIS + 1),
                history_max_events: None,
            },
            RetentionError::Ttl,
        ),
        (
            RetentionOverride {
                ttl_ms: None,
                history_max_events: Some(0),
            },
            RetentionError::History,
        ),
        (
            RetentionOverride {
                ttl_ms: None,
                history_max_events: Some(MAX_HISTORY_EVENTS + 1),
            },
            RetentionError::History,
        ),
    ];
    for (retention, error) in cases {
        assert_error(
            PolicySnapshotData {
                retention,
                ..empty()
            },
            PolicyError::Retention(error),
        );
        assert_error(
            state(
                b"queued",
                StatePolicy {
                    retention,
                    ..StatePolicy::default()
                },
            ),
            PolicyError::Retention(error),
        );
    }
}

#[test]
fn raw_value_byte_and_state_work_limits_have_inclusive_boundaries() -> TestResult {
    let candidate = restore_candidate(
        1,
        PolicySnapshotData {
            workflow_type: vec![0xff; MAX_POLICY_SNAPSHOT_BYTES],
            ..empty()
        },
        PolicyLimits::default(),
    )?;
    assert_eq!(
        candidate.policy().workflow_type().len(),
        MAX_POLICY_SNAPSHOT_BYTES
    );
    assert_error(
        PolicySnapshotData {
            workflow_type: vec![0xff; MAX_POLICY_SNAPSHOT_BYTES + 1],
            ..empty()
        },
        PolicyError::SnapshotTooLarge,
    );
    for extra in [0, 1] {
        let data = PolicySnapshotData {
            workflow_type: vec![0xff],
            version: Some(PolicyVersion::Bytes(vec![0xfe])),
            retry: RetryOverride {
                exhausted_to: Some(vec![0xfd]),
                ..RetryOverride::default()
            },
            states: BTreeMap::from([(
                vec![0xfc],
                StatePolicy {
                    retry: RetryOverride {
                        exhausted_to: Some(vec![0xfb; MAX_POLICY_SNAPSHOT_BYTES - 4 + extra]),
                        ..RetryOverride::default()
                    },
                    ..StatePolicy::default()
                },
            )]),
            ..empty()
        };
        let restored = restore_candidate(1, data, PolicyLimits::new(1)?);
        if extra == 0 {
            assert!(restored.is_ok());
        } else {
            assert_eq!(restored, Err(PolicyError::SnapshotTooLarge));
        }
    }
    let data = PolicySnapshotData {
        states: BTreeMap::from([
            (b"a".to_vec(), StatePolicy::default()),
            (b"b".to_vec(), StatePolicy::default()),
        ]),
        ..empty()
    };
    assert!(restore_candidate(1, data, PolicyLimits::new(2)?).is_ok());
    let data = PolicySnapshotData {
        states: BTreeMap::from([
            (b"a".to_vec(), StatePolicy::default()),
            (b"b".to_vec(), StatePolicy::default()),
            (b"c".to_vec(), StatePolicy::default()),
        ]),
        ..empty()
    };
    assert_eq!(
        restore_candidate(1, data, PolicyLimits::new(2)?),
        Err(PolicyError::StateBudget)
    );
    Ok(())
}

#[test]
fn range_binding_preserves_exact_candidate_and_encoded_owners() -> TestResult {
    let limits = PolicyLimits::default();
    let candidate = restore_candidate(MAX_POLICY_GENERATION, empty(), limits)?;
    let type_pointer = candidate.policy().workflow_type().as_ptr();
    let mut encoded = Vec::with_capacity(64);
    encoded.extend_from_slice(&[9, 8, 7, 6, 5, 4]);
    let encoded_pointer = encoded.as_ptr();
    let value = candidate.bind_encoded_snapshot(1..5, encoded, limits)?;
    assert_eq!(value.generation(), MAX_POLICY_GENERATION);
    assert_eq!(value.policy().workflow_type(), b"job");
    assert_eq!(value.policy().workflow_type().as_ptr(), type_pointer);
    assert_eq!(value.encoded(), &[9, 8, 7, 6, 5, 4]);
    assert_eq!(value.encoded().as_ptr(), encoded_pointer);
    Ok(())
}

#[test]
fn range_binding_rejects_reversed_and_out_of_bounds_ranges() -> TestResult {
    let limits = PolicyLimits::default();
    for (start, end) in [
        (2, 1),
        (0, 4),
        (4, 4),
        (usize::MAX, usize::MAX),
        (0, usize::MAX),
    ] {
        let candidate = restore_candidate(1, empty(), limits)?;
        assert_eq!(
            candidate.bind_encoded_snapshot(start..end, vec![0; 3], limits),
            Err(PolicyError::InvalidAuthority)
        );
    }
    Ok(())
}

#[test]
fn range_binding_checks_actual_snapshot_and_envelope_inclusive_limits() -> TestResult {
    let limits = PolicyLimits::default();
    for extra in [0, 1] {
        let candidate = restore_candidate(1, empty(), limits)?;
        let snapshot_len = MAX_POLICY_SNAPSHOT_BYTES + extra;
        let encoded = vec![0xff; snapshot_len + 2];
        let result = candidate.bind_encoded_snapshot(1..1 + snapshot_len, encoded, limits);
        if extra == 0 {
            assert_eq!(result?.encoded().len(), snapshot_len + 2);
        } else {
            assert_eq!(result, Err(PolicyError::SnapshotTooLarge));
        }
    }
    for extra in [0, 1] {
        let candidate = restore_candidate(1, empty(), limits)?;
        let encoded = vec![0xff; MAX_ENCODED_POLICY_BYTES + extra];
        let result = candidate.bind_encoded_snapshot(2..3, encoded, limits);
        if extra == 0 {
            assert_eq!(result?.encoded().len(), MAX_ENCODED_POLICY_BYTES);
        } else {
            assert_eq!(result, Err(PolicyError::EncodedPolicyTooLarge));
        }
    }
    Ok(())
}

#[test]
fn range_binding_revalidates_snapshot_under_selected_limits() -> TestResult {
    let data = PolicySnapshotData {
        states: BTreeMap::from([
            (b"a".to_vec(), StatePolicy::default()),
            (b"b".to_vec(), StatePolicy::default()),
        ]),
        ..empty()
    };
    let candidate = restore_candidate(1, data, PolicyLimits::new(2)?)?;
    assert_eq!(
        candidate.bind_encoded_snapshot(1..2, vec![0; 3], PolicyLimits::new(1)?),
        Err(PolicyError::StateBudget)
    );
    Ok(())
}
