//! A bounded write queue (ADR-017): an appender waits while the records
//! queued for the writer hold `max_queued_bytes` or more, continues once the
//! writer takes them, and is refused when the writer fails meanwhile.

use std::sync::Arc;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use wf_storage::log::{GroupLog, GroupLogError};

use super::gated::{Gate, LIMITS, Release, Sync, open_gated};
use super::{TestResult, directory};

fn bytes(fill: u8, length: usize) -> Vec<u8> {
    vec![fill; length]
}

/// Fills the queue: one batch held in the writer's sync and one queued past
/// the limit; then starts an appender that must wait.
fn fill(
    log: &Arc<GroupLog>,
    gate: &Gate,
) -> TestResult<mpsc::Receiver<Result<u64, GroupLogError>>> {
    log.append(bytes(b'a', 100))?;
    assert!(gate.started(1), "the writer holds the first batch");
    log.append(bytes(b'b', 2_000))?;
    let (sender, appended) = mpsc::channel();
    let waiting = Arc::clone(log);
    thread::spawn(move || {
        let _gone = sender.send(waiting.append(bytes(b'c', 10)));
    });
    assert!(
        appended.recv_timeout(Duration::from_millis(200)).is_err(),
        "the third append waits while the queue is over its limit"
    );
    Ok(appended)
}

#[test]
fn an_append_waits_while_the_queue_is_full_and_continues_once_it_drains() -> TestResult {
    let directory = directory("backlog")?;
    let path = directory.join("log");
    let gate = Gate::new();
    {
        let log = open_gated(&path, &gate)?;
        let _release = Release(Arc::clone(&gate));
        let appended = fill(&log, &gate)?;
        gate.set(Sync::Pass);
        let sequence = appended.recv_timeout(Duration::from_secs(5))??;
        log.wait_durable(sequence)?;
    }
    let (_log, recovered) = GroupLog::open(&path, LIMITS)?;
    assert_eq!(
        recovered.records,
        vec![bytes(b'a', 100), bytes(b'b', 2_000), bytes(b'c', 10)]
    );
    Ok(())
}

#[test]
fn a_writer_failure_refuses_a_waiting_append() -> TestResult {
    let directory = directory("backlog-fail")?;
    let path = directory.join("log");
    let gate = Gate::new();
    let log = open_gated(&path, &gate)?;
    let _release = Release(Arc::clone(&gate));
    let appended = fill(&log, &gate)?;
    gate.set(Sync::Fail);
    assert_eq!(
        appended.recv_timeout(Duration::from_secs(5))?,
        Err(GroupLogError::Uncertain)
    );
    Ok(())
}
