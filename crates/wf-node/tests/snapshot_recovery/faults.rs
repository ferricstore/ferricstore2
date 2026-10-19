//! Real separate-file pin and purge failures, acknowledged inside-log sync kills.

use super::{
    CommittedRecoveryError, Error, RaftLogStore, SnapshotError, SnapshotLimits, StateSnapshotStore,
    TestResult, caps, disk_backend, f, fixtures, purge_snapshot, recover_snapshotted,
};
use sha2::{Digest, Sha256};
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
    log::{RaftLogError, RaftLogLimits},
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

fn pinned(mut state: [f::Rows; 4]) -> TestResult<[f::Rows; 4]> {
    let descriptor = state[0]
        .get_mut(b"snapshot".as_slice())
        .ok_or("descriptor absent")?;
    assert_eq!(descriptor.len(), 87);
    assert_eq!(&descriptor[..7], b"FWSD\0\x01\x01");
    descriptor[5] = 2;
    let mut hash = Sha256::new();
    hash.update(b"wf-state-snapshot-descriptor-v1\0\x01g\0\x01z");
    hash.update(&descriptor[..55]);
    descriptor[55..].copy_from_slice(&hash.finalize());
    Ok(state)
}

fn anchored(mut meta: f::Rows, id: [u8; 32]) -> f::Rows {
    let mut wire = b"FWLS\0\x02\x0e".to_vec();
    for index in [1_u64, 1, 0] {
        wire.extend_from_slice(&1_u64.to_be_bytes());
        wire.extend_from_slice(&7_u64.to_be_bytes());
        wire.extend_from_slice(&index.to_be_bytes());
    }
    wire.extend_from_slice(&id);
    let mut hash = Sha256::new();
    hash.update(b"wf-raft-log-v1\0\x01g\0\x01z");
    hash.update(&wire);
    wire.extend_from_slice(&hash.finalize());
    assert!(meta.insert(b"state".to_vec(), wire).is_some());
    meta
}

fn reconcile(dir: &Path, id: [u8; 32], expected: &[f::Rows; 4]) -> TestResult {
    let s = f::scope()?;
    let (log, state) = f::open_pair(dir)?;
    let (log, state) = purge_snapshot(
        log,
        state,
        s.group(),
        s.recovery(),
        caps()?,
        SnapshotLimits::default(),
    )?;
    let (mut log, mut state) = recover_snapshotted(
        log,
        state,
        s.group(),
        s.recovery(),
        caps()?,
        SnapshotLimits::default(),
    )?;
    assert_eq!(log.state()?.last_purged(), Some(f::at(0)));
    assert_eq!(log.state()?.purged_snapshot_id(), Some(id));
    assert_eq!(state.snapshot_pin(SnapshotLimits::default())?, Some(id));
    drop((log, state));
    assert_eq!(fixtures::cohort(dir)?, *expected);
    assert_eq!(
        f::log_rows(dir)?.keys().copied().collect::<Vec<_>>(),
        vec![1]
    );
    Ok(())
}

#[test]
fn uncertain_state_pin_never_deletes_log_prefix_and_reopen_reproves_provenance() -> TestResult {
    let dir = f::directory("pin-before-purge-uncertain")?;
    let (log, state, id) = fixtures::seeded(&dir)?;
    drop((log, state));
    let old = fixtures::cohort(&dir)?;
    let new = pinned(old.clone())?;
    let meta = f::log_meta(&dir)?;
    let rows = f::log_rows(&dir)?;
    let (log, state) = f::open_pair(&dir)?;
    drop(state);
    let fault = Arc::new(AtomicU8::new(0));
    let state = state_backend(&dir, Arc::clone(&fault))?;
    fault.store(4, Ordering::SeqCst);
    let s = f::scope()?;
    let error = purge_snapshot(
        log,
        state,
        s.group(),
        s.recovery(),
        caps()?,
        SnapshotLimits::default(),
    )
    .err()
    .ok_or("pin sync injection did not refuse")?;
    assert_eq!(
        error,
        Error::Snapshot(SnapshotError::State(StateTransactionError::Uncertain))
    );
    assert_eq!(fault.load(Ordering::SeqCst), 0);
    assert_eq!(f::log_meta(&dir)?, meta);
    assert_eq!(f::log_rows(&dir)?, rows);
    let actual = fixtures::cohort(&dir)?;
    assert!(
        actual == old || actual == new,
        "partial state descriptor publication"
    );
    reconcile(&dir, id, &new)?;
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn uncertain_log_purge_always_leaves_exact_snapshot_durably_pinned() -> TestResult {
    let dir = f::directory("purge-after-pin-uncertain")?;
    let (log, state, id) = fixtures::seeded(&dir)?;
    drop((log, state));
    let expected = pinned(fixtures::cohort(&dir)?)?;
    let old_meta = f::log_meta(&dir)?;
    let old_rows = f::log_rows(&dir)?;
    let new_meta = anchored(old_meta.clone(), id);
    let mut new_rows = old_rows.clone();
    assert!(new_rows.remove(&0).is_some());
    let (log, state) = f::open_pair(&dir)?;
    drop(log);
    let fault = Arc::new(AtomicU8::new(0));
    let log = log_backend(&dir, Arc::clone(&fault))?;
    fault.store(4, Ordering::SeqCst);
    let s = f::scope()?;
    let error = purge_snapshot(
        log,
        state,
        s.group(),
        s.recovery(),
        caps()?,
        SnapshotLimits::default(),
    )
    .err()
    .ok_or("log sync injection did not refuse")?;
    assert_eq!(
        error,
        Error::Recovery(CommittedRecoveryError::Log(RaftLogError::Uncertain))
    );
    assert_eq!(fault.load(Ordering::SeqCst), 0);
    assert_eq!(fixtures::cohort(&dir)?, expected);
    let meta = f::log_meta(&dir)?;
    let rows = f::log_rows(&dir)?;
    assert!(
        (meta == old_meta && rows == old_rows) || (meta == new_meta && rows == new_rows),
        "log deletion and anchor must be one complete publication"
    );
    reconcile(&dir, id, &expected)?;
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

fn child(dir: &Path, after: bool) -> TestResult {
    let (log, state) = f::open_pair(dir)?;
    drop(log);
    let fault = Arc::new(AtomicU8::new(0));
    let log = log_backend(dir, Arc::clone(&fault))?;
    if !after {
        fault.store(6, Ordering::SeqCst);
    }
    let s = f::scope()?;
    let _owners = purge_snapshot(
        log,
        state,
        s.group(),
        s.recovery(),
        caps()?,
        SnapshotLimits::default(),
    )?;
    if after {
        disk_backend::parked("after-return")?;
    }
    Err("child returned without reaching the requested purge boundary".into())
}

fn acknowledge_and_kill(child: &mut Child, after: bool) -> TestResult {
    let result = (|| -> TestResult {
        let stdout = child.stdout.take().ok_or("child stdout missing")?;
        let mut stdin = child.stdin.take().ok_or("child stdin missing")?;
        let (sender, receiver) = mpsc::sync_channel(2);
        let reader = disk_backend::signal_reader(stdout, sender, after)?;
        let acknowledged = (|| -> TestResult {
            receiver.recv_timeout(Duration::from_secs(20))??;
            stdin.write_all(b"A")?;
            stdin.flush()?;
            receiver.recv_timeout(Duration::from_secs(20))??;
            Ok(())
        })();
        let terminated = disk_backend::terminate(child);
        drop(stdin);
        let joined = reader.join();
        acknowledged?;
        assert!(!terminated?.success());
        joined.map_err(|_| "snapshot ACK reader panicked")??;
        Ok(())
    })();
    if result.is_err() {
        let _ = disk_backend::terminate(child);
    }
    result
}

fn killed(dir: &Path, after: bool) -> TestResult {
    let mut process = Command::new(std::env::current_exe()?)
        .args([
            "--exact",
            "faults::pin_before_purge_kill_boundaries",
            "--nocapture",
            "--test-threads=1",
        ])
        .env("WF_SNAPSHOT_PURGE_DIR", dir)
        .env("WF_SNAPSHOT_PURGE_AFTER", if after { "1" } else { "0" })
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()?;
    acknowledge_and_kill(&mut process, after)
}

#[test]
fn pin_before_purge_kill_boundaries() -> TestResult {
    if let Some(dir) = std::env::var_os("WF_SNAPSHOT_PURGE_DIR") {
        return child(
            Path::new(&dir),
            std::env::var("WF_SNAPSHOT_PURGE_AFTER")? == "1",
        );
    }
    for after in [false, true] {
        let dir = f::directory("pin-purge-kill")?;
        let (log, state, id) = fixtures::seeded(&dir)?;
        drop((log, state));
        let expected = pinned(fixtures::cohort(&dir)?)?;
        let old_meta = f::log_meta(&dir)?;
        let old_rows = f::log_rows(&dir)?;
        let new_meta = anchored(old_meta.clone(), id);
        let mut new_rows = old_rows.clone();
        assert!(new_rows.remove(&0).is_some());
        killed(&dir, after)?;
        // Even the inside-log before-sync kill happens AFTER state pin commit.
        assert_eq!(fixtures::cohort(&dir)?, expected);
        let meta = f::log_meta(&dir)?;
        let rows = f::log_rows(&dir)?;
        if after {
            assert_eq!(meta, new_meta);
            assert_eq!(rows, new_rows);
        } else {
            assert!(
                (meta == old_meta && rows == old_rows) || (meta == new_meta && rows == new_rows)
            );
        }
        reconcile(&dir, id, &expected)?;
        std::fs::remove_dir_all(dir)?;
    }
    Ok(())
}
