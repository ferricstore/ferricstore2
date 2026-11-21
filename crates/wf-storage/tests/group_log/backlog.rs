//! A bounded write queue (ADR-017): an appender waits while the records
//! queued for the writer hold `max_queued_bytes` or more, continues once the
//! writer takes them, and is refused when the writer fails meanwhile.

use std::io;
use std::path::Path;
use std::sync::mpsc;
use std::sync::{Arc, Condvar, Mutex, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use wf_storage::log::{
    DirectoryStorage, GroupLog, GroupLogError, GroupLogLimits, SegmentStorage, SegmentWriter,
};

use super::{TestResult, directory};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Sync {
    Held,
    Pass,
    Fail,
}

/// Holds every sync until the test lets them pass or fail.
struct Gate {
    state: Mutex<(Sync, usize)>,
    changed: Condvar,
}

impl Gate {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            state: Mutex::new((Sync::Held, 0)),
            changed: Condvar::new(),
        })
    }

    fn set(&self, sync: Sync) {
        self.state.lock().unwrap_or_else(PoisonError::into_inner).0 = sync;
        self.changed.notify_all();
    }

    /// Waits until `count` syncs have started.
    fn started(&self, count: usize) -> bool {
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        while state.1 < count {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return false;
            }
            state = self
                .changed
                .wait_timeout(state, left)
                .unwrap_or_else(PoisonError::into_inner)
                .0;
        }
        true
    }

    fn sync(&self) -> io::Result<()> {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        state.1 = state.1.saturating_add(1);
        self.changed.notify_all();
        while state.0 == Sync::Held {
            state = self
                .changed
                .wait(state)
                .unwrap_or_else(PoisonError::into_inner);
        }
        match state.0 {
            Sync::Fail => Err(io::Error::other("injected sync failure")),
            _ => Ok(()),
        }
    }
}

/// Lets held syncs pass when dropped, so a failed assertion does not leave
/// the log's drop waiting on its writer.
struct Release(Arc<Gate>);

impl Drop for Release {
    fn drop(&mut self) {
        let mut state = self.0.state.lock().unwrap_or_else(PoisonError::into_inner);
        if state.0 == Sync::Held {
            state.0 = Sync::Pass;
        }
        drop(state);
        self.0.changed.notify_all();
    }
}

struct GatedStorage {
    inner: DirectoryStorage,
    gate: Arc<Gate>,
}

struct GatedWriter {
    inner: Box<dyn SegmentWriter>,
    gate: Arc<Gate>,
}

impl SegmentStorage for GatedStorage {
    fn segments(&self) -> io::Result<Vec<u64>> {
        self.inner.segments()
    }
    fn read(&self, segment: u64) -> io::Result<Vec<u8>> {
        self.inner.read(segment)
    }
    fn create(&self, segment: u64) -> io::Result<Box<dyn SegmentWriter>> {
        Ok(Box::new(GatedWriter {
            inner: self.inner.create(segment)?,
            gate: Arc::clone(&self.gate),
        }))
    }
    fn reopen(&self, segment: u64) -> io::Result<Box<dyn SegmentWriter>> {
        Ok(Box::new(GatedWriter {
            inner: self.inner.reopen(segment)?,
            gate: Arc::clone(&self.gate),
        }))
    }
    fn truncate(&self, segment: u64, length: u64) -> io::Result<()> {
        self.inner.truncate(segment, length)
    }
    fn remove(&self, segment: u64) -> io::Result<()> {
        self.inner.remove(segment)
    }
}

impl SegmentWriter for GatedWriter {
    fn append(&mut self, bytes: &[u8]) -> io::Result<()> {
        self.inner.append(bytes)
    }
    fn sync(&mut self) -> io::Result<()> {
        self.gate.sync()?;
        self.inner.sync()
    }
}

const LIMITS: GroupLogLimits = GroupLogLimits {
    max_record_bytes: 4_096,
    max_block_bytes: 64 * 1024,
    max_segment_bytes: 1024 * 1024,
    max_queued_bytes: 1_000,
};

fn open_gated(path: &Path, gate: &Arc<Gate>) -> TestResult<Arc<GroupLog>> {
    let storage = GatedStorage {
        inner: DirectoryStorage::open(path)?,
        gate: Arc::clone(gate),
    };
    Ok(Arc::new(GroupLog::open_with(Arc::new(storage), LIMITS)?.0))
}

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
