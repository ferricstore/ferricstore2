//! Pinned lease/lifecycle observations over actual single-record proposed writes.

#![forbid(unsafe_code)]

mod support;

use support::{Scenario, TestError, TestResult};
use wf_domain::claim::Claim;
use wf_domain::history::EventKind::{
    Cancelled, Claimed, Completed, Created, Failed, LeaseExtended,
};
use wf_domain::lease::{ExtendLease, LeaseToken};
use wf_domain::workflow::{
    Cancel, CommittedCommand, Create, Operation, Outcome, Terminal, Transition, reduce,
};
use wf_types::identity::RecordId;
use wf_types::version::{Fence, TimestampMillis};

#[test]
fn source_reclaim_boundary_preserves_claim_and_explicit_reclaim_scenarios() -> TestResult {
    // P757/P798 retain separate originals; public dispatch/selection is not implemented.
    for (id, workflow_type) in [
        (b"flow-reclaim:1".as_slice(), b"lease".as_slice()),
        (b"flow-reclaim-api:1".as_slice(), b"lease-api".as_slice()),
    ] {
        let expected_id = RecordId::new(id.to_vec())?;
        let events = [Created, Claimed, Claimed];
        let mut scenario = source_claimed(id, workflow_type, 50)?;
        let first = scenario.guard()?;
        assert_eq!(scenario.record.lease_deadline().get(), 1_050);
        let before = (scenario.record.clone(), scenario.history.clone());
        let reclaim = Operation::Reclaim(Claim {
            expected_state: b"running".to_vec(),
            worker: b"worker-b".to_vec(),
            token: LeaseToken::new([b"second-token:".as_slice(), id].concat())?,
            lease_ms: 50,
        });
        let early = reduce(
            Some(&scenario.record),
            &scenario.command(1_049, reclaim.clone()),
        )?;
        assert_eq!(early.outcome(), Outcome::NotEligible);
        assert!(early.write().is_none() && early.history().is_none());
        assert_eq!(scenario.record, before.0);
        assert_eq!(scenario.history, before.1);
        assert_eq!(scenario.apply(1_050, reclaim)?, Outcome::Changed);
        assert_eq!(scenario.record.id(), &expected_id);
        assert_eq!(scenario.record.workflow_type(), workflow_type);
        let lease = scenario.record.lease().ok_or("reclaim lost lease")?;
        assert_eq!(lease.worker, b"worker-b");
        assert_eq!(scenario.record.version(), 3);
        assert_ne!(lease.token, first.token);
        assert_eq!(scenario.record.fence().get(), 2);
        assert_eq!(scenario.record.lease_deadline().get(), 1_100);
        assert_eq!(
            scenario.record.next_run_at(),
            Some(TimestampMillis::new(1_100))
        );
        assert_eq!(scenario.record.state(), b"running");
        assert_eq!(scenario.record.run_state(), Some(b"queued".as_slice()));
        assert_eq!(scenario.record.created_at().get(), 1_000);
        assert_eq!(scenario.record.updated_at().get(), 1_050);
        assert_eq!(scenario.record.attempts(), 0);
        assert_eq!(
            (
                scenario.record.payload(),
                scenario.record.result(),
                scenario.record.error()
            ),
            (None, None, None)
        );
        let history = &scenario.history;
        assert!(history.iter().map(|event| event.kind).eq(events));
        assert_eq!(scenario.count(Created), 1);
        assert_eq!(scenario.count(Claimed), 2);
        let last = scenario.history.last().ok_or("missing reclaim history")?;
        assert_eq!(
            (
                last.version,
                last.fence.get(),
                last.occurred_at.get(),
                last.lease_deadline.get(),
                last.worker.as_deref()
            ),
            (3, 2, 1_050, 1_100, Some(b"worker-b".as_slice()))
        );
    }
    Ok(())
}

#[test]
fn source_extension_preserves_ownership_deadline_and_reclaim_ineligibility() -> TestResult {
    // P833/P877: second fixture represents scalar state, never return:ok_on_success.
    for (id, workflow_type) in [
        (
            b"flow-lease-extend:1".as_slice(),
            b"lease-extend:1".as_slice(),
        ),
        (
            b"flow-lease-extend-ok:1".as_slice(),
            b"lease-extend-ok:1".as_slice(),
        ),
    ] {
        let expected_id = RecordId::new(id.to_vec())?;
        let events = [Created, Claimed, LeaseExtended];
        let mut scenario = source_claimed(id, workflow_type, 50)?;
        let claimed = scenario.guard()?;
        assert_eq!(scenario.record.version(), 2);
        assert_eq!(
            scenario.apply(
                1_020,
                Operation::ExtendLease(ExtendLease {
                    guard: claimed.clone(),
                    lease_ms: 500
                })
            )?,
            Outcome::Changed
        );
        assert_eq!(scenario.record.id(), &expected_id);
        assert_eq!(scenario.record.workflow_type(), workflow_type);
        let record = &scenario.record;
        assert_eq!(record.state(), b"running");
        assert_eq!(record.version(), 3);
        let lease = record.lease().ok_or("extension lost lease")?;
        assert_eq!(lease.token, claimed.token);
        assert_eq!(record.fence(), claimed.fence);
        assert_eq!(record.lease_deadline().get(), 1_520);
        assert_eq!(lease.worker, b"worker-a");
        assert_eq!(record.next_run_at(), Some(TimestampMillis::new(1_520)));
        assert_eq!(record.run_state(), Some(b"queued".as_slice()));
        assert_eq!(
            (
                record.created_at().get(),
                record.updated_at().get(),
                record.attempts()
            ),
            (1_000, 1_020, 0)
        );
        let before = (record.clone(), scenario.history.clone());
        let early = reduce(
            Some(&scenario.record),
            &scenario.command(
                1_100,
                Operation::Reclaim(Claim {
                    expected_state: b"running".to_vec(),
                    worker: b"worker-b".to_vec(),
                    token: LeaseToken::new([b"second-token:".as_slice(), id].concat())?,
                    lease_ms: 50,
                }),
            ),
        )?;
        assert_eq!(early.outcome(), Outcome::NotEligible);
        assert!(early.write().is_none() && early.history().is_none());
        let mut wrong = claimed;
        wrong.fence = Fence::new(2);
        let error = reduce(
            Some(&scenario.record),
            &scenario.command(
                1_030,
                Operation::ExtendLease(ExtendLease {
                    guard: wrong,
                    lease_ms: 500,
                }),
            ),
        )
        .err()
        .ok_or("wrong extension fence accepted")?;
        assert_eq!(error.to_string(), "ERR stale flow lease");
        assert_eq!(scenario.record, before.0);
        assert_eq!(scenario.history, before.1);
        let history = &scenario.history;
        assert!(history.iter().map(|event| event.kind).eq(events));
        let last = scenario.history.last().ok_or("missing extension history")?;
        assert_eq!(
            (
                last.version,
                last.fence.get(),
                last.occurred_at.get(),
                last.lease_deadline.get(),
                last.worker.as_deref()
            ),
            (3, 1, 1_020, 1_520, Some(b"worker-a".as_slice()))
        );
    }
    Ok(())
}

#[test]
fn source_completed_record_preserves_transition_cancel_and_extension_errors() -> TestResult {
    // P955 and C128 retain different fixtures; omitted C command clocks are explicit here.
    for (id, workflow_type, lease_ms, result) in [
        (
            b"flow-lease-extend-terminal:1".as_slice(),
            b"lease-extend-terminal:1".as_slice(),
            50,
            None,
        ),
        (
            b"flow-terminal-guard:1".as_slice(),
            b"image".as_slice(),
            30_000,
            Some(b"result:flow-terminal-guard:1".as_slice()),
        ),
    ] {
        let expected_id = RecordId::new(id.to_vec())?;
        let events = [Created, Claimed, Completed];
        let mut scenario = source_claimed(id, workflow_type, lease_ms)?;
        let claimed = scenario.guard()?;
        assert_eq!(
            scenario.apply(
                1_010,
                Operation::Complete(Terminal {
                    guard: claimed.clone(),
                    payload: None,
                    value: result.map(<[u8]>::to_vec)
                })
            )?,
            Outcome::Changed
        );
        assert_eq!(scenario.record.id(), &expected_id);
        assert_eq!(scenario.record.workflow_type(), workflow_type);
        let record = &scenario.record;
        assert_eq!(record.state(), b"completed");
        assert_eq!((record.version(), record.fence().get()), (3, 1));
        assert_eq!(record.result(), result);
        assert_eq!((record.lease(), record.next_run_at()), (None, None));
        let before = (record.clone(), scenario.history.clone());
        for (operation, expected) in [
            (
                Operation::Transition(Transition {
                    from: b"completed".to_vec(),
                    to: b"queued".to_vec(),
                    fence: record.fence(),
                    lease_token: None,
                    payload: None,
                    run_at: None,
                }),
                "ERR flow is terminal; use FLOW.REWIND",
            ),
            (
                Operation::Cancel(Cancel {
                    fence: record.fence(),
                    lease_token: None,
                    expected_state: None,
                    expected_version: None,
                    error: None,
                }),
                "ERR flow is terminal; use FLOW.REWIND",
            ),
            (
                Operation::ExtendLease(ExtendLease {
                    guard: claimed.clone(),
                    lease_ms: 500,
                }),
                "ERR stale flow lease",
            ),
        ] {
            let error = reduce(Some(&scenario.record), &scenario.command(1_020, operation))
                .err()
                .ok_or("completed record accepted a new effect")?;
            assert_eq!(error.to_string(), expected);
            assert_eq!(scenario.record, before.0);
            assert_eq!(scenario.history, before.1);
        }
        assert_eq!(record.state(), b"completed");
        let history = &scenario.history;
        assert!(history.iter().map(|event| event.kind).eq(events));
        assert_eq!(scenario.count(Completed), 1);
        let last = scenario.history.last().ok_or("missing complete history")?;
        assert_eq!(
            (
                last.version,
                last.fence.get(),
                last.occurred_at.get(),
                last.worker.as_deref(),
                last.lease_deadline.get(),
                last.next_run_at
            ),
            (3, 1, 1_010, None, 0, None)
        );
    }
    Ok(())
}

#[test]
fn source_fail_and_unleased_cancel_remove_due_work_and_reject_stale_fail() -> TestResult {
    // F738 retains both creations before claiming only the failure record.
    let fail_id = b"flow-fail:1";
    let cancel_id = b"flow-cancel:1";
    let fail_body = b"error:flow-fail:1";
    let cancel_body = b"reason:flow-cancel:1";
    let failed_events = [Created, Claimed, Failed];
    let cancelled_events = [Created, Cancelled];
    let mut failed = source_queued(fail_id, b"jobs-fail")?;
    let mut cancelled = source_queued(cancel_id, b"jobs-cancel")?;
    assert_eq!(
        failed.apply(
            1_000,
            Operation::Claim(Claim {
                expected_state: b"queued".to_vec(),
                worker: b"worker-a".to_vec(),
                token: LeaseToken::new(b"first-token:flow-fail:1".to_vec())?,
                lease_ms: 30_000,
            })
        )?,
        Outcome::Changed
    );
    assert_eq!(failed.record.id(), &RecordId::new(fail_id.to_vec())?);
    let terminal = Terminal {
        guard: failed.guard()?,
        payload: None,
        value: Some(fail_body.to_vec()),
    };
    let mut wrong_guard = terminal.clone();
    wrong_guard.guard.fence = Fence::new(2);
    let before = (failed.record.clone(), failed.history.clone());
    let error = reduce(
        Some(&failed.record),
        &failed.command(1_500, Operation::Fail(wrong_guard)),
    )
    .err()
    .ok_or("stale failure guard accepted")?;
    assert_eq!(error.to_string(), "ERR stale flow lease");
    assert_eq!(failed.record, before.0);
    assert_eq!(failed.history, before.1);
    let queued = &cancelled.record;
    assert_eq!(
        (queued.version(), queued.fence().get(), queued.lease()),
        (1, 0, None)
    );
    let cancellation = Operation::Cancel(Cancel {
        fence: Fence::new(0),
        lease_token: None,
        expected_state: None,
        expected_version: None,
        error: Some(cancel_body.to_vec()),
    });
    let due_claim = Operation::Claim(Claim {
        expected_state: b"queued".to_vec(),
        worker: b"worker-b".to_vec(),
        token: LeaseToken::new(b"second-owner-token".to_vec())?,
        lease_ms: 30_000,
    });
    for (scenario, operation, expected, events) in [
        (
            &mut failed,
            Operation::Fail(terminal),
            (b"failed".as_slice(), fail_body.as_slice(), 3, 1),
            failed_events.as_slice(),
        ),
        (
            &mut cancelled,
            cancellation,
            (b"cancelled".as_slice(), cancel_body.as_slice(), 2, 0),
            cancelled_events.as_slice(),
        ),
    ] {
        let (state, body, version, fence) = expected;
        assert_eq!(scenario.apply(1_500, operation)?, Outcome::Changed);
        let record = &scenario.record;
        assert_eq!((record.state(), record.error()), (state, Some(body)));
        assert_eq!((record.version(), record.fence().get()), (version, fence));
        assert_eq!((record.lease(), record.next_run_at()), (None, None));
        assert_eq!(record.lease_deadline().get(), 0);
        assert_eq!(
            (record.created_at().get(), record.updated_at().get()),
            (1_000, 1_500)
        );
        let history = &scenario.history;
        assert!(history.iter().map(|e| e.kind).eq(events.iter().copied()));
        let last = scenario.history.last().ok_or("missing terminal history")?;
        let metadata = (
            last.version,
            last.fence.get(),
            last.occurred_at.get(),
            last.worker.as_deref(),
            last.next_run_at,
        );
        assert_eq!(metadata, (version, fence, 1_500, None, None));
        let plan = reduce(Some(record), &scenario.command(1_500, due_claim.clone()))?;
        assert_eq!(plan.outcome(), Outcome::NotEligible);
        assert!(plan.write().is_none() && plan.history().is_none());
    }
    Ok(())
}

fn source_queued(id: &[u8], workflow_type: &[u8]) -> Result<Scenario, TestError> {
    let command = CommittedCommand {
        id: RecordId::new(id.to_vec())?,
        now: TimestampMillis::new(1_000),
        operation: Operation::Create(Create {
            workflow_type: workflow_type.to_vec(),
            state: b"queued".to_vec(),
            payload: None,
            run_at: Some(TimestampMillis::new(1_000)),
        }),
    };
    let (outcome, write, event) = reduce(None, &command)?.into_parts();
    assert_eq!(outcome, Outcome::Changed);
    let write = write.ok_or("source creation did not emit a record")?;
    let event = event.ok_or("source creation did not emit history")?;
    assert_eq!(write.expected_version, None);
    assert_eq!(write.record.version(), event.version);
    assert_eq!(event.kind, Created);
    Ok(Scenario {
        record: write.record,
        history: vec![event],
    })
}

fn source_claimed(id: &[u8], workflow_type: &[u8], lease_ms: u64) -> Result<Scenario, TestError> {
    let mut scenario = source_queued(id, workflow_type)?;
    assert_eq!(
        scenario.apply(
            1_000,
            Operation::Claim(Claim {
                expected_state: b"queued".to_vec(),
                worker: b"worker-a".to_vec(),
                token: LeaseToken::new([b"first-token:".as_slice(), id].concat())?,
                lease_ms,
            }),
        )?,
        Outcome::Changed
    );
    Ok(scenario)
}
