//! Source legacy duplicate guards and logical retry state regressions.

#![forbid(unsafe_code)]

mod support;

use support::{Scenario, TestResult};
use wf_domain::claim::Claim;
use wf_domain::history::EventKind;
use wf_domain::lease::{LeaseGuard, LeaseToken};
use wf_domain::retry::{Retry, RetryTime};
use wf_domain::workflow::{Operation, Outcome, Terminal, Transition};
use wf_types::version::TimestampMillis;

#[test]
fn source_duplicates_ignore_changed_value_and_old_lease_but_preserve_first_data() -> TestResult {
    // flow_history_writes.ex:461-472 does not match an intent digest or old lease.
    let mut scenario = Scenario::claimed(b"duplicate-value", 6_000)?;
    let guard = scenario.guard()?;
    scenario.apply(
        6_100,
        Operation::Complete(Terminal {
            guard: guard.clone(),
            payload: None,
            value: Some(b"first".to_vec()),
        }),
    )?;
    let original = scenario.record.clone();
    let changed = Terminal {
        guard: LeaseGuard {
            token: LeaseToken::new(b"different-old-token".to_vec())?,
            fence: guard.fence,
        },
        payload: Some(b"replacement".to_vec()),
        value: Some(b"second".to_vec()),
    };
    assert_eq!(
        scenario.apply(6_200, Operation::Complete(changed))?,
        Outcome::LegacyNoop
    );
    assert_eq!(scenario.record, original);
    assert_eq!(scenario.count(EventKind::Completed), 1);
    Ok(())
}

#[test]
fn retry_schedule_shortcut_can_accept_matching_deadline_after_intervening_transition() -> TestResult
{
    // flow_history_writes.ex:474-509 explicitly ORs schedule and history guards.
    let mut scenario = Scenario::claimed(b"schedule-shortcut", 7_000)?;
    let guard = scenario.guard()?;
    let retry = Operation::Retry(Retry {
        guard: guard.clone(),
        run_at: RetryTime::Explicit(TimestampMillis::new(8_000)),
        payload: None,
        error: None,
    });
    scenario.apply(7_100, retry.clone())?;
    scenario.apply(
        7_200,
        Operation::Transition(Transition {
            from: b"queued".to_vec(),
            to: b"waiting".to_vec(),
            fence: guard.fence,
            lease_token: None,
            run_at: Some(TimestampMillis::new(8_000)),
            payload: None,
        }),
    )?;
    let original = scenario.record.clone();
    assert_eq!(scenario.apply(7_300, retry)?, Outcome::LegacyNoop);
    assert_eq!(scenario.record, original);
    assert_eq!(scenario.count(EventKind::Retry), 1);
    Ok(())
}

#[test]
fn latest_retry_witness_preserves_outcome_with_different_explicit_deadline() -> TestResult {
    // The source latest-history branch permits this even when schedule differs.
    let mut scenario = Scenario::claimed(b"history-branch", 8_000)?;
    let guard = scenario.guard()?;
    scenario.apply(
        8_100,
        Operation::Retry(Retry {
            guard: guard.clone(),
            run_at: RetryTime::Explicit(TimestampMillis::new(9_000)),
            payload: None,
            error: Some(b"first".to_vec()),
        }),
    )?;
    let original = scenario.record.clone();
    assert_eq!(
        scenario.apply(
            8_200,
            Operation::Retry(Retry {
                guard,
                run_at: RetryTime::Explicit(TimestampMillis::new(10_000)),
                payload: Some(b"changed".to_vec()),
                error: Some(b"changed".to_vec())
            })
        )?,
        Outcome::LegacyNoop
    );
    assert_eq!(scenario.record, original);
    assert_eq!(scenario.count(EventKind::Retry), 1);
    Ok(())
}

#[test]
fn policy_duplicate_never_validates_an_unused_recomputed_deadline() -> TestResult {
    // Source checks duplicate before policy resolution in flow_transition.ex:1075.
    let mut scenario = Scenario::claimed(b"unused-policy-time", 8_500)?;
    let guard = scenario.guard()?;
    scenario.apply(
        8_600,
        Operation::Retry(Retry {
            guard: guard.clone(),
            run_at: RetryTime::PolicyResolved(TimestampMillis::new(9_600)),
            payload: None,
            error: None,
        }),
    )?;
    let original = scenario.record.clone();
    assert_eq!(
        scenario.apply(
            8_700,
            Operation::Retry(Retry {
                guard,
                run_at: RetryTime::PolicyResolved(TimestampMillis::new(u64::MAX)),
                payload: None,
                error: None,
            })
        )?,
        Outcome::LegacyNoop
    );
    assert_eq!(scenario.record, original);
    assert_eq!(scenario.count(EventKind::Retry), 1);
    Ok(())
}

#[test]
fn retry_returns_to_claimed_logical_state_and_keeps_zero_deadline() -> TestResult {
    let mut scenario = Scenario::claimed(b"logical-state", 9_000)?;
    let old = scenario.guard()?;
    scenario.apply(
        9_100,
        Operation::Transition(Transition {
            from: b"running".to_vec(),
            to: b"waiting".to_vec(),
            fence: old.fence,
            lease_token: Some(old.token),
            run_at: None,
            payload: None,
        }),
    )?;
    scenario.apply(
        9_100,
        Operation::Claim(Claim {
            expected_state: b"waiting".to_vec(),
            worker: b"worker".to_vec(),
            token: LeaseToken::new(b"waiting-token".to_vec())?,
            lease_ms: 100,
        }),
    )?;
    assert_eq!(scenario.record.run_state(), Some(b"waiting".as_slice()));
    scenario.apply(
        9_150,
        Operation::Retry(Retry {
            guard: scenario.guard()?,
            run_at: RetryTime::Explicit(TimestampMillis::new(0)),
            payload: Some(Vec::new()),
            error: None,
        }),
    )?;
    assert_eq!(scenario.record.state(), b"waiting");
    assert_eq!(scenario.record.run_state(), None);
    assert_eq!(scenario.record.next_run_at(), Some(TimestampMillis::new(0)));
    assert_eq!(scenario.record.payload(), Some(b"".as_slice()));
    assert_eq!(scenario.record.attempts(), 1);
    Ok(())
}
