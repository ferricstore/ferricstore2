use super::{fixtures as s, *};
use wf_domain::history::EventKind;
use wf_domain::lease::LeaseGuard;
use wf_domain::retry::{Retry, RetryTime};
use wf_domain::workflow::Operation;
use wf_storage::transaction::WorkflowValueTransactionError;
use wf_types::version::TimestampMillis;

#[test]
fn step_node_fixture_control_uses_actual_upgraded_start_and_claim() -> TestResult {
    let dir = f::directory("step-node-fixture-control")?;
    let (log, mut state, _) = s::seed(&dir, false, true)?;
    let record = s::record(&mut state)?;
    assert_eq!(
        (
            record.record.version(),
            record.record.fence().get(),
            record.record.attempts()
        ),
        (1, 1, 1)
    );
    assert_eq!(
        record.record.lease().ok_or("seed lease")?.token.as_bytes(),
        b"worker:100:1"
    );
    assert_eq!(v::control(&mut state)?.position(), f::at(1).index());
    assert_eq!(s::history(&state)?.entries.len(), 1);
    drop((log, state));
    assert!(s::cohort(&dir)?[0][b"manifest".as_slice()].starts_with(b"FWSS\0\x08"));
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn step_node_legacy_history_and_retry_control_preserve_original_semantics() -> TestResult {
    let dir = f::directory("step-node-legacy-oracle-control")?;
    let (mut log, mut state, policy) = s::seed(&dir, false, false)?;
    assert_eq!(s::history(&state)?.entries[0].event.event_id, "100-1");
    let current = s::record(&mut state)?;
    let retry = v::encode(&v::batch(
        &f::scope()?,
        5,
        b"retry-control",
        200,
        vec![v::item(
            b"i",
            200,
            Operation::Retry(Retry {
                guard: LeaseGuard {
                    token: current.record.lease().ok_or("old lease")?.token.clone(),
                    fence: current.record.fence(),
                },
                run_at: RetryTime::Explicit(TimestampMillis::new(300)),
                payload: None,
                error: None,
            }),
        )?],
        v::copy_policy(&policy),
    )?)?;
    f::append(&mut log, 2, retry.as_bytes())?;
    v::accepted(v::apply(&mut state, &retry, 2)?, 2, 200, 5)?;
    log.save_committed(f::at(2))?;
    let after = s::record(&mut state)?;
    assert_eq!(after.record.state(), b"q");
    assert_eq!(after.record.run_state(), None);
    assert_eq!(s::history(&state)?.entries[1].event.event_id, "200-2");
    drop((log, state));
    assert!(s::cohort(&dir)?[0][b"manifest".as_slice()].starts_with(b"FWSS\0\x07"));
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn step_node_opt_in_and_actual_format_are_independent_requirements() -> TestResult {
    let limits = s::limits()?;
    assert!(v::limits()?.step_continue_limits().is_none());
    let dir = f::directory("step-node-opt-in")?;
    s::pending(&dir, false, false)?;
    s::refuse(&dir, &v::limits()?, Some(Error::UnsupportedEntry))?;
    let binary =
        v::limits()?.with_workflow_value_limits(1024, 64 * f::MIB, 64 * f::MIB, 64 * f::MIB)?;
    s::refuse(
        &dir,
        &binary,
        Some(Error::Codec(wf_ports::log::CodecError::UnsupportedVersion)),
    )?;
    let stored_dir = f::directory("step-node-stored-opt-in")?;
    s::pending(&stored_dir, false, true)?;
    let stored = v::limits()?.with_workflow_stored_value_limits(
        1024,
        64 * f::MIB,
        64 * f::MIB,
        64 * f::MIB,
    )?;
    s::refuse(
        &stored_dir,
        &stored,
        Some(Error::Codec(wf_ports::log::CodecError::UnsupportedVersion)),
    )?;
    std::fs::remove_dir_all(stored_dir)?;
    drop(s::recover(&dir, &limits)?);
    let legacy = f::directory("step-node-format7")?;
    let (mut log, state, _) = s::seed(&legacy, false, false)?;
    f::append(
        &mut log,
        2,
        s::frame(b"step", 200, Some(b"FSV2raw"))?.as_bytes(),
    )?;
    log.save_committed(f::at(2))?;
    drop((log, state));
    s::refuse(
        &legacy,
        &limits,
        Some(Error::Value(
            WorkflowValueTransactionError::UnsupportedProfile,
        )),
    )?;
    std::fs::remove_dir_all(legacy)?;
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn step_node_replays_committed_payload_presence_with_uncommitted_tail() -> TestResult {
    let limits = s::limits()?;
    for stored in [false, true] {
        for payload in [None, Some(b"".as_slice()), Some(b"FSV2\0\xff".as_slice())] {
            payload_case(&limits, stored, payload)?;
        }
    }
    Ok(())
}

fn payload_case(limits: &Limits, stored: bool, payload: Option<&[u8]>) -> TestResult {
    let dir = f::directory("step-node-payload")?;
    let (mut log, state, _) = s::seed(&dir, stored, true)?;
    let frame = s::frame(b"step", 200, payload)?;
    f::append(&mut log, 2, frame.as_bytes())?;
    f::append(&mut log, 3, b"uncommitted opaque tail")?;
    log.save_committed(f::at(2))?;
    drop((log, state));
    let raw_log = f::log_rows(&dir)?;
    let (mut log, mut state) = s::recover(&dir, limits)?;
    let record = s::record(&mut state)?;
    assert_eq!(
        (
            record.record.version(),
            record.record.fence().get(),
            record.record.attempts()
        ),
        (2, 2, 1)
    );
    assert_eq!(record.record.run_state(), Some(b"charge".as_slice()));
    assert_eq!(record.record.payload(), payload.or(Some(b"old")));
    assert_eq!(
        record.record.lease().ok_or("Step lease")?.token.as_bytes(),
        b"worker:200:2"
    );
    let receipt = v::outcome(&mut state, b"step")?;
    assert_eq!(
        (receipt.intent().version(), receipt.response_version()),
        (10, 9)
    );
    assert_eq!(receipt.status(), RetainedStatus::Accepted);
    assert_eq!(receipt.original_position(), f::at(2));
    assert_eq!(receipt.original_time().get(), 200);
    let history = s::history(&state)?;
    assert_eq!(history.entries.len(), 2);
    assert_eq!(history.entries[1].event.kind, EventKind::StepContinued);
    assert_eq!(history.entries[1].event.event_id, "200-2");
    assert_eq!(log.state()?.last_log(), Some(f::at(3)));
    assert_eq!(log.state()?.committed(), Some(f::at(2)));
    drop((log, state));
    let before = s::cohort(&dir)?;
    drop(s::recover(&dir, limits)?);
    assert_eq!(s::cohort(&dir)?, before);
    assert_eq!(f::log_rows(&dir)?, raw_log);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn step_node_retains_original_after_retry_and_audits_all_gate_controls() -> TestResult {
    let limits = s::limits()?;
    let dir = f::directory("step-node-original-replay")?;
    let (mut log, mut state, policy) = s::seed(&dir, false, true)?;
    let original_frame = s::frame(b"step", 200, Some(b"new"))?;
    f::append(&mut log, 2, original_frame.as_bytes())?;
    let original = s::retained(s::apply(&mut state, &original_frame, 2)?)?;
    let record = s::record(&mut state)?;
    let retry = v::encode(&v::batch(
        &f::scope()?,
        5,
        b"retry",
        205,
        vec![v::item(
            b"i",
            205,
            Operation::Retry(Retry {
                guard: LeaseGuard {
                    token: record.record.lease().ok_or("lease")?.token.clone(),
                    fence: record.record.fence(),
                },
                run_at: RetryTime::Explicit(TimestampMillis::new(1300)),
                payload: None,
                error: Some(b"again".to_vec()),
            }),
        )?],
        v::copy_policy(&policy),
    )?)?;
    f::append(&mut log, 3, retry.as_bytes())?;
    f::append(
        &mut log,
        4,
        s::frame(b"step", 210, Some(b"new"))?.as_bytes(),
    )?;
    f::append(
        &mut log,
        5,
        s::frame(b"step", 220, Some(b"conflict"))?.as_bytes(),
    )?;
    let expired = s::frame(b"step", 1200, Some(b"new"))?;
    f::append(&mut log, 6, expired.as_bytes())?;
    log.save_committed(f::at(6))?;
    drop((log, state));
    let (log, mut state) = s::recover(&dir, &limits)?;
    let current = s::record(&mut state)?;
    assert_eq!(current.record.version(), 3);
    assert!(current.record.lease().is_none());
    assert_eq!(current.record.state(), b"charge");
    assert_eq!(current.record.run_state(), None);
    assert_eq!(s::history(&state)?.entries.len(), 3);
    assert_eq!(v::outcome(&mut state, b"step")?, *original);
    assert_eq!(v::control(&mut state)?.position(), f::at(6).index());
    assert!(matches!(
        s::apply(&mut state, &expired, 6)?,
        wf_storage::transaction::RecordBatchTransactionOutcome::LogReplay(_)
    ));
    drop((log, state));
    let before = s::cohort(&dir)?;
    drop(s::recover(&dir, &limits)?);
    assert_eq!(s::cohort(&dir)?, before);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}
