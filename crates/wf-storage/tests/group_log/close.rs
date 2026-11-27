//! Closing a log as its owner stops (ADR-018): appenders and waiters are
//! refused at once, even while the writer waits on its disk, and the
//! writer changes nothing more once the write in hand is done.

use std::sync::{Arc, mpsc};
use std::thread;
use std::time::Duration;

use wf_storage::log::{GroupLog, GroupLogError};

use super::gated::{Gate, LIMITS, Release, Sync, open_gated};
use super::{TestResult, directory};

/// Runs `work` on a thread of its own; its result arrives on the receiver.
fn started<T: Send + 'static>(work: impl FnOnce() -> T + Send + 'static) -> mpsc::Receiver<T> {
    let (sender, result) = mpsc::channel();
    thread::spawn(move || {
        let _gone = sender.send(work());
    });
    result
}

#[test]
fn a_closed_log_refuses_its_waiters_at_once_and_changes_nothing_more() -> TestResult {
    let directory = directory("close")?;
    let path = directory.join("log");
    let gate = Gate::new();
    let _release = Release(Arc::clone(&gate));
    gate.set(Sync::Pass);
    let log = open_gated(&path, &gate)?;
    let first = log.append(b"first".to_vec())?;
    log.wait_durable(first)?;
    assert_eq!(log.rotate()?, 1);
    let dropped = gate.dropped_now();
    gate.set(Sync::Held);
    let held = log.append(b"held".to_vec())?;
    assert!(gate.started(2), "the writer holds the second batch");
    let queued = log.append(b"queued".to_vec())?;
    let waits = [held, queued].map(|sequence| {
        let log = Arc::clone(&log);
        started(move || log.wait_durable(sequence))
    });
    let removal = {
        let log = Arc::clone(&log);
        started(move || log.remove_before(1))
    };
    thread::sleep(Duration::from_millis(50));
    log.close();
    let at_once = Duration::from_secs(2);
    for wait in waits {
        assert_eq!(wait.recv_timeout(at_once)?, Err(GroupLogError::Closed));
    }
    assert_eq!(removal.recv_timeout(at_once)?, Err(GroupLogError::Closed));
    assert_eq!(log.append(b"late".to_vec()), Err(GroupLogError::Closed));
    gate.set(Sync::Pass);
    assert!(
        gate.dropped(dropped + 1),
        "the writer stops once the write in hand is done"
    );
    drop(log);
    let (_log, recovered) = GroupLog::open(&path, LIMITS)?;
    assert_eq!(recovered.records, vec![b"first".to_vec(), b"held".to_vec()]);
    assert_eq!(recovered.segments, vec![0, 1]);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}
