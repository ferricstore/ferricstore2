//! Injected storage failures. A failed write, sync or segment creation fences
//! the log: the affected and later appends fail, earlier acknowledgements
//! stand, and a reopen recovers every acknowledged record. A failed truncation
//! refuses to open; a failed removal is reported and the log stays usable.

use std::io;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use wf_storage::log::{
    DirectoryStorage, GroupLog, GroupLogError, GroupLogLimits, SegmentStorage, SegmentWriter,
};

use super::{TestResult, directory, record, segment};

/// Countdowns: an operation fails on the call that takes its counter from 1
/// to 0; 0 means never.
#[derive(Default)]
struct Faults {
    write: AtomicU64,
    sync: AtomicU64,
    create: AtomicU64,
    truncate: AtomicU64,
    remove: AtomicU64,
}

fn trip(counter: &AtomicU64) -> bool {
    counter
        .try_update(Ordering::SeqCst, Ordering::SeqCst, |left| {
            left.checked_sub(1)
        })
        .is_ok_and(|before| before == 1)
}

fn injected(operation: &str) -> io::Error {
    io::Error::other(format!("injected {operation} failure"))
}

struct FaultyStorage {
    inner: DirectoryStorage,
    faults: Arc<Faults>,
}

struct FaultyWriter {
    inner: Box<dyn SegmentWriter>,
    faults: Arc<Faults>,
}

impl SegmentStorage for FaultyStorage {
    fn segments(&self) -> io::Result<Vec<u64>> {
        self.inner.segments()
    }
    fn read(&self, segment: u64) -> io::Result<Vec<u8>> {
        self.inner.read(segment)
    }
    fn create(&self, segment: u64) -> io::Result<Box<dyn SegmentWriter>> {
        if trip(&self.faults.create) {
            return Err(injected("create"));
        }
        let inner = self.inner.create(segment)?;
        Ok(Box::new(FaultyWriter {
            inner,
            faults: Arc::clone(&self.faults),
        }))
    }
    fn reopen(&self, segment: u64) -> io::Result<Box<dyn SegmentWriter>> {
        let inner = self.inner.reopen(segment)?;
        Ok(Box::new(FaultyWriter {
            inner,
            faults: Arc::clone(&self.faults),
        }))
    }
    fn truncate(&self, segment: u64, length: u64) -> io::Result<()> {
        if trip(&self.faults.truncate) {
            return Err(injected("truncate"));
        }
        self.inner.truncate(segment, length)
    }
    fn remove(&self, segment: u64) -> io::Result<()> {
        if trip(&self.faults.remove) {
            return Err(injected("remove"));
        }
        self.inner.remove(segment)
    }
}

impl SegmentWriter for FaultyWriter {
    fn append(&mut self, bytes: &[u8]) -> io::Result<()> {
        if trip(&self.faults.write) {
            // A torn write: half the bytes reach the file, then the call fails.
            self.inner
                .append(bytes.get(..bytes.len() / 2).unwrap_or_default())?;
            return Err(injected("write"));
        }
        self.inner.append(bytes)
    }
    fn sync(&mut self) -> io::Result<()> {
        if trip(&self.faults.sync) {
            return Err(injected("sync"));
        }
        self.inner.sync()
    }
}

fn open_faulty(path: &Path, faults: &Arc<Faults>, limits: GroupLogLimits) -> TestResult<GroupLog> {
    let storage = FaultyStorage {
        inner: DirectoryStorage::open(path)?,
        faults: Arc::clone(faults),
    };
    Ok(GroupLog::open_with(Arc::new(storage), limits)?.0)
}

fn append_waited(log: &GroupLog, index: usize) -> Result<u64, GroupLogError> {
    let sequence = log.append(record(index))?;
    log.wait_durable(sequence)?;
    Ok(sequence)
}

#[test]
fn a_failed_sync_fences_the_log_and_keeps_acknowledged_records() -> TestResult {
    let directory = directory("sync")?;
    let path = directory.join("log");
    let faults = Arc::new(Faults::default());
    faults.sync.store(3, Ordering::SeqCst);
    {
        let log = open_faulty(&path, &faults, GroupLogLimits::default())?;
        let first = append_waited(&log, 0)?;
        append_waited(&log, 1)?;
        assert_eq!(append_waited(&log, 2), Err(GroupLogError::Uncertain));
        assert_eq!(log.append(record(3)), Err(GroupLogError::Uncertain));
        assert_eq!(log.rotate(), Err(GroupLogError::Uncertain));
        log.wait_durable(first)?;
    }
    let (log, recovered) = GroupLog::open(&path, GroupLogLimits::default())?;
    assert_eq!(
        recovered.records.get(..2),
        Some([record(0), record(1)].as_slice())
    );
    assert!(
        recovered.records.len() <= 3,
        "{} records",
        recovered.records.len()
    );
    append_waited(&log, 4)?;
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

#[test]
fn a_torn_write_fences_and_reopen_cuts_the_partial_block() -> TestResult {
    let directory = directory("write")?;
    let path = directory.join("log");
    let faults = Arc::new(Faults::default());
    faults.write.store(2, Ordering::SeqCst);
    {
        let log = open_faulty(&path, &faults, GroupLogLimits::default())?;
        append_waited(&log, 0)?;
        assert_eq!(append_waited(&log, 1), Err(GroupLogError::Uncertain));
        assert_eq!(log.append(record(2)), Err(GroupLogError::Uncertain));
    }
    let (log, recovered) = GroupLog::open(&path, GroupLogLimits::default())?;
    assert_eq!(recovered.records, vec![record(0)]);
    assert!(recovered.truncated_bytes > 0, "the half block was not cut");
    append_waited(&log, 3)?;
    drop(log);
    let (_log, reopened) = GroupLog::open(&path, GroupLogLimits::default())?;
    assert_eq!(reopened.records, vec![record(0), record(3)]);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

#[test]
fn a_failed_segment_creation_fences_size_and_explicit_rotation() -> TestResult {
    let directory = directory("create")?;
    let path = directory.join("log");
    let faults = Arc::new(Faults::default());
    // Smaller than two one-record blocks, so the second append rotates.
    let limits = GroupLogLimits {
        max_segment_bytes: 60,
        ..GroupLogLimits::default()
    };
    // The first create makes segment 0; the second is the first rotation.
    faults.create.store(2, Ordering::SeqCst);
    {
        let log = open_faulty(&path, &faults, limits)?;
        append_waited(&log, 0)?;
        assert_eq!(append_waited(&log, 1), Err(GroupLogError::Uncertain));
    }
    // Segment 0 exists now, so the next create is the explicit rotation.
    faults.create.store(1, Ordering::SeqCst);
    {
        let log = open_faulty(&path, &faults, GroupLogLimits::default())?;
        assert_eq!(log.rotate(), Err(GroupLogError::Uncertain));
        assert_eq!(log.append(record(2)), Err(GroupLogError::Uncertain));
    }
    let (_log, recovered) = GroupLog::open(&path, GroupLogLimits::default())?;
    assert_eq!(recovered.records, vec![record(0)]);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

#[test]
fn a_failed_tail_truncation_refuses_to_open_and_changes_nothing() -> TestResult {
    let directory = directory("truncate")?;
    let path = directory.join("log");
    {
        let (log, _) = GroupLog::open(&path, GroupLogLimits::default())?;
        append_waited(&log, 0)?;
    }
    let file = segment(&path, 0)?;
    let mut bytes = std::fs::read(&file)?;
    let synced = bytes.len();
    bytes.extend_from_slice(&[0x5A; 40]);
    std::fs::write(&file, &bytes)?;
    let faults = Arc::new(Faults::default());
    faults.truncate.store(1, Ordering::SeqCst);
    let error = open_faulty(&path, &faults, GroupLogLimits::default())
        .err()
        .ok_or("opened despite a failed truncation")?;
    assert_eq!(
        error.downcast_ref::<GroupLogError>(),
        Some(&GroupLogError::Uncertain)
    );
    assert_eq!(std::fs::read(&file)?.len(), bytes.len());
    let (_log, recovered) = GroupLog::open(&path, GroupLogLimits::default())?;
    assert_eq!(recovered.records, vec![record(0)]);
    assert_eq!(std::fs::read(&file)?.len(), synced);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

#[test]
fn a_failed_removal_is_reported_and_the_log_stays_usable() -> TestResult {
    let directory = directory("remove")?;
    let path = directory.join("log");
    let faults = Arc::new(Faults::default());
    // Segment 0 is removed; removing segment 1 fails.
    faults.remove.store(2, Ordering::SeqCst);
    {
        let log = open_faulty(&path, &faults, GroupLogLimits::default())?;
        append_waited(&log, 0)?;
        log.rotate()?;
        append_waited(&log, 1)?;
        let checkpoint = log.rotate()?;
        assert_eq!(
            log.remove_before(checkpoint),
            Err(GroupLogError::Unavailable)
        );
        append_waited(&log, 2)?;
        log.remove_before(checkpoint)?;
        append_waited(&log, 3)?;
    }
    let (_log, recovered) = GroupLog::open(&path, GroupLogLimits::default())?;
    assert_eq!(recovered.records, vec![record(2), record(3)]);
    assert_eq!(recovered.segments, vec![2]);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}
