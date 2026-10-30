use super::{fixtures as f, *};
use wf_domain::history::EventKind;
use wf_domain::retry::{Retry, RetryTime};
use wf_domain::value::DomainError;
use wf_domain::workflow::reduce;

fn check_returned_guard(next: &WorkflowRecord) -> TestResult {
    let lease = next.lease().ok_or("returned lease")?;
    let retry = CommittedCommand {
        id: next.id().clone(),
        now: TimestampMillis::new(3),
        operation: Operation::Retry(Retry {
            guard: LeaseGuard {
                token: lease.token.clone(),
                fence: next.fence(),
            },
            run_at: RetryTime::Explicit(TimestampMillis::new(5)),
            payload: None,
            error: None,
        }),
    };
    let retried = reduce(Some(next), &retry)?
        .into_parts()
        .1
        .ok_or("actual-guard Retry")?
        .record;
    assert_eq!(retried.state(), b"charge");
    assert!(retried.lease().is_none());
    let mut stale = retry;
    let Operation::Retry(value) = &mut stale.operation else {
        return Err("Retry fixture".into());
    };
    value.guard = LeaseGuard {
        token: LeaseToken::new(b"lease".to_vec())?,
        fence: Fence::new(1),
    };
    assert_eq!(
        reduce(Some(next), &stale).err(),
        Some(DomainError::StaleLease)
    );
    Ok(())
}

#[test]
fn fresh_step_rotates_guard_and_retains_actual_reply() -> TestResult {
    let frame = f::frame(2, b"charge", Some(b"\0\xff"))?;
    let record = f::record()?;
    let plan = f::plan(&frame, &record, StateMode::Parallel)?;
    let RecordBatchApplyDecision::Fresh { operation, outcome } = plan.decision() else {
        return Err("Fresh result".into());
    };
    let IndexedDecision::Accepted(batch) = operation else {
        return Err("Step rejected".into());
    };
    let write = batch.batch().items()[0]
        .plan()
        .write()
        .ok_or("actual write")?;
    let next = &write.record;
    assert_eq!(write.expected_version, Some(2));
    assert_eq!(
        (next.state(), next.run_state()),
        (b"running".as_slice(), Some(b"charge".as_slice()))
    );
    assert_eq!(
        (next.version(), next.fence().get(), next.attempts()),
        (3, 2, 0)
    );
    assert_eq!(
        next.latest_event_witness(),
        (EventKind::StepContinued, 3, Fence::new(2))
    );
    assert_eq!(next.payload(), Some(b"\0\xff".as_slice()));
    let lease = next.lease().ok_or("returned lease")?;
    assert_eq!(lease.token.as_bytes(), b"worker:2:2");
    assert_eq!(lease.deadline.get(), 30_002);
    assert_eq!(
        (
            batch.consumed_ordinals(),
            batch.metadata().next().ok_or("metadata")?.state_enter_seq
        ),
        (1, 1_u128 << 32)
    );
    assert_eq!(
        (
            outcome.intent().version(),
            outcome.response_version(),
            outcome.original_position(),
            outcome.original_time().get(),
            outcome.expires_at().get()
        ),
        (10, 9, f::log(1), 2, 102)
    );
    assert_eq!(outcome.status(), RetainedStatus::Accepted);
    assert!(step_continue_response_matches(
        operation,
        outcome.response(),
        f::log(1),
        TimestampMillis::new(2),
        f::limits()?.admission()
    )?);
    let effects = plan.value_effects().ok_or("actual provenance")?.steps();
    assert_eq!(effects.len(), 1);
    assert_eq!(
        (
            effects[0].item_index(),
            effects[0].version(),
            effects[0].supplied(),
            effects[0].terminal()
        ),
        (0, 3, [true, false, false], false)
    );
    let decoded = prepare_step_continue(frame.as_bytes(), f::limits()?, WORKSPACE)?;
    assert!(matches!(
        inspect_record_batch_value_effects(
            &decoded,
            &plan,
            std::mem::size_of::<RecordBatchValueEffect>(),
            WORKSPACE
        ),
        Ok(Some(_))
    ));
    check_returned_guard(next)?;
    assert_eq!(
        (record.version(), record.payload()),
        (2, Some(b"old".as_slice()))
    );
    Ok(())
}

#[test]
fn fresh_step_preserves_equal_state_order_and_payload_presence() -> TestResult {
    let healthy = f::frame(2, b"charge", None)?;
    drop(f::fresh(healthy.as_bytes(), WORKSPACE)?);
    let record = f::record()?;
    for payload in [None, Some(b"".as_slice()), Some(b"\0\xff".as_slice())] {
        let frame = f::frame(2, b"authorize", payload)?;
        let plan = f::plan(&frame, &record, StateMode::Parallel)?;
        let RecordBatchApplyDecision::Fresh {
            operation: IndexedDecision::Accepted(batch),
            outcome,
        } = plan.decision()
        else {
            return Err("equal-state Step".into());
        };
        let next = &batch.batch().items()[0]
            .plan()
            .write()
            .ok_or("same-state write")?
            .record;
        assert_eq!(next.payload(), payload.or(Some(b"old".as_slice())));
        assert_eq!(
            (batch.consumed_ordinals(), batch.next_ordinal()),
            (0, Some(0))
        );
        assert_eq!(
            batch.metadata().next().ok_or("ordering")?.state_enter_seq,
            5
        );
        assert_eq!(
            plan.value_effects().ok_or("presence provenance")?.steps()[0].supplied(),
            [payload.is_some(), false, false]
        );
        assert_eq!(outcome.status(), RetainedStatus::Accepted);
    }
    Ok(())
}
