//! All six pinned `FencingRetryTest` scenarios, with no manufactured request ID.
//!
//! The helper replaces partition selection/storage with the explicit single
//! record reducer. History counts describe emitted events, not durable storage.

#![forbid(unsafe_code)]

mod support;

use support::{Scenario, TestResult};
use wf_domain::history::EventKind;
use wf_domain::retry::{Retry, RetryTime};
use wf_domain::workflow::{Operation, Outcome, Terminal, Transition, reduce};
use wf_types::version::TimestampMillis;

#[test]
fn complete_retry_after_committed_response_loss_is_noop_success() -> TestResult {
    // Source declaration 8ad5a4d7b9e59aa1, fencing_retry_test.exs:33-51.
    let mut scenario = Scenario::claimed(b"fence-complete", 1_000)?;
    let operation = Operation::Complete(Terminal {
        guard: scenario.guard()?,
        payload: None,
        value: Some(b"first-result".to_vec()),
    });
    assert_eq!(scenario.apply(1_100, operation.clone())?, Outcome::Changed);
    let committed = scenario.record.clone();
    assert_eq!(scenario.apply(1_200, operation)?, Outcome::LegacyNoop);
    assert_eq!(scenario.count(EventKind::Completed), 1);
    assert_eq!(scenario.record, committed);
    assert_eq!(scenario.record.result(), Some(b"first-result".as_slice()));
    assert_eq!(scenario.record.updated_at().get(), 1_100);
    Ok(())
}

#[test]
fn fail_retry_after_committed_response_loss_is_noop_success() -> TestResult {
    // Source declaration 7dee0331572ef4c4, fencing_retry_test.exs:53-68.
    let mut scenario = Scenario::claimed(b"fence-fail", 2_000)?;
    let operation = Operation::Fail(Terminal {
        guard: scenario.guard()?,
        payload: None,
        value: Some(b"first-error".to_vec()),
    });
    assert_eq!(scenario.apply(2_100, operation.clone())?, Outcome::Changed);
    let committed = scenario.record.clone();
    assert_eq!(scenario.apply(2_200, operation)?, Outcome::LegacyNoop);
    assert_eq!(scenario.count(EventKind::Failed), 1);
    assert_eq!(scenario.record, committed);
    assert_eq!(scenario.record.error(), Some(b"first-error".as_slice()));
    Ok(())
}

#[test]
fn transition_retry_after_committed_response_loss_is_noop_success() -> TestResult {
    // Source declaration 57bb245681edfa80, fencing_retry_test.exs:70-94.
    let mut scenario = Scenario::claimed(b"fence-transition", 3_000)?;
    let guard = scenario.guard()?;
    let operation = Operation::Transition(Transition {
        from: b"running".to_vec(),
        to: b"waiting".to_vec(),
        fence: guard.fence,
        lease_token: Some(guard.token),
        payload: Some(b"next-payload".to_vec()),
        run_at: None,
    });
    assert_eq!(scenario.apply(3_100, operation.clone())?, Outcome::Changed);
    let committed = scenario.record.clone();
    assert_eq!(scenario.apply(3_200, operation)?, Outcome::LegacyNoop);
    assert_eq!(scenario.count(EventKind::Transitioned), 1);
    assert_eq!(scenario.record, committed);
    assert_eq!(scenario.record.payload(), Some(b"next-payload".as_slice()));
    assert_eq!(
        scenario.record.next_run_at(),
        Some(TimestampMillis::new(3_100))
    );
    Ok(())
}

#[test]
fn retry_retry_is_guarded_by_latest_history_event() -> TestResult {
    // Source declaration 093dafce3f08d6c3, fencing_retry_test.exs:96-133.
    let mut scenario = Scenario::claimed(b"fence-retry", 4_000)?;
    let guard = scenario.guard()?;
    let operation = Operation::Retry(Retry {
        guard: guard.clone(),
        payload: None,
        error: Some(b"transient-error".to_vec()),
        run_at: RetryTime::Explicit(TimestampMillis::new(5_000)),
    });
    assert_eq!(scenario.apply(4_100, operation.clone())?, Outcome::Changed);
    let committed = scenario.record.clone();
    assert_eq!(
        scenario.apply(4_200, operation.clone())?,
        Outcome::LegacyNoop
    );
    assert_eq!(scenario.count(EventKind::Retry), 1);
    assert_eq!(scenario.record, committed);
    assert_eq!(scenario.record.attempts(), 1);
    assert_eq!(
        scenario.apply(
            4_300,
            Operation::Transition(Transition {
                from: b"queued".to_vec(),
                to: b"waiting".to_vec(),
                fence: guard.fence,
                lease_token: None,
                payload: None,
                run_at: None,
            })
        )?,
        Outcome::Changed
    );
    let before = scenario.record.clone();
    let error = reduce(Some(&scenario.record), &scenario.command(4_400, operation))
        .err()
        .ok_or("old retry accepted after intervening transition")?;
    assert_eq!(error.to_string(), "ERR stale flow lease");
    assert_eq!(scenario.record, before);
    assert_eq!(scenario.count(EventKind::Retry), 1);
    Ok(())
}

#[test]
fn retry_retry_without_explicit_run_at_preserves_original_policy_schedule() -> TestResult {
    // Source declaration 21028e29c3421d3a, fencing_retry_test.exs:135-157.
    // Times below are caller-resolved committed inputs, not a policy port.
    let mut scenario = Scenario::claimed(b"fence-policy-retry", 5_000)?;
    let guard = scenario.guard()?;
    assert_eq!(
        scenario.apply(
            5_100,
            Operation::Retry(Retry {
                guard: guard.clone(),
                payload: None,
                error: Some(b"transient-error".to_vec()),
                run_at: RetryTime::PolicyResolved(TimestampMillis::new(6_100)),
            })
        )?,
        Outcome::Changed
    );
    let committed = scenario.record.clone();
    assert_eq!(
        scenario.apply(
            5_200,
            Operation::Retry(Retry {
                guard,
                payload: None,
                error: Some(b"transient-error".to_vec()),
                run_at: RetryTime::PolicyResolved(TimestampMillis::new(6_200)),
            })
        )?,
        Outcome::LegacyNoop
    );
    assert_eq!(scenario.count(EventKind::Retry), 1);
    assert_eq!(scenario.record, committed);
    assert_eq!(
        scenario.record.next_run_at(),
        Some(TimestampMillis::new(6_100))
    );
    assert_eq!(scenario.record.attempts(), 1);
    Ok(())
}

#[test]
fn same_fence_does_not_allow_different_terminal_effect() -> TestResult {
    // Source declaration eea318ca431453d0, fencing_retry_test.exs:159-179.
    let mut scenario = Scenario::claimed(b"fence-different-terminal", 6_000)?;
    let guard = scenario.guard()?;
    assert_eq!(
        scenario.apply(
            6_100,
            Operation::Fail(Terminal {
                guard: guard.clone(),
                payload: None,
                value: Some(b"failed".to_vec()),
            })
        )?,
        Outcome::Changed
    );
    let before = scenario.record.clone();
    let error = reduce(
        Some(&scenario.record),
        &scenario.command(
            6_200,
            Operation::Complete(Terminal {
                guard,
                payload: None,
                value: Some(b"done".to_vec()),
            }),
        ),
    )
    .err()
    .ok_or("different terminal effect incorrectly accepted")?;
    assert_eq!(error.to_string(), "ERR stale flow lease");
    assert_eq!(scenario.record, before);
    assert_eq!(scenario.count(EventKind::Failed), 1);
    assert_eq!(scenario.count(EventKind::Completed), 0);
    Ok(())
}
