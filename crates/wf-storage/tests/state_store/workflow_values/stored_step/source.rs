use super::{fixtures as s, *};
use wf_domain::history::EventKind;
use wf_domain::retry::{Retry, RetryTime};
use wf_domain::value::DomainError;
use wf_domain::workflow::Terminal;
use wf_domain::workflow::batch::BatchRejection;
use wf_domain::workflow::indexed::IndexRejection;
use wf_ports::blob::WorkflowValueBindingKnowledge;
use wf_types::version::TimestampMillis;

#[test]
fn stored_step_source_contract_rotates_lease_preserves_typed_history_and_retries_new_state()
-> TestResult {
    // Pinned flow_write_contract_test.exs:1257–1322, opaque identities/time normalized.
    let dir = f::directory("stored-step-source")?;
    let path = dir.join("state");
    let mut store = c::fresh(&path, true)?;
    let claimed = s::seed(&mut store)?;
    assert_eq!(claimed.record.state(), b"running");
    assert_eq!(claimed.record.run_state(), Some(b"queued".as_slice()));
    let frame = s::frame(Some(&s::map(b"charge_card")?))?;
    let original = s::apply(&mut store, &frame, 2)?;
    assert_eq!(
        (original.intent().version(), original.response_version()),
        (11, 9)
    );
    let continued = c::record(&mut store, b"i")?;
    assert_rotated(&claimed, &continued)?;
    let lease = continued.record.lease().ok_or("continued lease")?;
    s::assert_map(&store, 1, b"queued")?;
    s::assert_map(&store, 3, b"charge_card")?;
    let stale = s::mutation(
        &mut store,
        b"stale-complete",
        1002,
        3,
        Operation::Complete(Terminal {
            guard: s::guard(b"contract-step-worker:1000:1", 1)?,
            value: None,
            payload: None,
        }),
    )?;
    assert_eq!(
        stale.status(),
        RetainedStatus::IndexedRejected(IndexRejection::Batch(BatchRejection::Domain(
            DomainError::StaleLease
        )))
    );
    assert_eq!(c::record(&mut store, b"i")?, continued);
    let retry = s::mutation(
        &mut store,
        b"retry",
        1003,
        4,
        Operation::Retry(Retry {
            guard: s::guard(lease.token.as_bytes(), 2)?,
            run_at: RetryTime::Explicit(TimestampMillis::new(4000)),
            payload: None,
            error: Some(b"FSV2\x02\x83t\0\0\0\x01w\x06reasonm\0\0\0\x09try-again".to_vec()),
        }),
    )?;
    assert_eq!(retry.status(), RetainedStatus::Accepted);
    let retried = c::record(&mut store, b"i")?;
    assert_eq!(retried.record.state(), b"charge_card");
    assert_eq!(retried.record.run_state(), None);
    assert_eq!(
        retried.record.next_run_at(),
        Some(TimestampMillis::new(4000))
    );
    assert_eq!(retried.record.payload(), continued.record.payload());
    let page = c::history(&store, b"i")?;
    assert_eq!(page.entries.len(), 4);
    assert_eq!(page.entries[2].event.kind, EventKind::StepContinued);
    assert!(matches!(
        &page.entries[2].values,
        WorkflowValueBindingKnowledge::Known(Some(_))
    ));
    assert_eq!(s::apply(&mut store, &frame, 5)?, original);
    assert_eq!(c::record(&mut store, b"i")?, retried);
    drop(store);
    let reopened = crate::fixtures::open(&path)?;
    s::assert_map(&reopened, 3, b"charge_card")?;
    let expected = c::history(&reopened, b"i")?;
    // Replay advances control while preserving all original history and bindings.
    assert_eq!(page.entries.len(), expected.entries.len());
    for (left, right) in page.entries.iter().zip(&expected.entries) {
        assert_eq!(left.event, right.event);
    }
    drop(reopened);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

fn assert_rotated(
    claimed: &wf_ports::state::StoredWorkflowRecord,
    continued: &wf_ports::state::StoredWorkflowRecord,
) -> TestResult {
    assert_eq!(continued.record.id(), claimed.record.id());
    assert_eq!(continued.record.state(), b"running");
    assert_eq!(
        continued.record.run_state(),
        Some(b"charge_card".as_slice())
    );
    let lease = continued.record.lease().ok_or("continued lease")?;
    assert_ne!(
        lease.token,
        claimed.record.lease().ok_or("claim lease")?.token
    );
    assert_eq!(
        continued.record.fence().get(),
        claimed.record.fence().get() + 1
    );
    assert_eq!(lease.deadline.get(), 6001);
    Ok(())
}
