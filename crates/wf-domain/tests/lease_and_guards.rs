//! Source-derived lease/cancel/duplicate guards plus deterministic safety checks.

#![forbid(unsafe_code)]

mod support;

use support::{Scenario, TestResult};
use wf_domain::claim::Claim;
use wf_domain::history::EventKind;
use wf_domain::lease::{ExtendLease, LeaseGuard, LeaseToken};
use wf_domain::retry::{Retry, RetryTime};
use wf_domain::workflow::{
    Cancel, CommittedCommand, Create, MAX_EXACT_MILLIS, Operation, Outcome, Terminal, Transition,
    reduce,
};
use wf_types::identity::RecordId;
use wf_types::version::{Fence, TimestampMillis};

#[test]
fn extend_replaces_deadline_even_when_shorter_and_retains_token_fence() -> TestResult {
    // flow_claim_native_plan.ex:459-496 uses now+lease_ms, not max(old, new).
    let mut scenario = Scenario::claimed(b"extend", 1_000)?;
    let guard = scenario.guard()?;
    assert_eq!(
        scenario.apply(
            1_100,
            Operation::ExtendLease(ExtendLease {
                guard: guard.clone(),
                lease_ms: 100,
            })
        )?,
        Outcome::Changed
    );
    assert_eq!(scenario.record.version(), 3);
    assert_eq!(scenario.record.fence(), guard.fence);
    assert_eq!(
        scenario.record.lease().ok_or("lease lost")?.token,
        guard.token
    );
    assert_eq!(scenario.record.lease_deadline().get(), 1_200);
    assert_eq!(
        scenario.record.next_run_at(),
        Some(TimestampMillis::new(1_200))
    );
    assert_eq!(scenario.record.run_state(), Some(b"queued".as_slice()));
    assert_eq!(scenario.count(EventKind::LeaseExtended), 1);
    Ok(())
}

#[test]
fn reclaim_boundary_mints_new_fence_and_rejects_all_old_owner_effects() -> TestResult {
    // ClaimDueAPI.reclaim + flow_retention_values.ex:551-561, 668-674.
    let mut scenario = Scenario::claimed(b"reclaim", 2_000)?;
    let old = scenario.guard()?;
    let reclaim = Operation::Reclaim(Claim {
        expected_state: b"running".to_vec(),
        worker: b"new-worker".to_vec(),
        token: LeaseToken::new(b"new-opaque-token".to_vec())?,
        lease_ms: 10_000,
    });
    assert_eq!(
        scenario.apply(31_999, reclaim.clone())?,
        Outcome::NotEligible
    );
    assert_eq!(scenario.apply(32_000, reclaim)?, Outcome::Changed);
    assert_eq!(
        (
            scenario.record.version(),
            scenario.record.fence().get(),
            scenario.record.attempts()
        ),
        (3, 2, 0)
    );
    assert_eq!(scenario.record.run_state(), Some(b"queued".as_slice()));
    assert_eq!(scenario.record.lease_deadline().get(), 42_000);
    assert_eq!(scenario.count(EventKind::Claimed), 2);
    let operations = vec![
        Operation::Complete(Terminal {
            guard: old.clone(),
            payload: None,
            value: None,
        }),
        Operation::Fail(Terminal {
            guard: old.clone(),
            payload: None,
            value: None,
        }),
        Operation::Retry(Retry {
            guard: old.clone(),
            run_at: RetryTime::Explicit(TimestampMillis::new(33_000)),
            payload: None,
            error: None,
        }),
        Operation::ExtendLease(ExtendLease {
            guard: old.clone(),
            lease_ms: 100,
        }),
        Operation::Transition(Transition {
            from: b"running".to_vec(),
            to: b"waiting".to_vec(),
            fence: old.fence,
            lease_token: Some(old.token.clone()),
            run_at: None,
            payload: None,
        }),
        Operation::Cancel(Cancel {
            fence: old.fence,
            lease_token: Some(old.token),
            expected_state: None,
            expected_version: None,
            error: None,
        }),
    ];
    let before = scenario.record.clone();
    for operation in operations {
        let error = reduce(Some(&scenario.record), &scenario.command(32_001, operation))
            .err()
            .ok_or("old owner survived reclaim")?;
        assert_eq!(error.to_string(), "ERR stale flow lease");
        assert_eq!(scenario.record, before);
    }
    Ok(())
}

#[test]
fn cancel_enforces_guard_order_and_repeated_cancel_is_terminal_rejection() -> TestResult {
    // flow_transition.ex:1474-1515: version, state, fence, terminal, lease.
    let mut scenario = Scenario::claimed(b"cancel", 3_000)?;
    let guard = scenario.guard()?;
    let cancel = Cancel {
        fence: guard.fence,
        lease_token: Some(guard.token.clone()),
        expected_state: Some(b"running".to_vec()),
        expected_version: Some(2),
        error: Some(b"cancelled by caller".to_vec()),
    };
    let mut invalid = cancel.clone();
    invalid.expected_version = Some(1);
    let error = reduce(
        Some(&scenario.record),
        &scenario.command(3_100, Operation::Cancel(invalid)),
    )
    .err()
    .ok_or("stale version accepted")?;
    assert_eq!(error.to_string(), "ERR flow changed concurrently");
    let mut invalid = cancel.clone();
    invalid.expected_state = Some(b"queued".to_vec());
    let error = reduce(
        Some(&scenario.record),
        &scenario.command(3_100, Operation::Cancel(invalid)),
    )
    .err()
    .ok_or("wrong state accepted")?;
    assert_eq!(error.to_string(), "ERR flow wrong state");
    let mut invalid = cancel.clone();
    invalid.fence = Fence::new(0);
    let error = reduce(
        Some(&scenario.record),
        &scenario.command(3_100, Operation::Cancel(invalid)),
    )
    .err()
    .ok_or("wrong fence accepted")?;
    assert_eq!(error.to_string(), "ERR stale flow lease");
    let mut invalid = cancel.clone();
    invalid.lease_token = None;
    let error = reduce(
        Some(&scenario.record),
        &scenario.command(3_100, Operation::Cancel(invalid)),
    )
    .err()
    .ok_or("missing lease accepted")?;
    assert_eq!(error.to_string(), "ERR stale flow lease");
    assert_eq!(
        scenario.apply(3_100, Operation::Cancel(cancel))?,
        Outcome::Changed
    );
    assert_eq!(scenario.record.state(), b"cancelled");
    assert_eq!(scenario.record.lease(), None);
    assert_eq!(scenario.record.next_run_at(), None);
    assert_eq!(scenario.record.lease_deadline().get(), 0);
    assert_eq!(
        scenario.record.error(),
        Some(b"cancelled by caller".as_slice())
    );
    let repeat = Cancel {
        fence: guard.fence,
        lease_token: None,
        expected_state: None,
        expected_version: None,
        error: None,
    };
    let before = scenario.record.clone();
    let error = reduce(
        Some(&scenario.record),
        &scenario.command(3_200, Operation::Cancel(repeat)),
    )
    .err()
    .ok_or("cancel incorrectly deduplicated")?;
    assert_eq!(error.to_string(), "ERR flow is terminal; use FLOW.REWIND");
    assert_eq!(scenario.count(EventKind::Cancelled), 1);
    assert_eq!(scenario.record, before);
    Ok(())
}

#[test]
fn expiry_without_reclaim_does_not_revoke_source_lease() -> TestResult {
    // flow_history_writes.ex:446-459 compares identity, not now versus deadline.
    let mut complete = Scenario::claimed(b"late-complete", 4_000)?;
    assert_eq!(
        complete.apply(
            34_001,
            Operation::Complete(Terminal {
                guard: complete.guard()?,
                payload: None,
                value: Some(b"done".to_vec()),
            })
        )?,
        Outcome::Changed
    );
    let mut extend = Scenario::claimed(b"late-extend", 4_000)?;
    assert_eq!(
        extend.apply(
            34_001,
            Operation::ExtendLease(ExtendLease {
                guard: extend.guard()?,
                lease_ms: 100,
            })
        )?,
        Outcome::Changed
    );
    assert_eq!(extend.record.lease_deadline().get(), 34_101);
    Ok(())
}

#[test]
fn active_record_rejects_foreign_token_and_stale_fence() -> TestResult {
    let scenario = Scenario::claimed(b"foreign-owner", 5_000)?;
    let actual = scenario.guard()?;
    let guards = [
        LeaseGuard {
            token: LeaseToken::new(b"foreign-token".to_vec())?,
            fence: actual.fence,
        },
        LeaseGuard {
            token: actual.token,
            fence: Fence::new(0),
        },
    ];
    for guard in guards {
        let operations = [
            Operation::Complete(Terminal {
                guard: guard.clone(),
                payload: None,
                value: None,
            }),
            Operation::Fail(Terminal {
                guard: guard.clone(),
                payload: None,
                value: None,
            }),
            Operation::Retry(Retry {
                guard: guard.clone(),
                run_at: RetryTime::Explicit(TimestampMillis::new(6_000)),
                payload: None,
                error: None,
            }),
            Operation::ExtendLease(ExtendLease {
                guard,
                lease_ms: 100,
            }),
        ];
        for operation in operations {
            let error = reduce(Some(&scenario.record), &scenario.command(5_100, operation))
                .err()
                .ok_or("foreign guard accepted")?;
            assert_eq!(error.to_string(), "ERR stale flow lease");
        }
    }
    Ok(())
}

#[test]
fn every_represented_fence_guard_is_source_admitted_before_record_lookup() -> TestResult {
    // Types peer finding T-D1; Options.required_non_neg_integer/2 source ceiling.
    let scenario = Scenario::claimed(b"fence-input-bound", 5_500)?;
    let actual = scenario.guard()?;
    for (fence, expected) in [
        (Fence::new(MAX_EXACT_MILLIS), "ERR stale flow lease"),
        (
            Fence::new(MAX_EXACT_MILLIS + 1),
            "ERR flow fencing_token exceeds maximum 9007199254740991",
        ),
        (
            Fence::new(u64::MAX),
            "ERR flow fencing_token exceeds maximum 9007199254740991",
        ),
    ] {
        let guard = LeaseGuard {
            token: actual.token.clone(),
            fence,
        };
        let operations = [
            Operation::Complete(Terminal {
                guard: guard.clone(),
                payload: None,
                value: None,
            }),
            Operation::Fail(Terminal {
                guard: guard.clone(),
                payload: None,
                value: None,
            }),
            Operation::Retry(Retry {
                guard: guard.clone(),
                run_at: RetryTime::Explicit(TimestampMillis::new(6_000)),
                payload: None,
                error: None,
            }),
            Operation::ExtendLease(ExtendLease {
                guard,
                lease_ms: 100,
            }),
            Operation::Transition(Transition {
                from: b"running".to_vec(),
                to: b"waiting".to_vec(),
                fence,
                lease_token: Some(actual.token.clone()),
                run_at: None,
                payload: None,
            }),
            Operation::Cancel(Cancel {
                fence,
                lease_token: Some(actual.token.clone()),
                expected_state: None,
                expected_version: None,
                error: None,
            }),
        ];
        for operation in operations {
            let command = scenario.command(5_600, operation);
            let before = scenario.record.clone();
            let error = reduce(Some(&scenario.record), &command)
                .err()
                .ok_or("invalid guard accepted")?;
            assert_eq!(error.to_string(), expected);
            assert_eq!(scenario.record, before);
            if fence.get() > MAX_EXACT_MILLIS {
                let missing = reduce(None, &command)
                    .err()
                    .ok_or("invalid guard accepted for missing record")?;
                assert_eq!(missing.to_string(), expected);
            }
        }
    }
    Ok(())
}

#[test]
fn deadline_ceiling_rejects_arithmetic_before_any_write() -> TestResult {
    let scenario = Scenario::claimed(b"overflow-deadline", 10_000)?;
    let guard = scenario.guard()?;
    let before = scenario.record.clone();
    let cases = [
        (
            MAX_EXACT_MILLIS,
            1,
            "ERR flow lease_ms deadline exceeds maximum 9007199254740991",
        ),
        (10_000, 0, "ERR flow lease_ms must be a positive integer"),
        (
            10_000,
            u64::MAX,
            "ERR flow lease_ms exceeds maximum 9007199254740991",
        ),
    ];
    for (now, lease_ms, message) in cases {
        let command = scenario.command(
            now,
            Operation::ExtendLease(ExtendLease {
                guard: guard.clone(),
                lease_ms,
            }),
        );
        let error = reduce(Some(&scenario.record), &command)
            .err()
            .ok_or("unsafe deadline accepted")?;
        assert_eq!(error.to_string(), message);
        assert_eq!(scenario.record, before);
    }
    Ok(())
}

#[test]
fn missing_and_mismatched_identity_reject_without_mutating_command() -> TestResult {
    let scenario = Scenario::claimed(b"identity", 11_000)?;
    let mut command = scenario.command(
        11_100,
        Operation::Complete(Terminal {
            guard: scenario.guard()?,
            payload: None,
            value: None,
        }),
    );
    let missing = reduce(None, &command)
        .err()
        .ok_or("missing record accepted")?;
    assert_eq!(missing.to_string(), "ERR flow not found");
    command.id = RecordId::new(b"other-id".to_vec())?;
    let before = command.clone();
    let mismatch = reduce(Some(&scenario.record), &command)
        .err()
        .ok_or("wrong record selected")?;
    assert_eq!(mismatch.to_string(), "ERR flow not found");
    assert_eq!(command, before);
    Ok(())
}

#[test]
fn create_and_transition_revalidate_source_errors_and_binary_fields() -> TestResult {
    let mut create = CommittedCommand {
        id: RecordId::new(vec![0, 255, b':'])?,
        now: TimestampMillis::new(0),
        operation: Operation::Create(Create {
            workflow_type: vec![255, 0],
            state: b"queued".to_vec(),
            run_at: None,
            payload: None,
        }),
    };
    let plan = reduce(None, &create)?;
    let record = &plan.write().ok_or("binary create missing")?.record;
    assert_eq!(record.workflow_type(), &[255, 0]);
    assert_eq!(record.next_run_at(), Some(TimestampMillis::new(0)));
    create.now = TimestampMillis::new(u64::MAX);
    assert_eq!(
        reduce(None, &create)
            .err()
            .ok_or("large time accepted")?
            .to_string(),
        "ERR flow now_ms exceeds maximum 9007199254740991"
    );
    let transition = Transition {
        from: b"queued".to_vec(),
        to: b"running".to_vec(),
        fence: Fence::new(0),
        lease_token: None,
        run_at: None,
        payload: None,
    };
    let mut command = CommittedCommand {
        id: record.id().clone(),
        now: TimestampMillis::new(0),
        operation: Operation::Transition(transition.clone()),
    };
    assert_eq!(
        reduce(Some(record), &command)
            .err()
            .ok_or("running transition accepted")?
            .to_string(),
        "ERR flow running state is only entered by FLOW.CLAIM_DUE"
    );
    command.operation = Operation::Transition(Transition {
        to: b"completed".to_vec(),
        ..transition.clone()
    });
    assert_eq!(
        reduce(Some(record), &command)
            .err()
            .ok_or("terminal transition accepted")?
            .to_string(),
        "ERR terminal flow state requires FLOW.COMPLETE, FLOW.FAIL, or FLOW.CANCEL"
    );
    command.operation = Operation::Transition(Transition {
        from: b"waiting".to_vec(),
        to: b"next".to_vec(),
        ..transition
    });
    assert_eq!(
        reduce(Some(record), &command)
            .err()
            .ok_or("wrong expected state accepted")?
            .to_string(),
        "ERR flow wrong state"
    );
    Ok(())
}

#[test]
fn source_reclaim_takeover_fences_old_worker_and_preserves_three_history_rows() -> TestResult {
    let id = RecordId::new(b"flow-reclaim-fence:1".to_vec())?;
    let create = CommittedCommand {
        id: id.clone(),
        now: TimestampMillis::new(1_000),
        operation: Operation::Create(Create {
            workflow_type: b"lease-reclaim-fence:1".to_vec(),
            state: b"queued".to_vec(),
            run_at: Some(TimestampMillis::new(1_000)),
            payload: None,
        }),
    };
    let (outcome, write, event) = reduce(None, &create)?.into_parts();
    assert_eq!(outcome, Outcome::Changed);
    let mut scenario = Scenario {
        record: write.ok_or("source create record absent")?.record,
        history: vec![event.ok_or("source create history absent")?],
    };
    assert_eq!(
        scenario.apply(
            1_000,
            Operation::Claim(Claim {
                expected_state: b"queued".to_vec(),
                worker: b"worker-a".to_vec(),
                token: LeaseToken::new(b"first-committed-token".to_vec())?,
                lease_ms: 50,
            })
        )?,
        Outcome::Changed
    );
    let first = scenario.guard()?;
    assert_eq!(scenario.record.fence().get(), 1);
    assert_eq!(
        scenario.apply(
            1_050,
            Operation::Reclaim(Claim {
                expected_state: b"running".to_vec(),
                worker: b"worker-b".to_vec(),
                token: LeaseToken::new(b"second-committed-token".to_vec())?,
                lease_ms: 500,
            })
        )?,
        Outcome::Changed
    );
    assert_eq!(scenario.record.id(), &id);
    assert_eq!(scenario.record.version(), 3);
    assert_eq!(scenario.record.lease_deadline().get(), 1_550);
    assert_eq!(scenario.record.updated_at().get(), 1_050);
    assert_eq!(scenario.record.attempts(), 0);
    assert_eq!(
        scenario
            .record
            .lease()
            .ok_or("takeover lease absent")?
            .worker,
        b"worker-b"
    );
    assert_eq!(scenario.record.fence().get(), first.fence.get() + 1);
    assert_ne!(scenario.guard()?.token, first.token);
    let before = scenario.record.clone();
    let history_before = scenario.history.clone();
    for operation in [
        Operation::Complete(Terminal {
            guard: first.clone(),
            payload: None,
            value: None,
        }),
        Operation::ExtendLease(ExtendLease {
            guard: first,
            lease_ms: 500,
        }),
    ] {
        let command = scenario.command(1_060, operation);
        assert_eq!(
            reduce(Some(&scenario.record), &command)
                .err()
                .ok_or("old worker accepted")?
                .to_string(),
            "ERR stale flow lease"
        );
        assert_eq!(scenario.record, before);
        assert_eq!(scenario.history, history_before);
    }
    assert_eq!(scenario.history.len(), 3);
    assert_eq!(scenario.count(EventKind::Created), 1);
    assert_eq!(scenario.count(EventKind::Claimed), 2);
    assert_eq!(
        scenario
            .history
            .iter()
            .map(|entry| (entry.kind, entry.worker.as_deref()))
            .collect::<Vec<_>>(),
        [
            (EventKind::Created, None),
            (EventKind::Claimed, Some(b"worker-a".as_slice())),
            (EventKind::Claimed, Some(b"worker-b".as_slice())),
        ]
    );
    Ok(())
}

mod claimed_support;
