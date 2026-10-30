use super::{fixtures as s, *};
use wf_domain::history::EventKind;
use wf_domain::lease::{LeaseGuard, LeaseToken};
use wf_domain::retry::{Retry, RetryTime};
use wf_domain::workflow::{Operation, StepContinue};
use wf_ports::consensus::{PreparedOperation, PreparedRequest};
use wf_ports::log::{encode_step_continue, encode_workflow_stored_value_batch};
use wf_storage::transaction::apply_step_continue_transaction;
use wf_types::identity::RequestId;
use wf_types::version::{Fence, TimestampMillis};

#[test]
fn named_claim_step_retry_original_fifo_activation_preserves_fresh_entry_order() -> TestResult {
    // Pinned flow/invariant_test.exs850–920, including both Create timestamps.
    let dir = f::directory("named-claim-step-source-fifo")?;
    let path = dir.join("state");
    let mut store = s::fresh(&path, true)?;
    s::create(
        &mut store,
        b"transitioning",
        b"reserve",
        (0, 1000, 2000),
        None,
    )?;
    s::create(&mut store, b"waiting", b"charge", (1, 1001, 2200), None)?;
    let claimed = s::retained(s::apply(
        &mut store,
        &s::phase(b"claim-reserve", 2000, b"reserve", b"parallel-worker", 1)?,
        2,
    )?)?;
    s::assert_first_id(&claimed, b"transitioning")?;
    let before = s::record(&mut store, b"transitioning")?;
    assert_eq!(before.record.state(), b"running");
    assert_eq!(before.record.run_state(), Some(b"reserve".as_slice()));
    assert_eq!(
        (
            before.record.version(),
            before.record.fence().get(),
            before.record.attempts()
        ),
        (2, 1, 0)
    );
    let step = step_frame(
        b"transitioning",
        2100,
        b"reserve",
        b"charge",
        b"parallel-worker",
        b"parallel-worker:2000:1",
        None,
    )?;
    assert_eq!(
        s::retained_record(apply_step_continue_transaction(
            &mut store,
            &f::scope()?,
            step.as_bytes(),
            f::log(3),
            s::limits()
        )?)?
        .status(),
        RetainedStatus::Accepted
    );
    let continued = s::record(&mut store, b"transitioning")?;
    assert_eq!(continued.record.fence().get(), 2);
    assert_eq!(continued.ordering.state_enter_seq, 3_u128 << 32);
    assert_ne!(continued.record.lease(), before.record.lease());
    retry(&mut store, &continued, 4, 2101, 2200)?;
    let retried = s::record(&mut store, b"transitioning")?;
    assert_eq!(retried.record.state(), b"charge");
    assert_eq!(retried.record.run_state(), None);
    assert_eq!(
        retried.record.next_run_at(),
        Some(TimestampMillis::new(2200))
    );
    assert_eq!(retried.ordering.state_enter_seq, 3_u128 << 32);
    s::fifo(&mut store, 5, 2150)?;
    let fifo = s::retained(s::apply(
        &mut store,
        &s::phase(b"claim-charge", 2200, b"charge", b"fifo-worker", 1)?,
        6,
    )?)?;
    s::assert_first_id(&fifo, b"waiting")?;
    assert!(s::record(&mut store, b"waiting")?.record.lease().is_some());
    assert!(
        s::record(&mut store, b"transitioning")?
            .record
            .lease()
            .is_none()
    );
    let history = s::history(&store, b"transitioning")?;
    assert_eq!(
        history
            .entries
            .iter()
            .map(|row| row.event.kind)
            .collect::<Vec<_>>(),
        [
            EventKind::Created,
            EventKind::Claimed,
            EventKind::StepContinued,
            EventKind::Retry
        ]
    );
    drop(store);
    let mut reopened = f::open(&path)?;
    assert_eq!(s::record(&mut reopened, b"transitioning")?, retried);
    assert_eq!(s::history(&reopened, b"transitioning")?.entries.len(), 4);
    drop(reopened);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

fn step_frame(
    id: &[u8],
    now: u64,
    from: &[u8],
    to: &[u8],
    worker: &[u8],
    token: &[u8],
    payload: Option<&[u8]>,
) -> TestResult<PreparedBytes> {
    Ok(encode_step_continue(
        PreparedRequest {
            scope: f::scope()?,
            request: RequestId::new(b"step".to_vec())?,
            intent: ClientIntent::new(1, [0; 32])?,
            stamp: f::clock(now)?,
            operation: PreparedOperation::Batch(vec![v::item(
                id,
                now,
                Operation::StepContinue(StepContinue {
                    guard: LeaseGuard {
                        token: LeaseToken::new(token.to_vec())?,
                        fence: Fence::new(1),
                    },
                    from: from.to_vec(),
                    to: to.to_vec(),
                    worker: Some(worker.to_vec()),
                    lease_ms: 30000,
                    payload: payload.map(<[u8]>::to_vec),
                }),
            )?]),
        },
        s::limits().record_batch.codec,
        s::limits().record_batch.workspace_bytes,
    )?)
}

fn retry(
    store: &mut RedbStateStore,
    record: &wf_ports::state::StoredWorkflowRecord,
    at: u64,
    now: u64,
    run_at: u64,
) -> TestResult {
    // Actual typed stored %{reason: "retry"}, not a binary replacement for the map.
    let error = b"FSV2\x02\x83\x74\0\0\0\x01\x77\x06reason\x6d\0\0\0\x05retry";
    let request = v::batch(
        5,
        b"retry",
        now,
        PreparedOperation::Batch(vec![v::item(
            record.record.id().as_bytes(),
            now,
            Operation::Retry(Retry {
                guard: LeaseGuard {
                    token: record
                        .record
                        .lease()
                        .ok_or("continued lease")?
                        .token
                        .clone(),
                    fence: record.record.fence(),
                },
                run_at: RetryTime::Explicit(TimestampMillis::new(run_at)),
                payload: None,
                error: Some(error.to_vec()),
            }),
        )?]),
        vec![v::input(0, 0)],
        vec![v::witness(b"r", None)?],
    )?;
    let frame = encode_workflow_stored_value_batch(&request, s::limits().record_batch.codec)?;
    assert_eq!(
        s::retained_record(
            wf_storage::transaction::apply_workflow_stored_value_batch_transaction(
                store,
                &f::scope()?,
                frame.as_bytes(),
                f::log(at),
                s::limits()
            )?
        )?
        .status(),
        RetainedStatus::Accepted
    );
    Ok(())
}
