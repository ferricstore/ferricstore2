//! Actual received-anchor I/O faults and acknowledged process-kill boundaries.

use super::super::{MIB, TestResult, create, directory, entries, id, open, raw_cohort};
use super::{ARTIFACT, boundary, digest, expected};
use std::path::Path;
use std::sync::{
    Arc,
    atomic::{AtomicU8, Ordering},
    mpsc,
};
use std::{
    io::Write,
    process::{Child, Command, Stdio},
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
fn received_commit_uncertainty_fences_and_reconciles_only_complete_old_or_new() -> TestResult {
    let dir = directory("received-uncertain")?;
    let specs = [
        (id(1, 7, 0), b"zero".as_slice()),
        (id(1, 7, 1), b"one"),
        (id(1, 7, 2), b"two"),
    ];
    for flavor in 0..3 {
        let path = dir.join(format!("log-{flavor}.redb"));
        let mut store = create(&path, RaftLogLimits::default())?;
        store.append(entries(&specs))?;
        store.save_committed(specs[0].0)?;
        store.save_vote(RaftVote::new(3, 8, true))?;
        let before = store.state()?;
        drop(store);
        let old = raw_cohort(&path)?;
        let target = match flavor {
            0 => specs[1].0,
            1 => id(2, 9, 1),
            _ => id(2, 9, 20),
        };
        let new = expected(
            if flavor == 0 { &specs[2..] } else { &[] },
            Some((3, 8, true)),
            target,
        )?;
        let fault = Arc::new(AtomicU8::new(0));
        let mut store = wrapped(&path, Arc::clone(&fault), 16 * MIB)?;
        fault.store(4, Ordering::SeqCst);
        assert!(matches!(
            store.install_snapshot_boundary(before, target, digest(b"one"), ARTIFACT),
            Err(RaftLogError::Uncertain)
        ));
        assert_eq!(
            fault.load(Ordering::SeqCst),
            0,
            "actual non-growth sync not reached"
        );
        assert!(matches!(store.state(), Err(RaftLogError::Fenced)));
        assert!(matches!(
            store.install_snapshot_boundary(before, target, digest(b"one"), ARTIFACT),
            Err(RaftLogError::Fenced)
        ));
        assert!(matches!(
            store.read_entry(2, 1024),
            Err(RaftLogError::Fenced)
        ));
        assert!(matches!(
            store.append(Vec::new()),
            Err(RaftLogError::Fenced)
        ));
        drop(store);
        let actual = raw_cohort(&path)?;
        assert!(actual == old || actual == new, "mixed C/P/ID/deleted rows");
        let mut store = open(&path, RaftLogLimits::default())?;
        store.install_snapshot_boundary(before, target, digest(b"one"), ARTIFACT)?;
        boundary(
            &mut store,
            target,
            if flavor == 0 { specs[2].0 } else { target },
            before.vote(),
        )?;
        drop(store);
        assert_eq!(raw_cohort(&path)?, new);
    }
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn uncached_received_read_failure_fences_before_any_boundary_publication() -> TestResult {
    let dir = directory("received-read-failure")?;
    let path = dir.join("log.redb");
    let mut store = create(&path, RaftLogLimits::default())?;
    store.append(entries(&[(id(1, 7, 0), b"zero"), (id(1, 7, 1), b"one")]))?;
    store.save_committed(id(1, 7, 0))?;
    let before = store.state()?;
    drop(store);
    let old = raw_cohort(&path)?;
    let fault = Arc::new(AtomicU8::new(0));
    let mut store = wrapped(&path, Arc::clone(&fault), 0)?;
    fault.store(3, Ordering::SeqCst);
    assert!(matches!(
        store.install_snapshot_boundary(before, id(1, 7, 1), digest(b"one"), ARTIFACT),
        Err(RaftLogError::Unavailable)
    ));
    assert_eq!(
        fault.load(Ordering::SeqCst),
        0,
        "actual read injection not reached"
    );
    assert!(matches!(store.state(), Err(RaftLogError::Fenced)));
    assert!(matches!(
        store.install_snapshot_boundary(before, id(1, 7, 1), digest(b"one"), ARTIFACT),
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
        let stdout = child.stdout.take().ok_or("received child stdout absent")?;
        let mut stdin = child.stdin.take().ok_or("received child stdin absent")?;
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
        joined.map_err(|_| "received ACK reader panicked")??;
        Ok(())
    })();
    result.map_err(|error| match crate::disk_backend::terminate(child) {
        Ok(_) => error,
        Err(cleanup) => format!("{error}; child cleanup failed: {cleanup}").into(),
    })
}

#[test]
fn received_process_kills_preserve_one_complete_anchor_commit_and_physical_cohort() -> TestResult {
    if let Some(path) = std::env::var_os("WF_RECEIVED_LOG_CHILD") {
        let after = std::env::var("WF_RECEIVED_LOG_AFTER")? == "1";
        let flavor: u8 = std::env::var("WF_RECEIVED_LOG_FLAVOR")?.parse()?;
        let fault = Arc::new(AtomicU8::new(0));
        let mut store = wrapped(Path::new(&path), Arc::clone(&fault), 16 * MIB)?;
        let before = store.state()?;
        let target = match flavor {
            0 => id(1, 7, 1),
            1 => id(2, 9, 1),
            _ => id(2, 9, 20),
        };
        if !after {
            fault.store(6, Ordering::SeqCst);
        }
        store.install_snapshot_boundary(before, target, digest(b"one"), ARTIFACT)?;
        if after {
            crate::disk_backend::parked("after-return")?;
        }
        return Err("child did not reach requested received boundary".into());
    }
    let dir = directory("received-process-kill")?;
    let specs = [
        (id(1, 7, 0), b"zero".as_slice()),
        (id(1, 7, 1), b"one"),
        (id(1, 7, 2), b"two"),
    ];
    for flavor in 0..3 {
        let target = match flavor {
            0 => specs[1].0,
            1 => id(2, 9, 1),
            _ => id(2, 9, 20),
        };
        let new = expected(if flavor == 0 { &specs[2..] } else { &[] }, None, target)?;
        for after in [false, true] {
            let path = dir.join(format!("log-{flavor}-{after}.redb"));
            let mut store = create(&path, RaftLogLimits::default())?;
            store.append(entries(&specs))?;
            store.save_committed(specs[0].0)?;
            let before = store.state()?;
            drop(store);
            let old = raw_cohort(&path)?;
            let mut child = Command::new(std::env::current_exe()?)
                .args(["--exact", "received::faults::received_process_kills_preserve_one_complete_anchor_commit_and_physical_cohort",
                    "--nocapture", "--test-threads=1"])
                .env("WF_RECEIVED_LOG_CHILD", &path)
                .env("WF_RECEIVED_LOG_AFTER", if after { "1" } else { "0" })
                .env("WF_RECEIVED_LOG_FLAVOR", flavor.to_string())
                .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::inherit()).spawn()?;
            kill_ack(&mut child, after)?;
            let actual = raw_cohort(&path)?;
            if after {
                assert_eq!(actual, new);
            } else {
                assert!(actual == old || actual == new);
            }
            let mut store = open(&path, RaftLogLimits::default())?;
            store.install_snapshot_boundary(before, target, digest(b"one"), ARTIFACT)?;
            drop(store);
            assert_eq!(raw_cohort(&path)?, new);
        }
    }
    std::fs::remove_dir_all(dir)?;
    Ok(())
}
