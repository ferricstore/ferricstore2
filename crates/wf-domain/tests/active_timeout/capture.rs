//! Creation-time lifecycle and explicit restoration controls.
use super::*;

#[test]
fn captured_active_override_infinity_and_live_policy_changes() -> TestResult {
    let defaults = LifecycleDefaults {
        ttl_ms: 1_000,
        history_hot_max_events: 0,
        history_max_events: 100,
        max_history_hot_max_events: 10_000,
        max_history_max_events: 1_000_000,
    };
    let policy = restore_candidate(
        1,
        PolicySnapshotData {
            workflow_type: b"job".to_vec(),
            version: None,
            max_active_ms: Some(10_000),
            retry: RetryOverride::default(),
            retention: RetentionOverride::default(),
            states: BTreeMap::new(),
        },
        PolicyLimits::default(),
    )?;
    let overrides = LifecycleOverrides {
        active: MaxActiveOverride::Millis(100),
        retention: RetentionOverride::default(),
        history_hot_max_events: None,
    };
    let captured = capture_lifecycle(
        Some(policy.policy()),
        b"job",
        b"queued",
        overrides,
        defaults,
    )?;
    assert_eq!(captured.max_active_ms(), Some(100));
    let infinity = capture_lifecycle(
        Some(policy.policy()),
        b"job",
        b"queued",
        LifecycleOverrides {
            active: MaxActiveOverride::Infinite,
            ..overrides
        },
        defaults,
    )?;
    assert_eq!(infinity.max_active_ms(), None);
    let record = restore_active(
        fixture(
            b"one",
            b"p",
            b"queued",
            TimestampMillis::new(0),
            Some(TimestampMillis::new(0)),
            captured,
        )?,
        AdmissionLimits::default(),
    )?;
    with_view(
        &[record],
        &BTreeMap::new(),
        &BTreeMap::new(),
        ReleaseOutboxState { head: 1, tail: 0 },
        |view| {
            let root = ScopedRecordId {
                partition_key: b"p",
                id: b"one",
            };
            let order = EntryOrder {
                identity: LogIdentity::new(1, 7, AppliedPosition::new(11)),
                first_ordinal: 0,
            };
            let before = expire_at(
                view,
                &[root],
                TimestampMillis::new(99),
                order,
                (AdmissionLimits::default(), TimeoutLimits::default()),
            )?;
            let ActiveTimeoutDecision::Accepted(before) = before else {
                return Err("before deadline rejected".into());
            };
            assert_eq!(before.stats(), 0);
            assert!(before.writes().is_empty());
            let at = expire_at(
                view,
                &[root],
                TimestampMillis::new(100),
                order,
                (AdmissionLimits::default(), TimeoutLimits::default()),
            )?;
            let ActiveTimeoutDecision::Accepted(at) = at else {
                return Err("deadline rejected".into());
            };
            assert_eq!(at.stats(), 1);
            assert_eq!(at.writes()[0].record.captured().max_active_ms(), Some(100));
            Ok(())
        },
    )
}

#[test]
fn captured_retention_layers_hot_caps_and_numeric_extremes() -> TestResult {
    let defaults = LifecycleDefaults {
        ttl_ms: 1_000,
        history_hot_max_events: 0,
        history_max_events: 100,
        max_history_hot_max_events: u64::MAX,
        max_history_max_events: 1_000_000,
    };
    let mut states = BTreeMap::new();
    states.insert(
        b"queued".to_vec(),
        StatePolicy {
            mode: None,
            retry: RetryOverride::default(),
            retention: RetentionOverride {
                ttl_ms: Some(200),
                history_max_events: Some(5),
            },
        },
    );
    let policy = restore_candidate(
        1,
        PolicySnapshotData {
            workflow_type: b"job".to_vec(),
            version: None,
            max_active_ms: None,
            retry: RetryOverride::default(),
            retention: RetentionOverride {
                ttl_ms: Some(300),
                history_max_events: Some(10),
            },
            states,
        },
        PolicyLimits::default(),
    )?;
    let overrides = LifecycleOverrides {
        active: MaxActiveOverride::Inherit,
        retention: RetentionOverride {
            ttl_ms: Some(100),
            history_max_events: None,
        },
        history_hot_max_events: Some(u64::MAX),
    };
    let captured = capture_lifecycle(
        Some(policy.policy()),
        b"job",
        b"queued",
        overrides,
        defaults,
    )?;
    assert_eq!(
        (
            captured.retention_ttl_ms(),
            captured.history_hot_max_events(),
            captured.history_max_events()
        ),
        (100, 5, 5)
    );
    assert!(
        capture_lifecycle(
            Some(policy.policy()),
            b"foreign",
            b"queued",
            overrides,
            defaults
        )
        .is_err()
    );
    let too_hot = LifecycleOverrides {
        retention: RetentionOverride {
            ttl_ms: None,
            history_max_events: Some(1),
        },
        history_hot_max_events: Some(2),
        ..overrides
    };
    assert!(capture_lifecycle(None, b"job", b"queued", too_hot, defaults).is_err());
    for active in [
        MaxActiveOverride::Millis(0),
        MaxActiveOverride::Millis(31_536_000_001),
    ] {
        let request = LifecycleOverrides {
            active,
            ..overrides
        };
        assert!(capture_lifecycle(None, b"job", b"queued", request, defaults).is_err());
    }
    let hot_zero = LifecycleOverrides {
        history_hot_max_events: Some(0),
        ..overrides
    };
    let zero = capture_lifecycle(None, b"job", b"queued", hot_zero, defaults)?;
    assert_eq!(zero.history_hot_max_events(), 0);
    Ok(())
}

#[test]
fn restoration_moves_owners_and_rejects_incoherent_lifecycle() -> TestResult {
    let payload = vec![5; 64];
    let payload_pointer = payload.as_ptr();
    let state = b"running".to_vec();
    let state_pointer = state.as_ptr();
    let data = WorkflowRecordData {
        id: RecordId::new(b"r".to_vec())?,
        workflow_type: b"job".to_vec(),
        state,
        version: 3,
        fence: Fence::new(2),
        attempts: 4,
        created_at: TimestampMillis::new(1),
        updated_at: TimestampMillis::new(9),
        next_run_at: None,
        run_state: None,
        lease: None,
        payload: Some(payload),
        result: None,
        error: None,
        latest_kind: EventKind::ChildrenSpawned,
        latest_version: 3,
        latest_fence: Fence::new(2),
    };
    let core = restore_record(data, AdmissionLimits::default())?;
    assert_eq!(core.status(), WorkflowStatus::Running);
    assert!(core.lease().is_none());
    assert_eq!(core.state().as_ptr(), state_pointer);
    assert_eq!(
        core.payload().ok_or("missing payload")?.as_ptr(),
        payload_pointer
    );
    let entry = ClaimRecord {
        record: &core,
        partition_key: b"p",
        priority: 0,
        state_enter_seq: 11,
    };
    assert_eq!(entry.logical_state(), b"queued");
    assert_eq!(LaneOrder::for_record(&entry).score(), -1);
    assert!(DueOrder::for_record(&entry).is_none());
    let defaults = LifecycleDefaults {
        ttl_ms: 100,
        history_hot_max_events: 0,
        history_max_events: 1,
        max_history_hot_max_events: 10_000,
        max_history_max_events: 1_000_000,
    };
    let captured = capture_lifecycle(
        None,
        b"job",
        b"waiting",
        LifecycleOverrides {
            active: MaxActiveOverride::Infinite,
            retention: RetentionOverride::default(),
            history_hot_max_events: None,
        },
        defaults,
    )?;
    let mut active = fixture(
        b"w",
        b"p",
        b"waiting",
        TimestampMillis::new(0),
        None,
        captured,
    )?;
    active.carry.attributes_history = Some(Vec::new());
    let record = restore_active(active, AdmissionLimits::default())?;
    assert_eq!(record.core().status(), WorkflowStatus::Unscheduled);
    assert_eq!(record.carry().attributes_history.as_deref(), Some(&[][..]));
    let mut invalid = fixture(
        b"bad",
        b"p",
        b"queued",
        TimestampMillis::new(0),
        Some(TimestampMillis::new(0)),
        captured,
    )?;
    invalid.terminal_retention_until = Some(TimestampMillis::new(10));
    assert!(restore_active(invalid, AdmissionLimits::default()).is_err());
    Ok(())
}
