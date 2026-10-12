//! Query/authority bounds with unchanged named control bodies.

use super::*;

#[test]
fn query_and_authority_admission_preserve_error_classes() -> TestResult {
    let scope = test_scope()?;
    let request = RequestId::new(b"bounds".to_vec())?;
    let outcomes = BTreeMap::new();
    with_claim_view(
        &[],
        &[],
        GateView {
            scope: &scope,
            control: test_control()?,
            outcomes: &outcomes,
        },
        &BTreeMap::new(),
        1,
        |view| {
            check_query_admission(view, &scope, &request)?;
            let excessive = [b"queued".as_slice(); 65];
            let duplicate = [b"queued".as_slice(); 2];
            for states in [excessive.as_slice(), duplicate.as_slice()] {
                let query = ClaimQuery {
                    states: ClaimStates::Only(states),
                    priority: Some(3),
                    ..test_query(b"job", 1)
                };
                let plan = decide_claim_request(
                    view,
                    &test_envelope(test_header(&scope, &request, 1, 25)?, query),
                    AdmissionLimits::default(),
                    ClaimWorkLimits::default(),
                )?;
                assert!(matches!(
                    plan.decision(),
                    DispatchDecision::Fresh(ClaimDecision::Rejected(ClaimRejection::InvalidScope))
                ));
            }
            Ok(())
        },
    )?;
    let row = test_record(b"a", b"job", b"queued", None, 0, 0)?;
    let entries = [ClaimRecord {
        record: &row,
        partition_key: b"p",
        priority: 0,
        state_enter_seq: 1,
    }];
    with_claim_view(
        &entries,
        &[PartitionId::new(b"p".to_vec())?],
        GateView {
            scope: &scope,
            control: test_control()?,
            outcomes: &outcomes,
        },
        &BTreeMap::new(),
        1,
        |view| check_generated_token_admission(view, &scope, &request),
    )?;
    // Empty lane metadata and an unpopped parallel lookahead must be admitted too.
    let policy = test_fifo_policy(&[b"queued"])?;
    let policies = BTreeMap::from([(b"job".as_slice(), policy.policy())]);
    with_claim_view(
        &entries,
        &[PartitionId::new(b"p".to_vec())?],
        GateView {
            scope: &scope,
            control: test_control()?,
            outcomes: &outcomes,
        },
        &policies,
        1,
        |view| {
            check_lane_and_lookahead_admission(view, &scope, &request)?;
            super::lookahead::check_repeated_lookahead_charge(view, &scope, &request)?;
            super::lookahead::check_blocked_head_admission(view, &scope, &request)
        },
    )
}

#[test]
fn claim_rejections_exclude_operational_authority_failures() -> TestResult {
    let scope = test_scope()?;
    let request = RequestId::new(b"classes".to_vec())?;
    let row = test_record(b"a", b"job", b"queued", Some(vec![0; 100]), 0, 0)?;
    let entries = [ClaimRecord {
        record: &row,
        partition_key: b"p",
        priority: 0,
        state_enter_seq: 1,
    }];
    let outcomes = BTreeMap::new();
    with_claim_view(
        &entries,
        &[PartitionId::new(b"p".to_vec())?],
        GateView {
            scope: &scope,
            control: test_control()?,
            outcomes: &outcomes,
        },
        &BTreeMap::new(),
        1,
        |view| {
            assert!(matches!(
                decide_claim_request(
                    view,
                    &test_envelope(test_header(&scope, &request, 1, 25)?, test_query(b"job", 1)),
                    AdmissionLimits::new(99, 100_000, 1)?,
                    ClaimWorkLimits::default()
                ),
                Err(ApplyError::InvalidAuthority)
            ));
            let empty = BTreeMap::new();
            assert!(matches!(
                decide_claim_request(
                    ClaimApplyView {
                        encodings: &empty,
                        ..view
                    },
                    &test_envelope(test_header(&scope, &request, 1, 25)?, test_query(b"job", 1)),
                    AdmissionLimits::default(),
                    ClaimWorkLimits::default()
                ),
                Err(ApplyError::InvalidAuthority)
            ));
            let unsupported = ClaimEnvelope {
                token_version: 2,
                ..test_envelope(test_header(&scope, &request, 1, 25)?, test_query(b"job", 1))
            };
            assert!(matches!(
                decide_claim_request(
                    view,
                    &unsupported,
                    AdmissionLimits::default(),
                    ClaimWorkLimits::default()
                ),
                Err(ApplyError::InvalidAuthority)
            ));
            let retained = RetainedOutcome::new(
                RetainedMetadata {
                    scope: scope.clone(),
                    request: request.clone(),
                    intent: ClientIntent::new(1, [1; 32])?,
                    original_position: test_identity(1).log(),
                    original_time: TimestampMillis::new(25),
                    expires_at: TimestampMillis::new(100),
                    status: RetainedStatus::ClaimRejected(ClaimRejection::ZeroLease),
                },
                1,
                b"zero-lease".to_vec(),
                AdmissionLimits::default(),
            )?;
            assert_eq!(
                retained.status(),
                RetainedStatus::ClaimRejected(ClaimRejection::ZeroLease)
            );
            assert_eq!(
                ClaimRejection::ZeroLease.to_string(),
                "ERR flow lease_ms must be a positive integer"
            );
            Ok(())
        },
    )
}

fn check_query_admission(
    view: ClaimApplyView<'_>,
    scope: &RequestScope,
    request: &RequestId,
) -> TestResult {
    for (query, expected) in [
        (
            ClaimQuery {
                states: ClaimStates::Only(&[]),
                ..test_query(b"job", 1)
            },
            ClaimRejection::InvalidScope,
        ),
        (test_query(b"", 1), ClaimRejection::EmptyType),
        (
            ClaimQuery {
                worker: b"",
                ..test_query(b"job", 1)
            },
            ClaimRejection::EmptyWorker,
        ),
        (
            ClaimQuery {
                lease_ms: 0,
                ..test_query(b"job", 1)
            },
            ClaimRejection::ZeroLease,
        ),
        (
            ClaimQuery {
                lease_ms: u64::MAX,
                ..test_query(b"job", 1)
            },
            ClaimRejection::LeaseTooLarge,
        ),
        (
            ClaimQuery {
                lease_ms: wf_domain::workflow::MAX_EXACT_MILLIS,
                ..test_query(b"job", 1)
            },
            ClaimRejection::LeaseDeadlineTooLarge,
        ),
        (
            ClaimQuery {
                priority: Some(3),
                ..test_query(b"job", 1)
            },
            ClaimRejection::InvalidPriority,
        ),
        (test_query(b"job", 0), ClaimRejection::InvalidBudget),
    ] {
        let result = decide_claim_request(
            view,
            &test_envelope(test_header(scope, request, 1, 25)?, query),
            AdmissionLimits::default(),
            ClaimWorkLimits::default(),
        )?;
        assert!(
            matches!(result.decision(), DispatchDecision::Fresh(ClaimDecision::Rejected(actual)) if *actual == expected)
        );
    }
    for length in [1_025, 65_535, 65_536] {
        let name = vec![b'x'; length];
        let result = decide_claim_request(
            view,
            &test_envelope(test_header(scope, request, 1, 25)?, test_query(&name, 1)),
            AdmissionLimits::new(100_000, 64 * 1_048_576, 1)?,
            ClaimWorkLimits::new(u64::MAX)?,
        )?;
        if length <= 65_535 {
            assert!(matches!(
                result.decision(),
                DispatchDecision::Fresh(ClaimDecision::Accepted(_))
            ));
        } else {
            assert!(matches!(
                result.decision(),
                DispatchDecision::Fresh(ClaimDecision::Rejected(ClaimRejection::ValueTooLarge {
                    bytes: 65_536,
                    maximum: 65_535
                }))
            ));
        }
    }
    Ok(())
}

fn check_generated_token_admission(
    view: ClaimApplyView<'_>,
    scope: &RequestScope,
    request: &RequestId,
) -> TestResult {
    for (inline, expected) in [
        (13, None),
        (
            12,
            Some(ClaimRejection::ValueTooLarge {
                bytes: 13,
                maximum: 12,
            }),
        ),
    ] {
        let result = decide_claim_request(
            view,
            &test_envelope(test_header(scope, request, 1, 25)?, test_query(b"job", 1)),
            AdmissionLimits::new(inline, 100_000, 1)?,
            ClaimWorkLimits::default(),
        )?;
        match (result.decision(), expected) {
            (DispatchDecision::Fresh(ClaimDecision::Accepted(_)), None) => {}
            (DispatchDecision::Fresh(ClaimDecision::Rejected(actual)), Some(expected)) => {
                assert_eq!(*actual, expected);
            }
            _ => return Err("generated token boundary failed".into()),
        }
    }
    let entry = *view
        .group
        .records
        .first_key_value()
        .ok_or("missing fixture authority")?
        .1;
    // Profile bounds of each actual owner precede coherence equality.
    let huge = vec![b'x'; 65_536];
    let foreign = test_record(b"a", &huge, b"queued", None, 0, 0)?;
    let scoped = ScopedRecordId::for_record(&entry);
    for indexed_owner in [true, false] {
        let mut records = view.group.records.clone();
        let mut canonical = view.group.batch.records.clone();
        if indexed_owner {
            records.insert(
                scoped,
                ClaimRecord {
                    record: &foreign,
                    ..entry
                },
            );
        } else {
            canonical.insert(scoped, &foreign);
        }
        let result = decide_claim_request(
            ClaimApplyView {
                group: IndexedGroupView {
                    batch: GroupView {
                        records: &canonical,
                        ..view.group.batch
                    },
                    records: &records,
                    ..view.group
                },
                ..view
            },
            &test_envelope(test_header(scope, request, 1, 25)?, test_query(b"job", 1)),
            AdmissionLimits::new(100_000, 64 * 1_048_576, 1)?,
            ClaimWorkLimits::new(u64::MAX)?,
        )?;
        assert!(matches!(
            result.decision(),
            DispatchDecision::Fresh(ClaimDecision::Rejected(ClaimRejection::ValueTooLarge {
                bytes: 65_536,
                maximum: 65_535
            }))
        ));
    }
    Ok(())
}

fn check_lane_and_lookahead_admission(
    view: ClaimApplyView<'_>,
    scope: &RequestScope,
    request: &RequestId,
) -> TestResult {
    let huge = vec![b'x'; 65_536];
    let mut fifo = view.group.fifo.clone();
    fifo.insert(
        LaneKey {
            workflow_type: &huge,
            state: b"queued",
            partition_key: b"p",
        },
        BTreeMap::new(),
    );
    let oversized = ClaimApplyView {
        group: IndexedGroupView {
            fifo: &fifo,
            ..view.group
        },
        ..view
    };
    let result = decide_claim_request(
        oversized,
        &test_envelope(test_header(scope, request, 1, 25)?, test_query(b"job", 1)),
        AdmissionLimits::new(100_000, 64 * 1_048_576, 1)?,
        ClaimWorkLimits::new(u64::MAX)?,
    )?;
    assert!(matches!(
        result.decision(),
        DispatchDecision::Fresh(ClaimDecision::Rejected(ClaimRejection::ValueTooLarge {
            bytes: 65_536,
            maximum: 65_535
        }))
    ));
    let mut parallel = view.group.parallel.clone();
    parallel.insert(
        DueOrder {
            priority: 0,
            due: TimestampMillis::new(1),
            id: b"peek",
            workflow_type: &huge,
            state: b"queued",
            partition_key: b"p",
        },
        *view
            .group
            .records
            .first_key_value()
            .ok_or("missing fixture authority")?
            .1,
    );
    let lookahead = ClaimApplyView {
        group: IndexedGroupView {
            parallel: &parallel,
            ..view.group
        },
        ..view
    };
    let result = decide_claim_request(
        lookahead,
        &test_envelope(test_header(scope, request, 1, 25)?, test_query(b"job", 1)),
        AdmissionLimits::new(100_000, 64 * 1_048_576, 1)?,
        ClaimWorkLimits::new(u64::MAX)?,
    )?;
    assert!(matches!(
        result.decision(),
        DispatchDecision::Fresh(ClaimDecision::Rejected(ClaimRejection::ValueTooLarge {
            bytes: 65_536,
            maximum: 65_535
        }))
    ));
    // FIFO-only pre-copy scalar prefix: Q11*256+entry11+lane10*256+head1024+D60+N17*256=10823;
    // the prospective first scan needs another1024, before selecting/owning a command.
    let prefix = decide_claim_request(
        view,
        &test_envelope(test_header(scope, request, 1, 25)?, test_query(b"job", 1)),
        AdmissionLimits::default(),
        ClaimWorkLimits::new(11_846)?,
    )?;
    assert!(matches!(
        prefix.decision(),
        DispatchDecision::Fresh(ClaimDecision::Rejected(ClaimRejection::WorkBytes {
            bytes: 11_847,
            maximum: 11_846
        }))
    ));
    Ok(())
}
