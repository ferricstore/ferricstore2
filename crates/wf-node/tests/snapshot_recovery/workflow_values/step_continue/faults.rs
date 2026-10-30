use super::{fixtures as s, *};
use std::sync::{
    Arc,
    atomic::{AtomicU8, Ordering},
};
use wf_ports::state::StateTransactionError;
use wf_storage::state::StateStoreLimits;
use wf_storage::transaction::{RecordBatchTransactionError, WorkflowValueTransactionError};

#[test]
fn step_node_uncertain_commit_reopens_exact_old_or_new_complete_cohort() -> TestResult {
    let limits = s::limits()?;
    let reference = f::directory("step-node-fault-reference")?;
    s::pending(&reference, true, false)?;
    let expected = s::cohort(&reference)?;
    let dir = f::directory("step-node-fault-live")?;
    s::pending(&dir, false, false)?;
    let old = s::cohort(&dir)?;
    assert_ne!(old, expected);
    let rows = f::log_rows(&dir)?;
    let meta = f::log_meta(&dir)?;
    let (log, state) = f::open_pair(&dir)?;
    drop(state);
    let scope = f::scope()?;
    let fault = Arc::new(AtomicU8::new(0));
    let state = RedbStateStore::open_database(
        disk_backend::fault_database(&dir.join("state.redb"), Arc::clone(&fault), 16 * f::MIB)?,
        scope.group().clone(),
        scope.recovery().clone(),
        StateStoreLimits::default(),
    )?;
    fault.store(4, Ordering::SeqCst);
    assert_eq!(
        recover_committed(log, state, scope.group(), scope.recovery(), limits).err(),
        Some(Error::Value(WorkflowValueTransactionError::Record(
            RecordBatchTransactionError::Transaction(StateTransactionError::Uncertain)
        )))
    );
    assert_eq!(fault.load(Ordering::SeqCst), 0);
    drop(f::open_pair(&dir)?);
    let actual = s::cohort(&dir)?;
    assert!(
        actual == old || actual == expected,
        "partial Step cohort after uncertainty"
    );
    assert_eq!(f::log_rows(&dir)?, rows);
    assert_eq!(f::log_meta(&dir)?, meta);
    drop(s::recover(&dir, &limits)?);
    assert_eq!(s::cohort(&dir)?, expected);
    assert_eq!(f::log_rows(&dir)?, rows);
    assert_eq!(f::log_meta(&dir)?, meta);
    std::fs::remove_dir_all(reference)?;
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn step_node_later_value_refusal_preserves_real_nonzero_replay_prefix() -> TestResult {
    let healthy = s::limits()?;
    let dir = f::directory("step-node-prefix-live")?;
    let reference = f::directory("step-node-prefix-reference")?;
    prefix_input(&dir, false)?;
    prefix_input(&reference, true)?;
    let expected = s::cohort(&reference)?;
    let rows = f::log_rows(&dir)?;
    let meta = f::log_meta(&dir)?;
    let limited = v::limits()?.with_step_continue_limits(1024, 8192, 64 * f::MIB, 64 * f::MIB)?;
    let scope = f::scope()?;
    let (log, state) = f::open_pair(&dir)?;
    assert_eq!(
        recover_committed(log, state, scope.group(), scope.recovery(), limited).err(),
        Some(Error::Value(WorkflowValueTransactionError::Record(
            RecordBatchTransactionError::Transaction(StateTransactionError::Limit)
        )))
    );
    assert_eq!(s::cohort(&dir)?, expected);
    assert_eq!(f::log_rows(&dir)?, rows);
    assert_eq!(f::log_meta(&dir)?, meta);
    let (log, mut state) = s::recover(&dir, &healthy)?;
    assert_eq!(s::record(&mut state)?.record.version(), 3);
    assert_eq!(s::history(&state)?.entries.len(), 3);
    drop((log, state));
    std::fs::remove_dir_all(reference)?;
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

fn prefix_input(dir: &Path, applied: bool) -> TestResult {
    let (mut log, mut state, _) = s::seed(dir, false, true)?;
    let first = s::frame(b"first", 200, Some(b"small"))?;
    let large = vec![b'X'; 65536];
    let second = s::guarded_frame(
        b"second",
        210,
        Some(&large),
        (b"worker:200:2", 2, b"charge", b"next"),
    )?;
    f::append(&mut log, 2, first.as_bytes())?;
    f::append(&mut log, 3, second.as_bytes())?;
    log.save_committed(f::at(3))?;
    if applied {
        assert_eq!(
            s::retained(s::apply(&mut state, &first, 2)?)?.status(),
            RetainedStatus::Accepted
        );
    }
    drop((log, state));
    Ok(())
}
