//! Actual disk failure on the atomic prefix-delete/anchor transaction.

use super::super::{MIB, TestResult, create, directory, entries, id, open, raw_cohort};
use super::{SNAPSHOT, anchored, assert_anchor};
use std::path::Path;
use std::sync::{
    Arc,
    atomic::{AtomicU8, Ordering},
};
use std::{
    io::Write,
    process::{Child, Command, Stdio},
    sync::mpsc,
    time::Duration,
};
use wf_ports::log::{RaftLogError, RaftLogLimits, RaftLogStore, RaftVote};
use wf_storage::log::RedbLogStore;
use wf_types::identity::{GroupId, RecoveryId};

fn wrapped(path: &Path, fault: Arc<AtomicU8>, cache: usize) -> TestResult<RedbLogStore> {
    let database = crate::disk_backend::fault_database(path, fault, cache)?;
    Ok(RedbLogStore::open_database(
        database,
        GroupId::new(b"g".to_vec())?,
        RecoveryId::new(b"z".to_vec())?,
        RaftLogLimits::default(),
    )?)
}

#[test]
fn uncertain_purge_fences_all_operations_and_reopens_only_whole_old_or_new() -> TestResult {
    let dir = directory("purge-uncertain")?;
    let path = dir.join("log.redb");
    let specs = [
        (id(1, 7, 0), b"zero".as_slice()),
        (id(1, 7, 1), b"one"),
        (id(1, 7, 2), b"two"),
    ];
    let mut store = create(&path, RaftLogLimits::default())?;
    store.append(entries(&specs))?;
    store.save_committed(id(1, 7, 2))?;
    drop(store);
    let old = raw_cohort(&path)?;
    let new = anchored(&specs[2..], None, id(1, 7, 2), id(1, 7, 1))?;
    let fault = Arc::new(AtomicU8::new(0));
    let mut store = wrapped(&path, Arc::clone(&fault), 16 * MIB)?;
    fault.store(4, Ordering::SeqCst);
    assert!(matches!(
        store.purge_through(id(1, 7, 1), SNAPSHOT),
        Err(RaftLogError::Uncertain)
    ));
    assert_eq!(
        fault.load(Ordering::SeqCst),
        0,
        "must reach real non-growth commit sync"
    );
    assert!(matches!(store.state(), Err(RaftLogError::Fenced)));
    assert!(matches!(store.read_entry(2, 0), Err(RaftLogError::Fenced)));
    assert!(matches!(
        store.purge_through(id(1, 7, 1), SNAPSHOT),
        Err(RaftLogError::Fenced)
    ));
    assert!(matches!(
        store.append(Vec::new()),
        Err(RaftLogError::Fenced)
    ));
    assert!(matches!(store.truncate_from(3), Err(RaftLogError::Fenced)));
    assert!(matches!(
        store.save_committed(id(1, 7, 2)),
        Err(RaftLogError::Fenced)
    ));
    assert!(matches!(
        store.save_vote(RaftVote::new(3, 0, true)),
        Err(RaftLogError::Fenced)
    ));
    drop(store);
    let actual = raw_cohort(&path)?;
    assert!(
        actual == old || actual == new,
        "partial prefix/anchor publication"
    );
    let mut store = open(&path, RaftLogLimits::default())?;
    store.purge_through(id(1, 7, 1), SNAPSHOT)?;
    assert_anchor(&mut store, id(1, 7, 1))?;
    drop(store);
    assert_eq!(raw_cohort(&path)?, new);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn uncached_purge_read_error_fences_before_any_deletion() -> TestResult {
    let dir = directory("purge-read-failure")?;
    let path = dir.join("log.redb");
    let mut store = create(&path, RaftLogLimits::default())?;
    store.append(entries(&[(id(1, 7, 0), b"zero"), (id(1, 7, 1), b"one")]))?;
    store.save_committed(id(1, 7, 1))?;
    drop(store);
    let old = raw_cohort(&path)?;
    let fault = Arc::new(AtomicU8::new(0));
    let mut store = wrapped(&path, Arc::clone(&fault), 0)?;
    fault.store(3, Ordering::SeqCst);
    assert!(matches!(
        store.purge_through(id(1, 7, 0), SNAPSHOT),
        Err(RaftLogError::Unavailable)
    ));
    assert_eq!(
        fault.load(Ordering::SeqCst),
        0,
        "real read injection was not reached"
    );
    assert!(matches!(
        store.purge_through(id(1, 7, 0), SNAPSHOT),
        Err(RaftLogError::Fenced)
    ));
    drop(store);
    assert_eq!(raw_cohort(&path)?, old);
    drop(open(&path, RaftLogLimits::default())?);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

fn kill_ack(child: &mut Child, after: bool) -> TestResult {
    let result = (|| -> TestResult {
        let stdout = child.stdout.take().ok_or("purge child stdout missing")?;
        let mut stdin = child.stdin.take().ok_or("purge child stdin missing")?;
        let (sender, receiver) = mpsc::sync_channel(2);
        let reader = crate::disk_backend::signal_reader(stdout, sender, after)?;
        let acknowledged = (|| -> TestResult {
            receiver.recv_timeout(Duration::from_secs(20))??;
            stdin.write_all(b"A")?;
            stdin.flush()?;
            receiver.recv_timeout(Duration::from_secs(20))??;
            Ok(())
        })();
        let terminated = crate::disk_backend::terminate(child);
        drop(stdin);
        let joined = reader.join();
        acknowledged?;
        assert!(!terminated?.success());
        joined.map_err(|_| "purge ACK reader panicked")??;
        Ok(())
    })();
    result.map_err(|error| match crate::disk_backend::terminate(child) {
        Ok(_) => error,
        Err(cleanup) => format!("{error}; child cleanup failed: {cleanup}").into(),
    })
}

#[test]
fn purge_process_kills_keep_anchor_and_deleted_rows_in_one_complete_cohort() -> TestResult {
    if let Some(path) = std::env::var_os("WF_PURGE_LOG_CHILD") {
        let after = std::env::var("WF_PURGE_LOG_AFTER")? == "1";
        let fault = Arc::new(AtomicU8::new(0));
        let mut store = wrapped(Path::new(&path), Arc::clone(&fault), 16 * MIB)?;
        if !after {
            fault.store(6, Ordering::SeqCst);
        }
        store.purge_through(id(1, 7, 1), SNAPSHOT)?;
        if after {
            crate::disk_backend::parked("after-return")?;
        }
        return Err("child did not park at the requested purge boundary".into());
    }
    let dir = directory("purge-process-kill")?;
    let specs = [
        (id(1, 7, 0), b"zero".as_slice()),
        (id(1, 7, 1), b"one"),
        (id(1, 7, 2), b"two"),
    ];
    let new = anchored(&specs[2..], None, id(1, 7, 2), id(1, 7, 1))?;
    for after in [false, true] {
        let path = dir.join(format!("log-{after}.redb"));
        let mut store = create(&path, RaftLogLimits::default())?;
        store.append(entries(&specs))?;
        store.save_committed(id(1, 7, 2))?;
        drop(store);
        let old = raw_cohort(&path)?;
        let mut child=Command::new(std::env::current_exe()?)
            .args(["--exact","purge::faults::purge_process_kills_keep_anchor_and_deleted_rows_in_one_complete_cohort",
                "--nocapture","--test-threads=1"])
            .env("WF_PURGE_LOG_CHILD",&path).env("WF_PURGE_LOG_AFTER",if after {"1"}else{"0"})
            .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::inherit()).spawn()?;
        kill_ack(&mut child, after)?;
        let actual = raw_cohort(&path)?;
        if after {
            assert_eq!(actual, new);
        } else {
            assert!(actual == old || actual == new);
        }
        let mut store = open(&path, RaftLogLimits::default())?;
        store.purge_through(id(1, 7, 1), SNAPSHOT)?;
        assert_anchor(&mut store, id(1, 7, 1))?;
        drop(store);
        assert_eq!(raw_cohort(&path)?, new);
    }
    std::fs::remove_dir_all(dir)?;
    Ok(())
}
