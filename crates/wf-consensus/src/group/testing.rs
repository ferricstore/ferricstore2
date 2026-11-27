//! What the group's in-crate tests share: a runtime, a fresh path for a
//! test's stores, and a machine whose snapshots hold `captured`.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use wf_ports::consensus::log::{
    Applied, Capture, Captured, ConsensusLogError, ReplicatedMachine, SnapshotSource,
};

pub(super) type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

pub(super) fn runtime() -> TestResult<tokio::runtime::Runtime> {
    Ok(tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()?)
}

/// Paths handed out in this process: tests run as threads of one process,
/// and the clock alone repeats between them (it counts microseconds here).
static NEXT: AtomicU64 = AtomicU64::new(0);

/// A fresh path for one test's directories; the stores create them.
pub(super) struct Root(PathBuf);

impl Root {
    pub(super) fn new() -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |since| since.as_nanos());
        let next = NEXT.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "wf-group-tests-{}-{nanos}-{next}",
            std::process::id()
        ));
        Self(path)
    }

    pub(super) fn path(&self) -> &Path {
        &self.0
    }
}

/// A machine whose snapshots hold `captured`.
pub(super) struct Machine;

struct Written;

impl Capture for Written {
    fn write_to(self: Box<Self>, out: &mut dyn Write) -> Result<(), ConsensusLogError> {
        out.write_all(b"captured")
            .map_err(|error| ConsensusLogError::new(error.to_string()))
    }
}

impl ReplicatedMachine for Machine {
    fn apply(&self, _proposals: Vec<Arc<[u8]>>, done: Applied) {
        done(Ok(()));
    }

    fn promote(&self, done: Applied) {
        done(Ok(()));
    }

    fn demote(&self) {}

    fn snapshot(&self, done: Captured) -> bool {
        done(Ok(Box::new(Written)));
        true
    }

    fn install(&self, _snapshot: Box<dyn SnapshotSource>, done: Applied) {
        done(Ok(()));
    }
}
