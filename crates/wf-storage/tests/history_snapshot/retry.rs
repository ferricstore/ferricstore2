use super::{fixtures as f, *};
use std::collections::BTreeMap;
use wf_domain::policy::{Backoff, BackoffKind, InstalledRetryPolicy, RetryOverride};
use wf_domain::retry::{JitterSample, PolicyRetry, RetryDecision};
use wf_domain::workflow::reduce_with_policy;

#[test]
fn snapshot_preserves_retry_policy_and_optional_values() -> TestResult {
    let step = f::step(Some(b"\0\xff"))?;
    f::encoded(&step)?;
    let record = &step.postimage.record;
    let lease = record.lease().ok_or("fixture lease")?;
    let policy = InstalledRetryPolicy::new(
        b"email".to_vec(),
        RetryOverride {
            max_retries: Some(3),
            backoff: Some(Backoff {
                kind: BackoffKind::Exponential,
                base_ms: 10,
                max_ms: 100,
                jitter_pct: 0,
            }),
            exhausted_to: Some(b"failed".to_vec()),
        },
        BTreeMap::new(),
    )?;
    let command = CommittedCommand {
        id: record.id().clone(),
        now: TimestampMillis::new(3),
        operation: Operation::RetryWithPolicy(PolicyRetry {
            guard: LeaseGuard {
                token: lease.token.clone(),
                fence: record.fence(),
            },
            run_at: None,
            policy_override: RetryOverride::default(),
            jitter: JitterSample::new(0),
            payload: Some(Vec::new()),
            error: Some(b"retry".to_vec()),
        }),
    };
    let (_, Some(write), Some(event)) =
        reduce_with_policy(Some(record), &command, Some(&policy))?.into_parts()
    else {
        return Err("actual policy Retry".into());
    };
    assert_eq!(
        event.retry.as_ref().ok_or("retry audit")?.decision,
        RetryDecision::Scheduled
    );
    let snapshot = StoredWorkflowHistorySnapshot {
        event,
        postimage: StoredWorkflowRecord {
            record: write.record,
            ..step.postimage.clone()
        },
    };
    let bytes = f::encoded(&snapshot)?;
    assert_eq!(f::decoded(&bytes, f::limits()?)?, snapshot);
    assert_eq!(snapshot.postimage.record.payload(), Some(b"".as_slice()));
    assert_eq!(snapshot.postimage.record.error(), Some(b"retry".as_slice()));
    for payload in [None, Some(b"".as_slice()), Some(b"new".as_slice())] {
        let snapshot = f::step(payload)?;
        let bytes = f::encoded(&snapshot)?;
        assert_eq!(f::decoded(&bytes, f::limits()?)?, snapshot);
    }
    Ok(())
}
