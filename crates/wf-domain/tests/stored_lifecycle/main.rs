//! Full stored-donor lifecycle capture, distinct from mode/retry projections.

#![forbid(unsafe_code)]

mod fixtures;

use fixtures::{TestResult, input, policy_pair};
use wf_domain::policy::complete::{
    CompletePolicyEncodingError as PolicyError, PolicySchemaCharge, PolicySchemaError,
    PolicySchemaLimits, StoredLifecycleCaptureError as CaptureError, StoredPolicyValue,
    capture_stored_creation_lifecycle, decode_original_complete_policy_entry_portable,
};
use wf_domain::retention::RetentionOverride;
use wf_domain::workflow::active::{ActiveValueError, LifecycleOverrides, MaxActiveOverride};

#[test]
fn stored_lifecycle_captures_complete_policy_precedence() -> TestResult {
    let (complete, _original) = policy_pair()?;
    let schema = PolicySchemaLimits::default();
    let mut charge = PolicySchemaCharge::new(schema);
    for (state, command, expected) in [
        (
            b"q".as_slice(),
            input().overrides,
            (Some(5_000), 22_000, 2, 40),
        ),
        (
            b"other".as_slice(),
            input().overrides,
            (Some(5_000), 11_000, 2, 30),
        ),
        (
            b"q".as_slice(),
            LifecycleOverrides {
                active: MaxActiveOverride::Millis(6_000),
                retention: RetentionOverride {
                    ttl_ms: Some(33_000),
                    history_max_events: Some(50),
                },
                history_hot_max_events: Some(4),
            },
            (Some(6_000), 33_000, 4, 50),
        ),
    ] {
        let mut request = input();
        request.initial_state = state;
        request.overrides = command;
        let result = capture_stored_creation_lifecycle(
            Some(&complete),
            request,
            schema,
            &mut charge,
            usize::MAX,
        );
        assert!(result.is_ok(), "{:?}", result.as_ref().err());
        let Ok(capture) = result else {
            return Err("complete capture rejected".into());
        };
        assert_eq!(
            (
                capture.max_active_ms(),
                capture.retention_ttl_ms(),
                capture.history_hot_max_events(),
                capture.history_max_events(),
            ),
            expected,
        );
    }
    let mut invalid_active = input();
    invalid_active.overrides.active = MaxActiveOverride::Millis(0);
    let prefix = (charge.g(), charge.p());
    assert!(matches!(
        capture_stored_creation_lifecycle(
            Some(&complete),
            invalid_active,
            schema,
            &mut charge,
            usize::MAX,
        ),
        Err(CaptureError::Capture(
            ActiveValueError::InvalidCapturedActive
        )),
    ));
    assert!(charge.g() >= prefix.0 && charge.p() >= prefix.1);
    Ok(())
}

#[test]
fn stored_lifecycle_captures_original_policy_without_mode_projection_loss() -> TestResult {
    let (_complete, original) = policy_pair()?;
    let schema = PolicySchemaLimits::default();
    let mut charge = PolicySchemaCharge::new(schema);
    for (active, ttl, history, hot, caps, expected) in [
        (
            MaxActiveOverride::Inherit,
            None,
            None,
            None,
            (100, 1_000),
            (Some(5_000), 22_000, 2, 40),
        ),
        (
            MaxActiveOverride::Infinite,
            None,
            None,
            Some(0),
            (100, 1_000),
            (None, 22_000, 0, 40),
        ),
        (
            MaxActiveOverride::Millis(6_000),
            Some(33_000),
            Some(50),
            Some(4),
            (3, 25),
            (Some(6_000), 33_000, 3, 25),
        ),
    ] {
        let mut request = input();
        request.overrides = LifecycleOverrides {
            active,
            retention: RetentionOverride {
                ttl_ms: ttl,
                history_max_events: history,
            },
            history_hot_max_events: hot,
        };
        request.defaults.max_history_hot_max_events = caps.0;
        request.defaults.max_history_max_events = caps.1;
        let result = capture_stored_creation_lifecycle(
            Some(&original),
            request,
            schema,
            &mut charge,
            usize::MAX,
        );
        assert!(result.is_ok(), "{:?}", result.as_ref().err());
        let Ok(capture) = result else {
            return Err("original capture rejected".into());
        };
        assert_eq!(
            (
                capture.max_active_ms(),
                capture.retention_ttl_ms(),
                capture.history_hot_max_events(),
                capture.history_max_events(),
            ),
            expected,
        );
    }
    for (ttl, history, hot) in [
        (Some(0), None, None),
        (None, Some(0), None),
        (None, Some(3), Some(4)),
    ] {
        let mut request = input();
        request.overrides.retention = RetentionOverride {
            ttl_ms: ttl,
            history_max_events: history,
        };
        request.overrides.history_hot_max_events = hot;
        let prefix = (charge.g(), charge.p());
        assert!(matches!(
            capture_stored_creation_lifecycle(
                Some(&original),
                request,
                schema,
                &mut charge,
                usize::MAX,
            ),
            Err(CaptureError::Capture(
                ActiveValueError::InvalidCapturedRetention,
            )),
        ));
        assert!(charge.g() >= prefix.0 && charge.p() >= prefix.1);
    }
    Ok(())
}

#[test]
fn stored_lifecycle_distinguishes_absence_type_and_profile() -> TestResult {
    let (complete, original) = policy_pair()?;
    let schema = PolicySchemaLimits::default();
    let mut charge = PolicySchemaCharge::new(schema);
    let result = capture_stored_creation_lifecycle(None, input(), schema, &mut charge, 0);
    assert!(result.is_ok(), "{:?}", result.as_ref().err());
    let Ok(capture) = result else {
        return Err("absent policy capture rejected".into());
    };
    assert_eq!(
        (
            capture.max_active_ms(),
            capture.retention_ttl_ms(),
            capture.history_hot_max_events(),
            capture.history_max_events(),
        ),
        (None, 7_000, 2, 10),
    );
    for policy in [&complete, &original] {
        let mut request = input();
        request.workflow_type = b"foreign";
        assert!(matches!(
            capture_stored_creation_lifecycle(
                Some(policy),
                request,
                schema,
                &mut charge,
                usize::MAX,
            ),
            Err(CaptureError::Policy(PolicyError::Schema(
                PolicySchemaError::InvalidAuthority,
            ))),
        ));
    }
    let unsupported = StoredPolicyValue::Original(
        [
            b"\x83\x68\x03\x77\x0eflow_policy_v1\x61\x07\x74\x00\x00\x00\x02".as_slice(),
            b"\x77\x04type\x6d\x00\x00\x00\x01r",
            b"\x77\x07version\x77\x1cnot_in_the_canonical_emitter",
        ]
        .concat(),
    );
    assert!(matches!(
        decode_original_complete_policy_entry_portable(unsupported.encoded(), schema, &mut charge),
        Err(PolicyError::UnsupportedPolicyProfile),
    ));
    assert!(matches!(
        capture_stored_creation_lifecycle(
            Some(&unsupported),
            input(),
            schema,
            &mut charge,
            usize::MAX,
        ),
        Err(CaptureError::Policy(PolicyError::UnsupportedPolicyProfile)),
    ));
    let mut invalid_defaults = input();
    invalid_defaults.defaults.ttl_ms = 0;
    assert!(matches!(
        capture_stored_creation_lifecycle(None, invalid_defaults, schema, &mut charge, 0),
        Err(CaptureError::Capture(
            ActiveValueError::InvalidCapturedRetention,
        )),
    ));
    Ok(())
}

#[test]
fn stored_lifecycle_admits_work_and_temporary_owners_before_decode() -> TestResult {
    let (complete, original) = policy_pair()?;
    let schema = PolicySchemaLimits::default();
    let mut charge = PolicySchemaCharge::new(schema);
    assert!(
        decode_original_complete_policy_entry_portable(original.encoded(), schema, &mut charge)?
            .is_some()
    );
    let prefix = (charge.g(), charge.p());
    let result = capture_stored_creation_lifecycle(
        Some(&original),
        input(),
        schema,
        &mut charge,
        usize::MAX,
    );
    assert!(result.is_ok(), "{:?}", result.as_ref().err());
    let Ok(capture) = result else {
        return Err("funded original capture rejected".into());
    };
    assert_eq!(capture.retention_ttl_ms(), 22_000);
    assert!(charge.g() > prefix.0 && charge.p() >= prefix.1);
    let before_refusal = (charge.g(), charge.p());
    // Only Original has a selected temporary decode/index gate. A returned
    // byte refusal is not allocator instrumentation or a Complete-cap0 oracle.
    assert!(matches!(
        capture_stored_creation_lifecycle(Some(&original), input(), schema, &mut charge, 0),
        Err(CaptureError::Policy(PolicyError::ByteLimit)),
    ));
    assert!(charge.g() >= before_refusal.0 && charge.p() >= before_refusal.1);
    let result = capture_stored_creation_lifecycle(
        Some(&original),
        input(),
        schema,
        &mut charge,
        usize::MAX,
    );
    assert!(result.is_ok(), "{:?}", result.as_ref().err());
    let Ok(retried) = result else {
        return Err("healthy funded retry rejected".into());
    };
    assert_eq!(retried.retention_ttl_ms(), 22_000);
    assert!(charge.profile() == schema);
    assert!(charge.total() <= schema.max_work_bytes());
    let tiny = PolicySchemaLimits::new(schema.policy_limits(), 1)?;
    for policy in [&complete, &original] {
        let mut exhausted = PolicySchemaCharge::new(tiny);
        assert!(matches!(
            capture_stored_creation_lifecycle(
                Some(policy),
                input(),
                tiny,
                &mut exhausted,
                usize::MAX,
            ),
            Err(CaptureError::Policy(PolicyError::Schema(
                PolicySchemaError::WorkBytes { maximum: 1, .. },
            ))),
        ));
        assert_eq!((exhausted.g(), exhausted.p()), (0, 0));
    }
    let other_schema =
        PolicySchemaLimits::new(schema.policy_limits(), schema.max_work_bytes() - 1)?;
    let mut other_charge = PolicySchemaCharge::new(other_schema);
    assert!(matches!(
        capture_stored_creation_lifecycle(
            Some(&complete),
            input(),
            schema,
            &mut other_charge,
            usize::MAX,
        ),
        Err(CaptureError::Policy(PolicyError::Schema(
            PolicySchemaError::ProfileMismatch,
        ))),
    ));
    assert_eq!((other_charge.g(), other_charge.p()), (0, 0));
    Ok(())
}

#[test]
fn stored_lifecycle_capture_survives_backing_drop() -> TestResult {
    let captures = {
        let (complete, original) = policy_pair()?;
        let schema = PolicySchemaLimits::default();
        let mut charge = PolicySchemaCharge::new(schema);
        let first = capture_stored_creation_lifecycle(
            Some(&complete),
            input(),
            schema,
            &mut charge,
            usize::MAX,
        );
        assert!(first.is_ok(), "{:?}", first.as_ref().err());
        let Ok(first) = first else {
            return Err("complete capture rejected".into());
        };
        let second = capture_stored_creation_lifecycle(
            Some(&original),
            input(),
            schema,
            &mut charge,
            usize::MAX,
        );
        assert!(second.is_ok(), "{:?}", second.as_ref().err());
        let Ok(second) = second else {
            return Err("original capture rejected".into());
        };
        (first, second)
    };
    // Both full owners and their decode work account have dropped. Only scalar
    // captures cross this scope; no encoded data, snapshot or index is retained.
    for capture in [captures.0, captures.1] {
        assert_eq!(
            (
                capture.max_active_ms(),
                capture.retention_ttl_ms(),
                capture.history_hot_max_events(),
                capture.history_max_events(),
            ),
            (Some(5_000), 22_000, 2, 40),
        );
    }
    Ok(())
}
