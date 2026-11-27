//! Segment storage whose syncs the test holds, lets pass or fails, and
//! which reports when the writer let go of a segment.

use std::io;
use std::path::Path;
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use wf_storage::log::{DirectoryStorage, GroupLog, GroupLogLimits, SegmentStorage, SegmentWriter};

use super::TestResult;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Sync {
    Held,
    Pass,
    Fail,
}

struct State {
    sync: Sync,
    /// Syncs started.
    started: usize,
    /// Segment writers dropped.
    dropped: usize,
}

/// Holds every sync until the test lets them pass or fail.
pub(super) struct Gate {
    state: Mutex<State>,
    changed: Condvar,
}

impl Gate {
    pub(super) fn new() -> Arc<Self> {
        Arc::new(Self {
            state: Mutex::new(State {
                sync: Sync::Held,
                started: 0,
                dropped: 0,
            }),
            changed: Condvar::new(),
        })
    }

    fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub(super) fn set(&self, sync: Sync) {
        self.state().sync = sync;
        self.changed.notify_all();
    }

    /// Waits up to five seconds until `count(state)` reaches `at_least`.
    fn reaches(&self, count: fn(&State) -> usize, at_least: usize) -> bool {
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut state = self.state();
        while count(&state) < at_least {
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

    /// Waits until `count` syncs have started.
    pub(super) fn started(&self, count: usize) -> bool {
        self.reaches(|state| state.started, count)
    }

    /// Segment writers dropped so far.
    pub(super) fn dropped_now(&self) -> usize {
        self.state().dropped
    }

    /// Waits until `count` segment writers were dropped.
    pub(super) fn dropped(&self, count: usize) -> bool {
        self.reaches(|state| state.dropped, count)
    }

    fn sync(&self) -> io::Result<()> {
        let mut state = self.state();
        state.started = state.started.saturating_add(1);
        self.changed.notify_all();
        while state.sync == Sync::Held {
            state = self
                .changed
                .wait(state)
                .unwrap_or_else(PoisonError::into_inner);
        }
        match state.sync {
            Sync::Fail => Err(io::Error::other("injected sync failure")),
            _ => Ok(()),
        }
    }
}

/// Lets held syncs pass when dropped, so a failed assertion does not leave
/// the log's drop waiting on its writer.
pub(super) struct Release(pub(super) Arc<Gate>);

impl Drop for Release {
    fn drop(&mut self) {
        let mut state = self.0.state();
        if state.sync == Sync::Held {
            state.sync = Sync::Pass;
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

impl Drop for GatedWriter {
    fn drop(&mut self) {
        let mut state = self.gate.state();
        state.dropped = state.dropped.saturating_add(1);
        drop(state);
        self.gate.changed.notify_all();
    }
}

pub(super) const LIMITS: GroupLogLimits = GroupLogLimits {
    max_record_bytes: 4_096,
    max_block_bytes: 64 * 1024,
    max_segment_bytes: 1024 * 1024,
    max_queued_bytes: 1_000,
};

pub(super) fn open_gated(path: &Path, gate: &Arc<Gate>) -> TestResult<Arc<GroupLog>> {
    let storage = GatedStorage {
        inner: DirectoryStorage::open(path)?,
        gate: Arc::clone(gate),
    };
    Ok(Arc::new(GroupLog::open_with(Arc::new(storage), LIMITS)?.0))
}
