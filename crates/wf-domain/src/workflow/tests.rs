//! Internal overflow fixtures exercise unreachable-through-short-runs boundaries.

use super::*;
use crate::claim::Claim;
use crate::lease::{LeaseGuard, LeaseToken};
use crate::policy::RetryOverride;
use crate::retry::{JitterSample, PolicyRetry, Retry, RetryTime};
use crate::value::Counter;

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn queued() -> Result<WorkflowRecord, Box<dyn std::error::Error>> {
    let command = CommittedCommand {
        id: RecordId::new(b"counter-boundary".to_vec())?,
        now: TimestampMillis::new(0),
        operation: Operation::Create(Create {
            workflow_type: b"type".to_vec(),
            state: b"queued".to_vec(),
            run_at: None,
            payload: None,
        }),
    };
    let (_, write, _) = reduce(None, &command)?.into_parts();
    Ok(write.ok_or("fixture create missing")?.record)
}

fn claim_command(record: &WorkflowRecord) -> Result<CommittedCommand, DomainError> {
    Ok(CommittedCommand {
        id: record.id.clone(),
        now: TimestampMillis::new(0),
        operation: Operation::Claim(Claim {
            expected_state: b"queued".to_vec(),
            worker: b"worker".to_vec(),
            token: LeaseToken::new(b"token".to_vec())?,
            lease_ms: 1,
        }),
    })
}

#[test]
fn version_overflow_rejects_without_changing_the_authoritative_input() -> TestResult {
    let mut record = queued()?;
    record.version = u64::MAX;
    let command = claim_command(&record)?;
    let before = record.clone();
    assert_eq!(
        reduce(Some(&record), &command),
        Err(DomainError::CounterOverflow(Counter::Version))
    );
    assert_eq!(record, before);
    Ok(())
}

#[test]
fn fence_overflow_rejects_instead_of_reusing_a_generation() -> TestResult {
    let mut record = queued()?;
    record.fence = Fence::new(u64::MAX);
    let command = claim_command(&record)?;
    let before = record.clone();
    assert_eq!(
        reduce(Some(&record), &command),
        Err(DomainError::CounterOverflow(Counter::Fence))
    );
    assert_eq!(record, before);
    Ok(())
}

#[test]
fn attempts_overflow_cannot_release_a_running_lease_or_append_history() -> TestResult {
    let record = queued()?;
    let (_, write, _) = reduce(Some(&record), &claim_command(&record)?)?.into_parts();
    let mut running = write.ok_or("fixture claim missing")?.record;
    running.attempts = u64::MAX;
    let guard = LeaseGuard {
        token: running
            .lease
            .as_ref()
            .ok_or("fixture lease missing")?
            .token
            .clone(),
        fence: running.fence,
    };
    let command = CommittedCommand {
        id: running.id.clone(),
        now: TimestampMillis::new(0),
        operation: Operation::Retry(Retry {
            guard,
            run_at: RetryTime::Explicit(TimestampMillis::new(0)),
            payload: None,
            error: None,
        }),
    };
    let before = running.clone();
    assert_eq!(
        reduce(Some(&running), &command),
        Err(DomainError::CounterOverflow(Counter::Attempts))
    );
    assert_eq!(running, before);
    Ok(())
}

#[test]
fn legacy_duplicates_cannot_bypass_the_source_fence_input_limit() -> TestResult {
    let mut record = queued()?;
    let fence = Fence::new(MAX_EXACT_MILLIS + 1);
    record.fence = fence;
    record.attempts = 1;
    let guard = LeaseGuard {
        token: LeaseToken::new(b"old-token".to_vec())?,
        fence,
    };
    let cases = [
        (
            b"completed".as_slice(),
            Operation::Complete(Terminal {
                guard: guard.clone(),
                payload: None,
                value: None,
            }),
        ),
        (
            b"failed".as_slice(),
            Operation::Fail(Terminal {
                guard: guard.clone(),
                payload: None,
                value: None,
            }),
        ),
        (
            b"waiting".as_slice(),
            Operation::Transition(Transition {
                from: b"running".to_vec(),
                to: b"waiting".to_vec(),
                fence,
                lease_token: Some(guard.token.clone()),
                run_at: None,
                payload: None,
            }),
        ),
        (
            b"failed".as_slice(),
            Operation::RetryWithPolicy(PolicyRetry {
                guard: guard.clone(),
                run_at: None,
                policy_override: RetryOverride::default(),
                jitter: JitterSample::new(0),
                payload: None,
                error: None,
            }),
        ),
        (
            b"queued".as_slice(),
            Operation::Retry(Retry {
                guard,
                run_at: RetryTime::PolicyResolved(TimestampMillis::new(0)),
                payload: None,
                error: None,
            }),
        ),
    ];
    for (state, operation) in cases {
        record.state = state.to_vec();
        let before = record.clone();
        let command = CommittedCommand {
            id: record.id.clone(),
            now: TimestampMillis::new(0),
            operation,
        };
        let error = reduce(Some(&record), &command)
            .err()
            .ok_or("duplicate bypassed numeric guard admission")?;
        assert_eq!(
            error.to_string(),
            "ERR flow fencing_token exceeds maximum 9007199254740991"
        );
        assert_eq!(record, before);
    }
    Ok(())
}

#[test]
fn installed_retry_overflow_and_restored_result_preservation_are_atomic() -> TestResult {
    let record = queued()?;
    let (_, write, _) = reduce(Some(&record), &claim_command(&record)?)?.into_parts();
    let mut running = write.ok_or("fixture claim missing")?.record;
    running.result = Some(b"retained-result".to_vec());
    running.error = Some(b"old-error".to_vec());
    let retry = PolicyRetry {
        guard: LeaseGuard {
            token: running
                .lease
                .as_ref()
                .ok_or("fixture lease missing")?
                .token
                .clone(),
            fence: running.fence,
        },
        run_at: Some(TimestampMillis::new(0)),
        policy_override: RetryOverride::default(),
        jitter: JitterSample::new(0),
        payload: None,
        error: None,
    };
    let command = CommittedCommand {
        id: running.id.clone(),
        now: TimestampMillis::new(0),
        operation: Operation::RetryWithPolicy(retry),
    };
    for (version, attempts, expected) in [
        (u64::MAX, 0, Counter::Version),
        (2, u64::MAX, Counter::Attempts),
    ] {
        running.version = version;
        running.attempts = attempts;
        let before = running.clone();
        assert_eq!(
            reduce(Some(&running), &command),
            Err(DomainError::CounterOverflow(expected))
        );
        assert_eq!(running, before);
    }
    running.version = 2;
    running.attempts = 0;
    let plan = reduce(Some(&running), &command)?;
    let next = &plan.write().ok_or("retry write missing")?.record;
    assert_eq!(next.result(), Some(b"retained-result".as_slice()));
    assert_eq!(next.error(), None);
    assert_eq!(next.lease(), None);
    Ok(())
}
