//! Atomic running-step guards, payload presence and returned lease ownership.

use super::*;

mod guards;
mod indexed;

fn step_running() -> Result<WorkflowRecord, Box<dyn std::error::Error>> {
    let record = queued()?;
    let plan = reduce(Some(&record), &claim_command(&record)?)?;
    let (Outcome::Changed, Some(write), _) = plan.into_parts() else {
        return Err("step fixture claim must change one record".into());
    };
    Ok(write.record)
}

fn step_command(record: &WorkflowRecord) -> Result<CommittedCommand, DomainError> {
    let lease = record.lease().ok_or(DomainError::StaleLease)?;
    Ok(CommittedCommand {
        id: record.id().clone(),
        now: TimestampMillis::new(2),
        operation: Operation::StepContinue(StepContinue {
            guard: LeaseGuard {
                token: lease.token.clone(),
                fence: record.fence(),
            },
            from: validation::retry_run_state(record).to_vec(),
            to: b"charge".to_vec(),
            worker: None,
            lease_ms: 30_000,
            payload: None,
        }),
    })
}

#[test]
fn step_continue_rotates_running_guard_once() -> TestResult {
    let mut record = step_running()?;
    record.attempts = 7;
    record.payload = Some(b"old-payload".to_vec());
    record.result = Some(b"retained-result".to_vec());
    record.error = Some(b"retained-error".to_vec());
    let before = record.clone();
    let command = step_command(&record)?;
    let plan = reduce(Some(&record), &command)?;
    assert_eq!(plan.outcome(), Outcome::Changed);
    let write = plan.write().ok_or("step write absent")?;
    let next = &write.record;
    let lease = next.lease().ok_or("step lease absent")?;
    assert_eq!(write.expected_version, Some(2));
    assert_eq!(
        (next.state(), next.run_state()),
        (b"running".as_slice(), Some(b"charge".as_slice()))
    );
    assert_eq!(
        (next.version(), next.fence().get(), next.updated_at().get()),
        (3, 2, 2)
    );
    assert_eq!(
        (next.next_run_at(), lease.deadline.get()),
        (Some(TimestampMillis::new(30_002)), 30_002)
    );
    assert_eq!(lease.worker, b"worker");
    assert_eq!(lease.token.as_bytes(), b"worker:2:2");
    assert_ne!(lease.token, before.lease().ok_or("old lease absent")?.token);
    assert_eq!(next.id(), before.id());
    assert_eq!(next.workflow_type(), before.workflow_type());
    assert_eq!(
        (next.created_at(), next.attempts()),
        (before.created_at(), 7)
    );
    assert_eq!(
        (next.payload(), next.result(), next.error()),
        (before.payload(), before.result(), before.error())
    );
    let history = plan.history().ok_or("step history absent")?;
    assert_eq!(
        (history.kind, history.kind.as_str()),
        (EventKind::StepContinued, "step_continued")
    );
    assert_eq!(
        (
            history.version,
            history.fence.get(),
            history.occurred_at.get()
        ),
        (3, 2, 2)
    );
    assert_eq!(
        (history.state.as_slice(), history.worker.as_deref()),
        (b"running".as_slice(), Some(b"worker".as_slice()))
    );
    assert_eq!(
        next.latest_event_witness(),
        (EventKind::StepContinued, 3, Fence::new(2))
    );
    assert_eq!(record, before);
    Ok(())
}

#[test]
fn step_continue_uses_returned_guard_for_retry() -> TestResult {
    let record = step_running()?;
    let command = step_command(&record)?;
    let continued = reduce(Some(&record), &command)?
        .into_parts()
        .1
        .ok_or("step write absent")?
        .record;
    let actual = continued.lease().ok_or("returned lease absent")?;
    assert_eq!(continued.payload(), None);
    let retry = CommittedCommand {
        id: continued.id().clone(),
        now: TimestampMillis::new(3),
        operation: Operation::Retry(Retry {
            guard: LeaseGuard {
                token: actual.token.clone(),
                fence: continued.fence(),
            },
            run_at: RetryTime::Explicit(TimestampMillis::new(5)),
            payload: None,
            error: Some(b"retry-error".to_vec()),
        }),
    };
    let mut stale = retry.clone();
    let Operation::Retry(old) = &mut stale.operation else {
        return Err("retry fixture absent".into());
    };
    old.guard = LeaseGuard {
        token: record.lease().ok_or("old lease absent")?.token.clone(),
        fence: record.fence(),
    };
    let before = continued.clone();
    assert_eq!(
        reduce(Some(&continued), &stale),
        Err(DomainError::StaleLease)
    );
    assert_eq!(
        reduce(Some(&continued), &command),
        Err(DomainError::StaleLease)
    );
    let plan = reduce(Some(&continued), &retry)?;
    assert_eq!(plan.outcome(), Outcome::Changed);
    let next = &plan.write().ok_or("retry write absent")?.record;
    assert_eq!(
        (next.state(), next.run_state()),
        (b"charge".as_slice(), None)
    );
    assert_eq!(
        (next.lease(), next.next_run_at()),
        (None, Some(TimestampMillis::new(5)))
    );
    assert_eq!(
        (next.version(), next.fence(), next.attempts()),
        (4, continued.fence(), 1)
    );
    assert_eq!(next.error(), Some(b"retry-error".as_slice()));
    assert_eq!(
        plan.history().ok_or("retry history absent")?.kind,
        EventKind::Retry
    );
    assert_eq!(continued, before);
    Ok(())
}

#[test]
fn step_continue_preserves_binary_owner_and_payload_presence() -> TestResult {
    let mut record = step_running()?;
    let owner = vec![0xff, 0, b':', b'x'];
    record
        .lease
        .as_mut()
        .ok_or("fixture lease absent")?
        .worker
        .clone_from(&owner);
    record.payload = Some(b"old".to_vec());
    record.result = Some(b"result".to_vec());
    record.error = Some(b"error".to_vec());
    let before = record.clone();
    for payload in [None, Some(Vec::new()), Some(vec![0xff, 0])] {
        for worker in [None, Some(owner.clone())] {
            let mut command = step_command(&record)?;
            let Operation::StepContinue(step) = &mut command.operation else {
                return Err("step fixture absent".into());
            };
            step.worker = worker;
            step.payload.clone_from(&payload);
            let plan = reduce(Some(&record), &command)?;
            let next = &plan.write().ok_or("binary step write absent")?.record;
            let lease = next.lease().ok_or("binary step lease absent")?;
            assert_eq!(lease.worker, owner);
            assert_eq!(
                lease.token.as_bytes(),
                &[0xff, 0, b':', b'x', b':', b'2', b':', b'2']
            );
            assert_eq!(next.payload(), Some(payload.as_deref().unwrap_or(b"old")));
            assert_eq!(
                (next.result(), next.error()),
                (before.result(), before.error())
            );
            assert_eq!(
                (next.id(), next.created_at(), next.attempts()),
                (before.id(), before.created_at(), before.attempts())
            );
            assert_eq!((next.version(), next.fence().get()), (3, 2));
            assert_eq!(record, before);
        }
    }
    Ok(())
}
