//! Actual separate-file commit failures and acknowledged advancement kills.

use super::{
    CommittedRecoveryError, Error, RaftLogStore, SnapshotError, SnapshotLimits, StateSnapshotStore,
    TestResult, caps, disk_backend, f, fixtures, purge_snapshot, recover_snapshotted,
};
use std::{
    io::Write,
    path::Path,
    process::{Child, Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicU8, Ordering},
        mpsc,
    },
    time::Duration,
};
use wf_ports::{
    log::{RaftLogError, RaftLogLimits, RaftLogState},
    snapshot::{SnapshotTransition, StateSnapshotTransitionStore},
    state::StateTransactionError,
};
use wf_storage::{
    log::RedbLogStore,
    state::{RedbStateStore, StateStoreLimits},
};

fn log_backend(dir: &Path, mode: Arc<AtomicU8>) -> TestResult<RedbLogStore> {
    let s = f::scope()?;
    Ok(RedbLogStore::open_database(
        disk_backend::fault_database(&dir.join("log.redb"), mode, 16 * f::MIB)?,
        s.group().clone(),
        s.recovery().clone(),
        RaftLogLimits::default(),
    )?)
}

fn state_backend(dir: &Path, mode: Arc<AtomicU8>) -> TestResult<RedbStateStore> {
    let s = f::scope()?;
    Ok(RedbStateStore::open_database(
        disk_backend::fault_database(&dir.join("state.redb"), mode, 16 * f::MIB)?,
        s.group().clone(),
        s.recovery().clone(),
        StateStoreLimits::default(),
    )?)
}

fn reconcile(dir: &Path, expected: &[f::Rows; 6], id: [u8; 32]) -> TestResult {
    let (log, state) = f::open_pair(dir)?;
    let scope = f::scope()?;
    let (log, state) = purge_snapshot(
        log,
        state,
        scope.group(),
        scope.recovery(),
        caps()?,
        SnapshotLimits::default(),
    )?;
    let (mut log, mut state) = recover_snapshotted(
        log,
        state,
        scope.group(),
        scope.recovery(),
        caps()?,
        SnapshotLimits::default(),
    )?;
    assert_eq!(log.state()?.last_purged(), Some(f::at(1)));
    assert_eq!(state.snapshot_transition(SnapshotLimits::default())?, None);
    assert_eq!(state.snapshot_pin(SnapshotLimits::default())?, Some(id));
    drop((log, state));
    assert_eq!(fixtures::cohort(dir)?, *expected);
    assert!(f::log_rows(dir)?.is_empty());
    Ok(())
}

#[test]
fn state_prepare_or_finish_uncertainty_retains_complete_recoverable_artifact_cohorts() -> TestResult
{
    for finish in [false, true] {
        let dir = f::directory("advance-state-uncertain")?;
        let (mut log, state, id) = fixtures::candidate(&dir)?;
        let t = SnapshotTransition::local(log.state()?, f::at(1), id)?;
        drop((log, state));
        let old = fixtures::cohort(&dir)?;
        let pending = fixtures::pending(old.clone(), t)?;
        let expected = fixtures::finalized(pending.clone(), fixtures::slot(&pending, id)?)?;
        let (mut log, mut state) = f::open_pair(&dir)?;
        if finish {
            state.prepare_snapshot_transition(t, SnapshotLimits::default())?;
            log.purge_through(f::at(1), id)?;
        }
        drop((log, state));
        let meta = f::log_meta(&dir)?;
        let rows = f::log_rows(&dir)?;
        let (log, unused) = f::open_pair(&dir)?;
        drop(unused);
        let fault = Arc::new(AtomicU8::new(0));
        let state = state_backend(&dir, Arc::clone(&fault))?;
        fault.store(4, Ordering::SeqCst);
        let scope = f::scope()?;
        let result = if finish {
            recover_snapshotted(
                log,
                state,
                scope.group(),
                scope.recovery(),
                caps()?,
                SnapshotLimits::default(),
            )
        } else {
            purge_snapshot(
                log,
                state,
                scope.group(),
                scope.recovery(),
                caps()?,
                SnapshotLimits::default(),
            )
        };
        assert_eq!(
            result.err().ok_or("state sync fault accepted")?,
            Error::Snapshot(SnapshotError::State(StateTransactionError::Uncertain))
        );
        assert_eq!(fault.load(Ordering::SeqCst), 0);
        assert_eq!(f::log_meta(&dir)?, meta);
        assert_eq!(f::log_rows(&dir)?, rows);
        let actual = fixtures::cohort(&dir)?;
        if finish {
            assert!(actual == pending || actual == expected);
        } else {
            assert!(actual == old || actual == pending);
        }
        reconcile(&dir, &expected, id)?;
        std::fs::remove_dir_all(dir)?;
    }
    Ok(())
}

#[test]
fn log_uncertainty_occurs_only_after_pending_old_and_new_data_are_durable() -> TestResult {
    let dir = f::directory("advance-log-uncertain")?;
    let (mut log, state, id) = fixtures::candidate(&dir)?;
    let before = log.state()?;
    let t = SnapshotTransition::local(before, f::at(1), id)?;
    drop((log, state));
    let old = fixtures::cohort(&dir)?;
    let pending = fixtures::pending(old, t)?;
    let expected = fixtures::finalized(pending.clone(), fixtures::slot(&pending, id)?)?;
    let old_meta = f::log_meta(&dir)?;
    let old_rows = f::log_rows(&dir)?;
    let new_meta = fixtures::log_state(
        old_meta.clone(),
        RaftLogState::new_with_anchor(
            before.vote(),
            before.committed(),
            before.last_log(),
            Some((f::at(1), id)),
        ),
    );
    let (log, state) = f::open_pair(&dir)?;
    drop(log);
    let fault = Arc::new(AtomicU8::new(0));
    let log = log_backend(&dir, Arc::clone(&fault))?;
    fault.store(4, Ordering::SeqCst);
    let scope = f::scope()?;
    assert_eq!(
        purge_snapshot(
            log,
            state,
            scope.group(),
            scope.recovery(),
            caps()?,
            SnapshotLimits::default()
        )
        .err()
        .ok_or("log sync fault accepted")?,
        Error::Recovery(CommittedRecoveryError::Log(RaftLogError::Uncertain))
    );
    assert_eq!(fault.load(Ordering::SeqCst), 0);
    assert_eq!(fixtures::cohort(&dir)?, pending);
    let meta = f::log_meta(&dir)?;
    let rows = f::log_rows(&dir)?;
    assert!((meta == old_meta && rows == old_rows) || (meta == new_meta && rows.is_empty()));
    reconcile(&dir, &expected, id)?;
    assert_eq!(f::log_meta(&dir)?, new_meta);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

fn child(dir: &Path, phase: &str) -> TestResult {
    let (log, state) = f::open_pair(dir)?;
    let fault = Arc::new(AtomicU8::new(0));
    let scope = f::scope()?;
    if phase == "log" {
        drop(log);
        let log = log_backend(dir, Arc::clone(&fault))?;
        fault.store(6, Ordering::SeqCst);
        let _owners = purge_snapshot(
            log,
            state,
            scope.group(),
            scope.recovery(),
            caps()?,
            SnapshotLimits::default(),
        )?;
    } else if phase == "state" {
        drop(state);
        let state = state_backend(dir, Arc::clone(&fault))?;
        fault.store(6, Ordering::SeqCst);
        let _owners = purge_snapshot(
            log,
            state,
            scope.group(),
            scope.recovery(),
            caps()?,
            SnapshotLimits::default(),
        )?;
    } else {
        let _owners = purge_snapshot(
            log,
            state,
            scope.group(),
            scope.recovery(),
            caps()?,
            SnapshotLimits::default(),
        )?;
        disk_backend::parked("after-return")?;
    }
    Err("child missed requested advance boundary".into())
}

fn killed(child: &mut Child, after: bool) -> TestResult {
    let result = (|| -> TestResult {
        let stdout = child.stdout.take().ok_or("advance stdout absent")?;
        let mut stdin = child.stdin.take().ok_or("advance stdin absent")?;
        let (sender, receiver) = mpsc::sync_channel(2);
        let reader = disk_backend::signal_reader(stdout, sender, after)?;
        let acknowledged = (|| -> TestResult {
            receiver.recv_timeout(Duration::from_secs(20))??;
            stdin.write_all(b"A")?;
            stdin.flush()?;
            receiver.recv_timeout(Duration::from_secs(20))??;
            Ok(())
        })();
        let exit = disk_backend::terminate(child);
        drop(stdin);
        let joined = reader.join();
        acknowledged?;
        assert!(!exit?.success());
        joined.map_err(|_| "advance reader panicked")??;
        Ok(())
    })();
    if result.is_err() {
        let _ = disk_backend::terminate(child);
    }
    result
}

#[test]
fn acknowledged_state_prepare_log_switch_and_after_return_kills_reconcile_complete_generations()
-> TestResult {
    if let Some(dir) = std::env::var_os("WF_ADVANCE_DIR") {
        return child(Path::new(&dir), &std::env::var("WF_ADVANCE_PHASE")?);
    }
    for phase in ["state", "log", "after"] {
        let dir = f::directory("advance-kill")?;
        let (mut log, state, id) = fixtures::candidate(&dir)?;
        let before = log.state()?;
        let t = SnapshotTransition::local(before, f::at(1), id)?;
        drop((log, state));
        let old = fixtures::cohort(&dir)?;
        let pending = fixtures::pending(old.clone(), t)?;
        let expected = fixtures::finalized(pending.clone(), fixtures::slot(&pending, id)?)?;
        let old_meta = f::log_meta(&dir)?;
        let old_rows = f::log_rows(&dir)?;
        let new_meta = fixtures::log_state(
            old_meta.clone(),
            RaftLogState::new_with_anchor(
                before.vote(),
                before.committed(),
                before.last_log(),
                Some((f::at(1), id)),
            ),
        );
        let mut process=Command::new(std::env::current_exe()?).args(["--exact",
            "advance::faults::acknowledged_state_prepare_log_switch_and_after_return_kills_reconcile_complete_generations",
            "--nocapture","--test-threads=1"]).env("WF_ADVANCE_DIR",&dir).env("WF_ADVANCE_PHASE",phase)
            .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::inherit()).spawn()?;
        killed(&mut process, phase == "after")?;
        let actual = fixtures::cohort(&dir)?;
        let meta = f::log_meta(&dir)?;
        let rows = f::log_rows(&dir)?;
        match phase {
            "state" => {
                assert!(actual == old || actual == pending);
                assert_eq!(meta, old_meta);
                assert_eq!(rows, old_rows);
            }
            "log" => {
                assert_eq!(actual, pending);
                assert!(
                    (meta == old_meta && rows == old_rows) || (meta == new_meta && rows.is_empty())
                );
            }
            _ => {
                assert_eq!(actual, expected);
                assert_eq!(meta, new_meta);
                assert!(rows.is_empty());
            }
        }
        reconcile(&dir, &expected, id)?;
        std::fs::remove_dir_all(dir)?;
    }
    Ok(())
}
