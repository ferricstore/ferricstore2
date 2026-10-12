//! Independent logical-byte boundaries and business/authority distinctions.

use super::*;

mod bounds;
mod lookahead;

#[test]
fn selected_fifo_preflight_matches_nested_record_charge() -> TestResult {
    let scope = test_scope()?;
    let request = RequestId::new(b"fifo-bytes".to_vec())?;
    let row = test_record(b"a", b"job", b"queued", Some(vec![7; 1_024]), 0, 0)?;
    let entries = [ClaimRecord {
        record: &row,
        partition_key: b"p",
        priority: 0,
        state_enter_seq: 1,
    }];
    let candidate = test_fifo_policy(&[b"queued"])?;
    let policies = BTreeMap::from([(b"job".as_slice(), candidate.policy())]);
    let outcomes = BTreeMap::new();
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
            check_fifo_plan_boundaries(view, &scope, &request)?;
            assert_eq!(row.version(), 1);
            Ok(())
        },
    )?;
    // Two1MiB FIFO payloads need over24MiB at the unchanged nested indexed gate.
    let rows = [
        test_record(b"a", b"job", b"queued", Some(vec![0; 1_048_576]), 0, 0)?,
        test_record(b"b", b"job", b"queued", Some(vec![0; 1_048_576]), 0, 0)?,
    ];
    let entries = [
        ClaimRecord {
            record: &rows[0],
            partition_key: b"p",
            priority: 0,
            state_enter_seq: 1,
        },
        ClaimRecord {
            record: &rows[1],
            partition_key: b"q",
            priority: 0,
            state_enter_seq: 2,
        },
    ];
    let partitions = [
        PartitionId::new(b"p".to_vec())?,
        PartitionId::new(b"q".to_vec())?,
    ];
    with_claim_view(
        &entries,
        &partitions,
        GateView {
            scope: &scope,
            control: test_control()?,
            outcomes: &outcomes,
        },
        &policies,
        1,
        |view| {
            let plan = decide_claim_request(
                view,
                &test_envelope(test_header(&scope, &request, 1, 25)?, test_query(b"job", 2)),
                AdmissionLimits::default(),
                ClaimWorkLimits::default(),
            )?;
            assert!(matches!(
                plan.decision(),
                DispatchDecision::Fresh(ClaimDecision::Rejected(ClaimRejection::DecisionBytes))
            ));
            let larger = decide_claim_request(
                view,
                &test_envelope(test_header(&scope, &request, 1, 25)?, test_query(b"job", 2)),
                AdmissionLimits::new(1_048_576, 32 * 1_048_576, 2)?,
                ClaimWorkLimits::default(),
            )?;
            assert!(matches!(
                larger.decision(),
                DispatchDecision::Fresh(ClaimDecision::Accepted(_))
            ));
            Ok(())
        },
    )
}

#[test]
fn trusted_work_profile_and_resume_require_same_effective_limits() -> TestResult {
    assert_eq!(ClaimWorkLimits::new(0), Err(AdmissionError::InvalidLimits));
    assert_eq!(ClaimWorkLimits::new(u64::MAX)?.max_work_bytes(), u64::MAX);
    assert_eq!(
        ClaimWorkLimits::default().max_work_bytes(),
        128 * 1_024 * 1_024
    );
    let scope = test_scope()?;
    let request = RequestId::new(b"resume".to_vec())?;
    let rows = [
        test_record(b"a", b"job", b"queued", None, 0, 0)?,
        test_record(b"b", b"job", b"queued", None, 0, 0)?,
    ];
    let entries = rows
        .iter()
        .map(|record| ClaimRecord {
            record,
            partition_key: b"p",
            priority: 0,
            state_enter_seq: 1,
        })
        .collect::<Vec<_>>();
    let outcomes = BTreeMap::new();
    let states = [b"waiting".as_slice()];
    with_claim_view(
        &entries,
        &[PartitionId::new(b"p".to_vec())?],
        GateView {
            scope: &scope,
            control: test_control()?,
            outcomes: &outcomes,
        },
        &BTreeMap::new(),
        9,
        |view| {
            let query = ClaimQuery {
                states: ClaimStates::Only(&states),
                ..test_query(b"job", 1)
            };
            let envelope = ClaimEnvelope {
                scan_budget: 1,
                ..test_envelope(test_header(&scope, &request, 1, 25)?, query)
            };
            let first = decide_claim_request(
                view,
                &envelope,
                AdmissionLimits::default(),
                ClaimWorkLimits::default(),
            )?;
            let DispatchDecision::Fresh(ClaimDecision::Accepted(claim)) = first.decision() else {
                return Err("zero-write partial not accepted".into());
            };
            assert!(claim.batch().is_none());
            assert_eq!(claim.stop(), SelectionStop::ScanBudget);
            check_finalized_resume(view, &envelope, &first, (None, SelectionStop::Complete))?;
            let resumed = ClaimEnvelope {
                resume: Some(claim.resume()),
                ..envelope
            };
            check_resume_mismatches(view, &resumed)
        },
    )
}

fn check_fifo_plan_boundaries(
    view: ClaimApplyView<'_>,
    scope: &RequestScope,
    request: &RequestId,
) -> TestResult {
    // R=1034, B=1072, F=2068, Q=11. P=4*(Q+B+F)+1024=13628.
    // G=entry11+D6204+T10340+256*N38+1024*(head1+scan1)=28331.
    for (plan_bytes, work_bytes, expected) in [
        (13_628, 41_959, None),
        (13_627, 41_959, Some(ClaimRejection::DecisionBytes)),
        (
            13_628,
            41_958,
            Some(ClaimRejection::WorkBytes {
                bytes: 41_959,
                maximum: 41_958,
            }),
        ),
    ] {
        let plan = decide_claim_request(
            view,
            &test_envelope(test_header(scope, request, 1, 25)?, test_query(b"job", 1)),
            AdmissionLimits::new(2_048, plan_bytes, 1)?,
            ClaimWorkLimits::new(work_bytes)?,
        )?;
        match (plan.decision(), expected) {
            (DispatchDecision::Fresh(ClaimDecision::Accepted(claim)), None) => {
                assert_eq!(
                    claim
                        .batch()
                        .ok_or("missing exact-boundary batch")?
                        .batch()
                        .items()
                        .len(),
                    1
                );
            }
            (DispatchDecision::Fresh(ClaimDecision::Rejected(actual)), Some(expected)) => {
                assert_eq!(*actual, expected);
            }
            _ => return Err("wrong pre-copy FIFO boundary".into()),
        }
    }
    Ok(())
}

fn check_resume_mismatches(view: ClaimApplyView<'_>, envelope: &ClaimEnvelope<'_>) -> TestResult {
    let resumed = *envelope;
    let query = envelope.query;
    let scope = envelope.header.scope;
    let request = envelope.header.request;
    let same = decide_claim_request(
        view,
        &resumed,
        AdmissionLimits::default(),
        ClaimWorkLimits::default(),
    )?;
    let DispatchDecision::Fresh(ClaimDecision::Accepted(second)) = same.decision() else {
        return Err("same-snapshot resume failed".into());
    };
    assert_eq!(second.stop(), SelectionStop::Complete);
    for (limits, work) in [
        (
            AdmissionLimits::new(1_048_576, 16 * 1_048_576 + 1, 1_000)?,
            ClaimWorkLimits::default(),
        ),
        (
            AdmissionLimits::default(),
            ClaimWorkLimits::new(128 * 1_048_576 + 1)?,
        ),
    ] {
        let mismatch = decide_claim_request(view, &resumed, limits, work)?;
        assert!(matches!(
            mismatch.decision(),
            DispatchDecision::Fresh(ClaimDecision::Rejected(ClaimRejection::StaleContinuation))
        ));
    }
    for changed in [
        ClaimEnvelope {
            query: ClaimQuery {
                worker: b"other",
                ..query
            },
            ..resumed
        },
        ClaimEnvelope {
            query: ClaimQuery {
                lease_ms: 51,
                ..query
            },
            ..resumed
        },
        ClaimEnvelope {
            query: ClaimQuery { limit: 2, ..query },
            ..resumed
        },
        ClaimEnvelope {
            scan_budget: 2,
            ..resumed
        },
        ClaimEnvelope {
            head_budget: 1,
            ..resumed
        },
        ClaimEnvelope {
            header: test_header(scope, request, 1, 26)?,
            ..resumed
        },
    ] {
        let mismatch = decide_claim_request(
            view,
            &changed,
            AdmissionLimits::default(),
            ClaimWorkLimits::default(),
        )?;
        assert!(matches!(
            mismatch.decision(),
            DispatchDecision::Fresh(ClaimDecision::Rejected(ClaimRejection::StaleContinuation))
        ));
    }
    let changed = decide_claim_request(
        ClaimApplyView {
            revision: 10,
            ..view
        },
        &resumed,
        AdmissionLimits::default(),
        ClaimWorkLimits::default(),
    )?;
    assert!(matches!(
        changed.decision(),
        DispatchDecision::Fresh(ClaimDecision::Rejected(ClaimRejection::StaleContinuation))
    ));
    Ok(())
}

pub(super) fn check_finalized_resume<'a>(
    view: ClaimApplyView<'a>,
    envelope: &ClaimEnvelope<'a>,
    first: &DispatchPlan<'a, ClaimDecision<'a>>,
    expected: (Option<&[u8]>, SelectionStop),
) -> TestResult {
    let DispatchDecision::Fresh(ClaimDecision::Accepted(claim)) = first.decision() else {
        return Err("zero-write partial not accepted".into());
    };
    assert!(claim.batch().is_none());
    assert_eq!(
        (claim.scanned(), claim.stop()),
        (1, SelectionStop::ScanBudget)
    );
    let scope = envelope.header.scope;
    let request = envelope.header.request;
    let retained = BTreeMap::from([(
        request.clone(),
        RetainedOutcome::new(
            RetainedMetadata {
                scope: scope.clone(),
                request: request.clone(),
                intent: envelope.header.intent,
                original_position: envelope.header.identity.log(),
                original_time: TimestampMillis::new(25),
                expires_at: TimestampMillis::new(100),
                status: RetainedStatus::Accepted,
            },
            1,
            b"original-partial".to_vec(),
            AdmissionLimits::default(),
        )?,
    )]);
    let next_view = ClaimApplyView {
        gate: GateView {
            control: first.control().next(),
            outcomes: &retained,
            ..view.gate
        },
        ..view
    };
    let duplicate = decide_claim_request(
        next_view,
        &ClaimEnvelope {
            header: test_header(scope, request, 2, 25)?,
            query: ClaimQuery {
                worker: b"",
                ..envelope.query
            },
            resume: Some(claim.resume()),
            ..*envelope
        },
        AdmissionLimits::default(),
        ClaimWorkLimits::default(),
    )?;
    let DispatchDecision::Replay(original) = duplicate.decision() else {
        return Err("partial ID reran".into());
    };
    assert_eq!(original.response(), b"original-partial");
    assert_eq!(original.original_position(), test_identity(1).log());
    assert_eq!(
        (original.original_time(), original.expires_at()),
        (TimestampMillis::new(25), TimestampMillis::new(100))
    );
    assert!(!duplicate.requires_outcome_write());
    let fresh_id = RequestId::new(b"resume-fresh".to_vec())?;
    let fresh = decide_claim_request(
        next_view,
        &ClaimEnvelope {
            header: test_header(scope, &fresh_id, 2, 25)?,
            resume: Some(claim.resume()),
            ..*envelope
        },
        AdmissionLimits::default(),
        ClaimWorkLimits::default(),
    )?;
    let DispatchDecision::Fresh(ClaimDecision::Accepted(continued)) = fresh.decision() else {
        return Err("fresh finalized-ID continuation failed".into());
    };
    if let Some(id) = expected.0 {
        let actual = continued
            .batch()
            .ok_or("fresh continuation missing b")?
            .batch()
            .items();
        assert_eq!(actual.len(), 1);
        assert_eq!(
            actual[0]
                .plan()
                .write()
                .ok_or("fresh continuation no write")?
                .record
                .id()
                .as_bytes(),
            id
        );
    } else {
        assert!(continued.batch().is_none());
    }
    assert_eq!((continued.scanned(), continued.stop()), (1, expected.1));
    assert_eq!(fresh.control().next().position(), AppliedPosition::new(2));
    Ok(())
}
