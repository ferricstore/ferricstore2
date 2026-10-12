//! Deterministic lifecycle oracle supplements source assertions without clocks.
#![forbid(unsafe_code)]

use wf_domain::claim::Claim;
use wf_domain::lease::{LeaseGuard, LeaseToken};
use wf_domain::retry::{Retry, RetryTime};
use wf_domain::workflow::{
    CommittedCommand, Create, DomainError, Operation, Outcome, WorkflowRecord, reduce,
};
use wf_types::identity::RecordId;
use wf_types::version::{Fence, TimestampMillis};

type TestError = Box<dyn std::error::Error>;
type TestResult = Result<(), TestError>;

fn command(record: &WorkflowRecord, now: u64, operation: Operation) -> CommittedCommand {
    CommittedCommand {
        id: record.id().clone(),
        now: TimestampMillis::new(now),
        operation,
    }
}
fn next(record: &WorkflowRecord, command: &CommittedCommand) -> Result<WorkflowRecord, TestError> {
    let plan = reduce(Some(record), command)?;
    assert_eq!(plan, reduce(Some(record), command)?);
    Ok(plan
        .into_parts()
        .1
        .ok_or("required test fixture value absent")?
        .record)
}
fn initial(seed: u32) -> Result<WorkflowRecord, TestError> {
    Ok(reduce(
        None,
        &CommittedCommand {
            id: RecordId::new(seed.to_be_bytes().to_vec())?,
            now: TimestampMillis::new(0),
            operation: Operation::Create(Create {
                workflow_type: b"model".to_vec(),
                state: b"queued".to_vec(),
                run_at: None,
                payload: Some(seed.to_be_bytes().to_vec()),
            }),
        },
    )?
    .into_parts()
    .1
    .ok_or("required test fixture value absent")?
    .record)
}

fn assert_stale(record: &WorkflowRecord, guard: &LeaseGuard, now: u64) {
    assert_eq!(
        reduce(
            Some(record),
            &command(
                record,
                now,
                Operation::Retry(Retry {
                    guard: guard.clone(),
                    run_at: RetryTime::Explicit(TimestampMillis::new(now)),
                    payload: None,
                    error: None,
                })
            )
        ),
        Err(DomainError::StaleLease)
    );
}

#[test]
fn generated_claim_reclaim_retry_sequences_match_fence_attempt_and_duplicate_model() -> TestResult {
    for seed in 0..32_u32 {
        let mut record = initial(seed)?;
        let mut version = 1;
        let mut fence = 0;
        let mut attempts = 0;
        let mut old_guard: Option<LeaseGuard> = None;
        for step in 0..32_u8 {
            let mut now = 100 + u64::from(seed) + u64::from(step) * 100;
            let token = LeaseToken::new(vec![step, 0xff])?;
            let claim = command(
                &record,
                now,
                Operation::Claim(Claim {
                    expected_state: b"queued".to_vec(),
                    worker: vec![step, 0],
                    token: token.clone(),
                    lease_ms: 10,
                }),
            );
            record = next(&record, &claim)?;
            version += 1;
            fence += 1;
            assert_eq!(record.fence().get(), fence);
            assert_eq!(record.attempts(), attempts);
            if let Some(stale) = &old_guard {
                assert_stale(&record, stale, now);
            }
            let mut guard = LeaseGuard {
                token,
                fence: Fence::new(fence),
            };
            if step % 3 == 0 {
                now += 10;
                let reclaimed_token = LeaseToken::new(vec![step, 0xfe])?;
                let reclaimed = next(
                    &record,
                    &command(
                        &record,
                        now,
                        Operation::Reclaim(Claim {
                            expected_state: b"running".to_vec(),
                            worker: b"reclaimer".to_vec(),
                            token: reclaimed_token.clone(),
                            lease_ms: 10,
                        }),
                    ),
                )?;
                assert_stale(&reclaimed, &guard, now);
                record = reclaimed;
                fence += 1;
                version += 1;
                guard = LeaseGuard {
                    token: reclaimed_token,
                    fence: Fence::new(fence),
                };
            }
            let retry = command(
                &record,
                now + 1,
                Operation::Retry(Retry {
                    guard: guard.clone(),
                    run_at: RetryTime::Explicit(TimestampMillis::new(now + 5)),
                    payload: (step % 2 == 0).then(Vec::new),
                    error: Some(vec![step]),
                }),
            );
            record = next(&record, &retry)?;
            version += 1;
            attempts += 1;
            assert_eq!(record.state(), b"queued");
            assert_eq!(record.version(), version);
            assert_eq!(record.fence().get(), fence);
            assert_eq!(record.attempts(), attempts);
            assert_eq!(record.payload(), Some([].as_slice()));
            assert_eq!(record.error(), Some([step].as_slice()));
            assert!(record.lease().is_none());
            let duplicate = reduce(Some(&record), &retry)?;
            assert_eq!(duplicate.outcome(), Outcome::LegacyNoop);
            assert!(duplicate.write().is_none() && duplicate.history().is_none());
            old_guard = Some(guard);
        }
    }
    Ok(())
}
