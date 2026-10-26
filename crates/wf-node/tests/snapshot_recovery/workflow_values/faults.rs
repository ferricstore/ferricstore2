//! A failed delegated commit is uncertain; a later refusal keeps its real prefix.

use std::path::Path;
use std::sync::{
    Arc,
    atomic::{AtomicU8, Ordering},
};

use wf_ports::blob::OriginalValueRefKind;
use wf_ports::log::RaftLogStore;
use wf_ports::state::StateTransactionError;
use wf_storage::state::{RedbStateStore, StateStoreLimits};
use wf_storage::transaction::{RecordBatchTransactionError, WorkflowValueTransactionError};

use super::{Error, TestResult, disk_backend, f, fixtures as v, recover_committed};

#[test]
fn value_recovery_uncertain_and_later_refusal_preserve_reopenable_prefix() -> TestResult {
    let enabled =
        v::limits()?.with_workflow_value_limits(1_024, 64 * f::MIB, 64 * f::MIB, 64 * f::MIB);
    assert!(
        enabled.is_ok(),
        "explicit value recovery opt-in must succeed"
    );
    let limits = enabled?;
    uncertain_commit(&limits)?;
    later_refusal(&limits)?;
    Ok(())
}

fn state_backend(dir: &Path, fault: Arc<AtomicU8>) -> TestResult<RedbStateStore> {
    let scope = f::scope()?;
    Ok(RedbStateStore::open_database(
        disk_backend::fault_database(&dir.join("state.redb"), fault, 16 * f::MIB)?,
        scope.group().clone(),
        scope.recovery().clone(),
        StateStoreLimits::default(),
    )?)
}

fn uncertain_input(dir: &Path, applied: bool) -> TestResult {
    let (mut log, mut state) = v::pair(dir)?;
    let policy = v::install(&mut log, &mut state, false, None, 0, 10)?;
    let frame = v::encode(&v::batch(
        &f::scope()?,
        5,
        b"fault",
        100,
        vec![v::create(b"i", 100, b"A")?],
        policy,
    )?)?;
    f::append(&mut log, 1, frame.as_bytes())?;
    if applied {
        v::accepted(v::apply(&mut state, &frame, 1)?, 1, 100, 5)?;
    }
    log.save_committed(f::at(1))?;
    drop((log, state));
    Ok(())
}

fn uncertain_commit(limits: &super::Limits) -> TestResult {
    let reference = f::directory("value-uncertain-reference")?;
    uncertain_input(&reference, true)?;
    let new = v::cohort(&reference)?;
    let dir = f::directory("value-uncertain-live")?;
    uncertain_input(&dir, false)?;
    let old = v::cohort(&dir)?;
    assert_ne!(old, new);
    let logs = f::log_rows(&dir)?;
    let metadata = f::log_meta(&dir)?;
    let (log, state) = f::open_pair(&dir)?;
    drop(state);
    let fault = Arc::new(AtomicU8::new(0));
    let state = state_backend(&dir, Arc::clone(&fault))?;
    fault.store(4, Ordering::SeqCst);
    let scope = f::scope()?;
    let result = recover_committed(log, state, scope.group(), scope.recovery(), *limits);
    assert_eq!(
        result.err(),
        Some(Error::Value(WorkflowValueTransactionError::Record(
            RecordBatchTransactionError::Transaction(StateTransactionError::Uncertain),
        ))),
    );
    assert_eq!(
        fault.load(Ordering::SeqCst),
        0,
        "delegated commit fault was not consumed"
    );
    drop(f::open_pair(&dir)?);
    let actual = v::cohort(&dir)?;
    assert!(
        actual == old || actual == new,
        "uncertain commit exposed a partial cohort"
    );
    assert_eq!(f::log_rows(&dir)?, logs);
    assert_eq!(f::log_meta(&dir)?, metadata);
    let (log, state) = f::open_pair(&dir)?;
    drop(recover_committed(
        log,
        state,
        scope.group(),
        scope.recovery(),
        *limits,
    )?);
    assert_eq!(v::cohort(&dir)?, new);
    assert_eq!(f::log_rows(&dir)?, logs);
    assert_eq!(f::log_meta(&dir)?, metadata);
    drop(f::open_pair(&dir)?);
    std::fs::remove_dir_all(reference)?;
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

fn prefix_input(dir: &Path, applied: u64) -> TestResult {
    let (mut log, mut state) = v::pair(dir)?;
    let policy = v::install(&mut log, &mut state, true, None, 0, 10)?;
    let first = v::encode(&v::batch(
        &f::scope()?,
        5,
        b"first",
        100,
        vec![v::create(b"i", 100, b"A")?],
        v::copy_policy(&policy),
    )?)?;
    let second = v::encode(&v::batch(
        &f::scope()?,
        5,
        b"second",
        200,
        vec![v::create(b"j", 200, b"B")?, v::create(b"k", 200, b"C")?],
        policy,
    )?)?;
    for (at, frame) in [(1, first), (2, second)] {
        f::append(&mut log, at, frame.as_bytes())?;
        if at <= applied {
            v::accepted(
                v::apply(&mut state, &frame, at)?,
                at,
                if at == 1 { 100 } else { 200 },
                5,
            )?;
        }
    }
    log.save_committed(f::at(2))?;
    drop((log, state));
    Ok(())
}

fn later_refusal(limits: &super::Limits) -> TestResult {
    let prefix_dir = f::directory("value-prefix-reference")?;
    let final_dir = f::directory("value-final-reference")?;
    let dir = f::directory("value-prefix-live")?;
    prefix_input(&prefix_dir, 1)?;
    prefix_input(&final_dir, 2)?;
    prefix_input(&dir, 0)?;
    let prefix = v::cohort(&prefix_dir)?;
    let final_state = v::cohort(&final_dir)?;
    let logs = f::log_rows(&dir)?;
    let metadata = f::log_meta(&dir)?;
    let narrow = (*limits).with_workflow_value_limits(4, 64 * f::MIB, 64 * f::MIB, 64 * f::MIB)?;
    let scope = f::scope()?;
    let (log, state) = f::open_pair(&dir)?;
    assert_eq!(
        recover_committed(log, state, scope.group(), scope.recovery(), narrow).err(),
        Some(Error::Value(WorkflowValueTransactionError::Record(
            RecordBatchTransactionError::Transaction(StateTransactionError::Limit),
        ))),
    );
    let (log, mut state) = f::open_pair(&dir)?;
    assert_eq!(
        v::control(&mut state)?
            .last_entry()
            .map(wf_domain::workflow::control::EntryIdentity::log),
        Some(f::at(1))
    );
    drop((log, state));
    assert_eq!(v::cohort(&dir)?, prefix);
    assert_eq!(f::log_rows(&dir)?, logs);
    assert_eq!(f::log_meta(&dir)?, metadata);
    let (log, state) = f::open_pair(&dir)?;
    let (log, mut state) = recover_committed(log, state, scope.group(), scope.recovery(), *limits)?;
    assert_eq!(
        v::control(&mut state)?
            .last_entry()
            .map(wf_domain::workflow::control::EntryIdentity::log),
        Some(f::at(2))
    );
    let a = v::reference(b"i", OriginalValueRefKind::Payload, 1)?;
    let b = v::reference(b"j", OriginalValueRefKind::Payload, 1)?;
    let c = v::reference(b"k", OriginalValueRefKind::Payload, 1)?;
    let read = v::read(&state, &[&a, &b, &c, &a])?;
    drop((log, state, a, b, c));
    v::assert_read(
        &read,
        &[b"A", b"B", b"C"],
        &[Some(0), Some(1), Some(2), Some(0)],
    );
    assert_eq!(v::cohort(&dir)?, final_state);
    assert_eq!(f::log_rows(&dir)?, logs);
    assert_eq!(f::log_meta(&dir)?, metadata);
    drop(f::open_pair(&dir)?);
    for path in [prefix_dir, final_dir, dir] {
        std::fs::remove_dir_all(path)?;
    }
    Ok(())
}
