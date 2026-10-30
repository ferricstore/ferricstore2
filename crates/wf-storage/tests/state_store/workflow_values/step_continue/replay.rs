use super::{fixtures as f, *};
use wf_domain::retry::{Retry, RetryTime};

#[test]
fn durable_step_replays_original_before_changed_current_and_retains_all_gate_kinds() -> TestResult {
    let directory = common::directory("step-original-replay")?;
    let path = directory.join("state");
    let mut store = f::fresh(&path)?;
    f::seed(&mut store)?;
    let original_frame = f::frame(2, Some(b"new"), b"w:0:1")?;
    let original = f::retained(f::apply(&mut store, &original_frame, 1)?)?;
    let next = f::record(&mut store)?;
    let lease = next.record.lease().ok_or("Step lease")?;
    let retry = values::batch(
        5,
        b"retry",
        3,
        PreparedOperation::Batch(vec![values::item(
            b"i",
            3,
            Operation::Retry(Retry {
                guard: LeaseGuard {
                    token: lease.token.clone(),
                    fence: next.record.fence(),
                },
                run_at: RetryTime::Explicit(TimestampMillis::new(10)),
                payload: None,
                error: Some(b"again".to_vec()),
            }),
        )?]),
        vec![values::input(0, 0)],
        vec![values::witness(b"r", None)?],
    )?;
    assert_eq!(
        f::retained(values::apply_value(
            &mut store,
            &common::scope()?,
            &retry,
            2
        )?)?
        .status(),
        RetainedStatus::Accepted
    );
    let after = f::record(&mut store)?;
    assert_eq!(after.record.version(), 3);
    assert!(after.record.lease().is_none());
    assert_eq!(after.record.state(), b"charge");
    let duplicate = f::frame(4, Some(b"new"), b"w:0:1")?;
    let replay = f::retained(f::apply(&mut store, &duplicate, 3)?)?;
    assert_eq!(replay.as_ref(), original.as_ref());
    assert_eq!(f::record(&mut store)?, after);
    let conflict = f::frame(5, Some(b"conflict"), b"w:0:1")?;
    assert!(matches!(
        f::apply(&mut store, &conflict, 4)?,
        RecordBatchTransactionOutcome::IntentConflict(_)
    ));
    let expired = f::frame(102, Some(b"new"), b"w:0:1")?;
    assert!(matches!(
        f::apply(&mut store, &expired, 5)?,
        RecordBatchTransactionOutcome::Expired(_)
    ));
    assert!(matches!(
        f::apply(&mut store, &expired, 5)?,
        RecordBatchTransactionOutcome::LogReplay(_)
    ));
    assert_eq!(f::record(&mut store)?, after);
    let history = f::history(&store)?;
    assert_eq!(history.entries.len(), 3);
    assert_eq!(history.entries[1].event.event_id, "2-2");
    drop(store);
    let rows = common::raw_rows(&path)?;
    let reopened = common::open(&path)?;
    assert_eq!(f::history(&reopened)?.entries.len(), 3);
    let key = key::encode_outcome_key(
        &common::scope()?,
        &RequestId::new(b"step".to_vec())?,
        common::MIB,
    )?;
    let scanned = value::scan_step_continue_outcome(
        rows.get(&key).ok_or("original persisted outcome")?,
        &common::scope()?,
        &RequestId::new(b"step".to_vec())?,
        f::limits().record_batch.codec.admission(),
        common::MIB,
    )?;
    assert_eq!(scanned.into_owned(common::MIB)?, *original);
    drop(reopened);
    assert_eq!(common::raw_rows(&path)?, rows);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}
