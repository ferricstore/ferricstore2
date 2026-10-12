//! Replay/control composition and an independent bounded lease model.

use std::collections::BTreeSet;

use super::*;

#[test]
fn bounded_scan_distinguishes_empty_partial_and_restart() -> TestResult {
    let scope = test_scope()?;
    let request = RequestId::new(b"partial".to_vec())?;
    let a = test_record(b"a", b"other", b"queued", None, 0, 0)?;
    let b = test_record(b"b", b"job", b"queued", None, 0, 0)?;
    let c = test_record(b"c", b"job", b"queued", None, 0, 0)?;
    let entries = [&a, &b, &c].map(|record| ClaimRecord {
        record,
        partition_key: b"p",
        priority: 0,
        state_enter_seq: 1,
    });
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
            let envelope = ClaimEnvelope {
                scan_budget: 1,
                ..test_envelope(test_header(&scope, &request, 1, 25)?, test_query(b"job", 1))
            };
            let first = decide_claim_request(
                view,
                &envelope,
                AdmissionLimits::default(),
                ClaimWorkLimits::default(),
            )?;
            let DispatchDecision::Fresh(ClaimDecision::Accepted(empty_partial)) = first.decision()
            else {
                return Err("expected empty partial".into());
            };
            assert!(empty_partial.batch().is_none());
            assert_eq!(
                (empty_partial.scanned(), empty_partial.stop()),
                (1, SelectionStop::ScanBudget)
            );
            super::admission::check_finalized_resume(
                view,
                &envelope,
                &first,
                (Some(b"b"), SelectionStop::ClaimLimit),
            )?;
            let second = decide_claim_request(
                view,
                &ClaimEnvelope {
                    resume: Some(empty_partial.resume()),
                    ..envelope
                },
                AdmissionLimits::default(),
                ClaimWorkLimits::default(),
            )?;
            let DispatchDecision::Fresh(ClaimDecision::Accepted(mutating_partial)) =
                second.decision()
            else {
                return Err("expected resumed write".into());
            };
            let batch = mutating_partial.batch().ok_or("no resumed batch")?;
            assert_eq!(
                batch.batch().items()[0]
                    .plan()
                    .write()
                    .ok_or("no resumed write")?
                    .record
                    .id()
                    .as_bytes(),
                b"b"
            );
            assert_eq!(mutating_partial.stop(), SelectionStop::ClaimLimit);
            let wrong = decide_claim_request(
                view,
                &ClaimEnvelope {
                    resume: Some(mutating_partial.resume()),
                    ..envelope
                },
                AdmissionLimits::default(),
                ClaimWorkLimits::default(),
            )?;
            assert!(matches!(
                wrong.decision(),
                DispatchDecision::Fresh(ClaimDecision::Rejected(ClaimRejection::StaleContinuation))
            ));
            Ok(())
        },
    )
}

#[test]
fn retained_and_log_replay_do_not_select_or_mint() -> TestResult {
    let scope = test_scope()?;
    let request = RequestId::new(b"replay".to_vec())?;
    let row = test_record(b"a", b"job", b"queued", None, 0, 0)?;
    let entries = [ClaimRecord {
        record: &row,
        partition_key: b"p",
        priority: 0,
        state_enter_seq: 1,
    }];
    let empty_outcomes = BTreeMap::new();
    let first_control = with_claim_view(
        &entries,
        &[PartitionId::new(b"p".to_vec())?],
        GateView {
            scope: &scope,
            control: test_control()?,
            outcomes: &empty_outcomes,
        },
        &BTreeMap::new(),
        1,
        |view| {
            let first = decide_claim_request(
                view,
                &test_envelope(test_header(&scope, &request, 1, 25)?, test_query(b"job", 1)),
                AdmissionLimits::default(),
                ClaimWorkLimits::default(),
            )?;
            assert!(matches!(
                first.decision(),
                DispatchDecision::Fresh(ClaimDecision::Accepted(_))
            ));
            Ok(first.control().next())
        },
    )?;
    for status in [
        RetainedStatus::Accepted,
        RetainedStatus::ClaimRejected(ClaimRejection::ZeroLease),
    ] {
        let outcomes = BTreeMap::from([(
            request.clone(),
            RetainedOutcome::new(
                RetainedMetadata {
                    scope: scope.clone(),
                    request: request.clone(),
                    intent: ClientIntent::new(1, [1; 32])?,
                    original_position: test_identity(1).log(),
                    original_time: TimestampMillis::new(25),
                    expires_at: TimestampMillis::new(100),
                    status,
                },
                1,
                b"exact-original\0bytes".to_vec(),
                AdmissionLimits::default(),
            )?,
        )]);
        with_claim_view(
            &entries,
            &[PartitionId::new(b"p".to_vec())?],
            GateView {
                scope: &scope,
                control: first_control,
                outcomes: &outcomes,
            },
            &BTreeMap::new(),
            2,
            |view| check_retained_status_replay(view, &scope, &request, status),
        )?;
    }
    assert_eq!(
        (row.version(), row.fence().get(), row.attempts()),
        (1, 0, 0)
    );
    Ok(())
}

#[test]
fn claim_phase_replay_matches_independent_bounded_model() -> TestResult {
    let scope = test_scope()?;
    let partition = PartitionId::new(b"p".to_vec())?;
    let mut rows = [
        test_record(b"a", b"job", b"queued", None, 0, 0)?,
        test_record(b"b", b"job", b"queued", None, 0, 1)?,
        test_record(b"c", b"job", b"queued", None, 0, 2)?,
    ];
    // Independent tuple oracle: (id, priority, due, version, fence, logical sequence).
    let mut model = [
        (b"a".to_vec(), 0_u8, 0_u64, 1_u64, 0_u64, 11_u128),
        (b"b".to_vec(), 2, 1, 1, 0, 12),
        (b"c".to_vec(), 1, 2, 1, 0, 13),
    ];
    let mut control = test_control()?;
    let outcomes = BTreeMap::new();
    for index in 1..=12_u64 {
        let now = index * 50;
        let request = RequestId::new(format!("model-{index}").into_bytes())?;
        let entries = rows
            .iter()
            .map(|record| {
                let item = model.iter().find(|item| item.0 == record.id().as_bytes());
                let (priority, sequence) = item.map_or((0, 0), |item| (item.1, item.5));
                ClaimRecord {
                    record,
                    partition_key: b"p",
                    priority,
                    state_enter_seq: sequence,
                }
            })
            .collect::<Vec<_>>();
        let mut expected = model
            .iter()
            .filter(|item| item.2 <= now)
            .collect::<Vec<_>>();
        expected.sort_by_key(|item| (std::cmp::Reverse(item.1), item.2, &item.0));
        let expected_tuples = expected
            .iter()
            .take(2)
            .map(|item| (item.0.as_slice(), item.3 + 1, item.4 + 1, item.5))
            .collect::<Vec<_>>();
        let (writes, next_control) = with_claim_view(
            &entries,
            std::slice::from_ref(&partition),
            GateView {
                scope: &scope,
                control,
                outcomes: &outcomes,
            },
            &BTreeMap::new(),
            index,
            |view| check_model_step(view, &scope, &request, index, now, &expected_tuples),
        )?;
        for write in writes {
            let old = rows
                .iter_mut()
                .find(|record| record.id() == write.id())
                .ok_or("missing modeled row")?;
            let state = model
                .iter_mut()
                .find(|item| item.0 == write.id().as_bytes())
                .ok_or("missing modeled tuple")?;
            state.2 = now + 50;
            state.3 += 1;
            state.4 += 1;
            *old = write;
        }
        control = next_control;
    }
    assert_eq!(control.position(), AppliedPosition::new(12));
    assert!(rows.iter().all(|row| row.attempts() == 0));
    Ok(())
}
fn check_retained_status_replay(
    view: ClaimApplyView<'_>,
    scope: &RequestScope,
    request: &RequestId,
    status: RetainedStatus,
) -> TestResult {
    let missing = BTreeMap::new();
    let corrupt = ClaimApplyView {
        encodings: &missing,
        ..view
    };
    let invalid = ClaimQuery {
        worker: b"",
        states: ClaimStates::Only(&[]),
        ..test_query(b"", 0)
    };
    let duplicate = decide_claim_request(
        corrupt,
        &ClaimEnvelope {
            token_version: 0,
            ..test_envelope(test_header(scope, request, 2, 30)?, invalid)
        },
        AdmissionLimits::default(),
        ClaimWorkLimits::new(1)?,
    )?;
    let DispatchDecision::Replay(original) = duplicate.decision() else {
        return Err("replay selected anew".into());
    };
    let owner = view
        .gate
        .outcomes
        .get(request)
        .ok_or("missing retained fixture")?;
    assert!(std::ptr::eq(*original, owner));
    assert_eq!(original.response().as_ptr(), owner.response().as_ptr());
    assert_eq!(
        (
            original.status(),
            original.original_time(),
            original.original_position(),
            original.expires_at()
        ),
        (
            status,
            TimestampMillis::new(25),
            test_identity(1).log(),
            TimestampMillis::new(100)
        )
    );
    assert!(!duplicate.requires_outcome_write());
    assert_eq!(
        duplicate.control().next().position(),
        AppliedPosition::new(2)
    );
    let envelope = test_envelope(test_header(scope, request, 1, 25)?, invalid);
    check_replay_terminal_guards(corrupt, &envelope)
}

fn check_replay_terminal_guards(
    view: ClaimApplyView<'_>,
    envelope: &ClaimEnvelope<'_>,
) -> TestResult {
    let corrupt = view;
    let scope = envelope.header.scope;
    let request = envelope.header.request;
    let invalid = envelope.query;
    let exact = decide_claim_request(
        corrupt,
        &test_envelope(test_header(scope, request, 1, 25)?, invalid),
        AdmissionLimits::default(),
        ClaimWorkLimits::new(1)?,
    )?;
    assert!(matches!(exact.decision(), DispatchDecision::LogReplay));
    assert_eq!(exact.control().next(), view.gate.control);
    let mut conflict = test_header(scope, request, 2, 30)?;
    conflict.intent = ClientIntent::new(1, [2; 32])?;
    assert!(matches!(
        decide_claim_request(
            corrupt,
            &test_envelope(conflict, invalid),
            AdmissionLimits::default(),
            ClaimWorkLimits::new(1)?
        )?
        .decision(),
        DispatchDecision::IntentConflict
    ));
    assert!(matches!(
        decide_claim_request(
            corrupt,
            &test_envelope(test_header(scope, request, 2, 100)?, invalid),
            AdmissionLimits::default(),
            ClaimWorkLimits::new(1)?
        )?
        .decision(),
        DispatchDecision::Expired
    ));
    Ok(())
}

fn check_model_step(
    view: ClaimApplyView<'_>,
    scope: &RequestScope,
    request: &RequestId,
    index: u64,
    now: u64,
    expected: &[(&[u8], u64, u64, u128)],
) -> Result<(Vec<WorkflowRecord>, ControlState), TestError> {
    let plan = decide_claim_request(
        view,
        &test_envelope(
            test_header(scope, request, index, now)?,
            test_query(b"job", 2),
        ),
        AdmissionLimits::default(),
        ClaimWorkLimits::default(),
    )?;
    let DispatchDecision::Fresh(ClaimDecision::Accepted(claim)) = plan.decision() else {
        return Err("model claim rejected".into());
    };
    let batch = claim.batch().ok_or("model missing writes")?;
    assert_eq!(batch.consumed_ordinals(), 0);
    assert_eq!(
        batch
            .metadata()
            .map(|meta| meta.state_enter_seq)
            .collect::<Vec<_>>(),
        expected.iter().map(|item| item.3).collect::<Vec<_>>()
    );
    assert_eq!(batch.batch().items().len(), expected.len());
    let mut writes = Vec::new();
    for (item, &(expected_id, version, fence, _sequence)) in
        batch.batch().items().iter().zip(expected)
    {
        let next = &item.plan().write().ok_or("model no record")?.record;
        assert_eq!(next.id().as_bytes(), expected_id);
        assert_eq!(
            (next.version(), next.fence().get(), next.attempts()),
            (version, fence, 0)
        );
        assert_eq!(
            next.lease().ok_or("model no lease")?.token.as_bytes(),
            format!("worker-a:{now}:{fence}").as_bytes()
        );
        let history = item.plan().history().ok_or("model no history")?;
        assert_eq!(
            (history.version, history.fence.get(), history.attempts),
            (version, fence, 0)
        );
        assert_eq!(next.next_run_at(), Some(TimestampMillis::new(now + 50)));
        writes.push(next.clone());
    }
    assert_eq!(writes.len(), expected.len());
    let replay_view = ClaimApplyView {
        gate: GateView {
            control: plan.control().next(),
            ..view.gate
        },
        ..view
    };
    let replay = decide_claim_request(
        replay_view,
        &test_envelope(
            test_header(scope, request, index, now)?,
            test_query(b"job", 2),
        ),
        AdmissionLimits::default(),
        ClaimWorkLimits::default(),
    )?;
    assert!(matches!(replay.decision(), DispatchDecision::LogReplay));
    assert_eq!(replay.control().next(), plan.control().next());
    Ok((writes, plan.control().next()))
}

#[test]
fn source_default_parallel_claim_returns_both_same_lane_records() -> TestResult {
    // Pinned invariant_test.exs:5–36, declaration 62328c799831fd82.
    let kind = b"flow-invariant-default-parallel-type".as_slice();
    let partition = b"tenant:default-parallel".as_slice();
    let ids = [
        b"z-default-parallel-first".as_slice(),
        b"a-default-parallel-second".as_slice(),
    ];
    let mut rows = [(ids[0], 1_000), (ids[1], 1_001)]
        .into_iter()
        .map(|(id, now)| {
            let record = test_record(id, kind, b"queued", Some(id.to_vec()), now, 2_000)?;
            let actual = (record.workflow_type(), record.state(), record.payload());
            assert_eq!(actual, (kind, b"queued".as_slice(), Some(id)));
            Ok(record)
        })
        .collect::<Result<Vec<_>, TestError>>()?;
    let scope = test_scope()?;
    let request = RequestId::new(b"default-parallel".to_vec())?;
    let partitions = [PartitionId::new(partition.to_vec())?];
    let filter = [partition];
    let states = [b"queued".as_slice()];
    let entries = rows
        .iter()
        .enumerate()
        .map(|(index, record)| ClaimRecord {
            record,
            partition_key: partition,
            priority: 1,
            state_enter_seq: 1 + index as u128,
        })
        .collect::<Vec<_>>();
    let outcomes = BTreeMap::new();
    let query = ClaimQuery {
        partitions: PartitionFilter::Only(&filter),
        states: ClaimStates::Only(&states),
        priority: Some(1),
        worker: b"parallel-worker",
        lease_ms: 30_000,
        ..test_query(kind, 10)
    };
    let (writes, control) = with_claim_view(
        &entries,
        &partitions,
        GateView {
            scope: &scope,
            control: test_control()?,
            outcomes: &outcomes,
        },
        &BTreeMap::new(),
        1,
        |view| {
            let plan = decide_claim_request(
                view,
                &test_envelope(test_header(&scope, &request, 1, 2_000)?, query),
                AdmissionLimits::default(),
                ClaimWorkLimits::default(),
            )?;
            assert!(matches!(
                plan.decision(),
                DispatchDecision::Fresh(ClaimDecision::Accepted(_))
            ));
            let DispatchDecision::Fresh(ClaimDecision::Accepted(claim)) = plan.decision() else {
                return Err("source parallel claim was not fresh acceptance".into());
            };
            let batch = claim.batch().ok_or("source parallel claim has no batch")?;
            let writes = batch
                .batch()
                .items()
                .iter()
                .map(|item| {
                    let write = item.plan().write().ok_or("missing source parallel write")?;
                    Ok(write.record.clone())
                })
                .collect::<Result<Vec<_>, TestError>>()?;
            let actual = writes
                .iter()
                .map(|record| record.id().as_bytes())
                .collect::<BTreeSet<_>>();
            assert_eq!(actual, BTreeSet::from(ids));
            let complete = (2, SelectionStop::Complete);
            assert_eq!((batch.batch().items().len(), claim.stop()), complete);
            Ok((writes, plan.control().next()))
        },
    )?;
    rows = writes;
    assert!(
        rows.iter()
            .all(|record| record.state() == b"running" && record.lease().is_some())
    );
    assert_eq!(control.position(), AppliedPosition::new(1));
    Ok(())
}
