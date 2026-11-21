//! The node-local group-committed log (ADR-011): appends from many threads
//! share one write and one sync, acknowledgements follow the sync, recovery
//! keeps every complete block while cutting only a torn tail, segments rotate
//! and are removed after a checkpoint, and injected write, sync, create,
//! truncate and remove failures fence or report without losing acknowledged
//! records.

#![forbid(unsafe_code)]

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use wf_storage::log::{DirectoryStorage, GroupLog, GroupLogLimits};

mod backlog;
mod concurrency;
mod crash;
mod faults;
mod recovery;
mod rotation;

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

static NEXT_PATH: AtomicU64 = AtomicU64::new(0);

/// A fresh directory; removed by the test that made it.
fn directory(tag: &str) -> TestResult<PathBuf> {
    let suffix = NEXT_PATH.fetch_add(1, Ordering::SeqCst);
    let path = std::env::temp_dir().join(format!(
        "wf-group-log-{}-{suffix}-{tag}",
        std::process::id()
    ));
    std::fs::create_dir(&path)?;
    Ok(path)
}

/// The file holding `segment` of the log at `path`.
fn segment(path: &Path, segment: u64) -> TestResult<PathBuf> {
    Ok(DirectoryStorage::open(path)?.segment_path(segment))
}

fn record(index: usize) -> Vec<u8> {
    format!("record-{index:06}").into_bytes()
}

#[test]
fn appended_records_are_durable_and_replayed_in_order() -> TestResult {
    let directory = directory("order")?;
    let path = directory.join("log");
    {
        let (log, recovered) = GroupLog::open(&path, GroupLogLimits::default())?;
        assert_eq!(recovered.records, Vec::<Vec<u8>>::new());
        let mut last = 0;
        for index in 0..10 {
            last = log.append(record(index))?;
        }
        log.wait_durable(last)?;
    }
    let (log, recovered) = GroupLog::open(&path, GroupLogLimits::default())?;
    assert_eq!(recovered.records, (0..10).map(record).collect::<Vec<_>>());
    assert_eq!(recovered.truncated_bytes, 0);
    let next = log.append(record(10))?;
    log.wait_durable(next)?;
    drop(log);
    let (_log, recovered) = GroupLog::open(&path, GroupLogLimits::default())?;
    assert_eq!(recovered.records, (0..11).map(record).collect::<Vec<_>>());
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

#[test]
fn records_above_the_limit_are_refused_without_writing() -> TestResult {
    let directory = directory("limit")?;
    let path = directory.join("log");
    let limits = GroupLogLimits {
        max_record_bytes: 8,
        ..GroupLogLimits::default()
    };
    {
        let (log, _) = GroupLog::open(&path, limits)?;
        assert!(log.append(vec![0; 9]).is_err());
        let accepted = log.append(vec![7; 8])?;
        log.wait_durable(accepted)?;
    }
    let (_log, recovered) = GroupLog::open(&path, limits)?;
    assert_eq!(recovered.records, vec![vec![7; 8]]);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

#[test]
fn records_still_queued_at_drop_are_written_before_the_writer_stops() -> TestResult {
    let directory = directory("drain")?;
    let path = directory.join("log");
    {
        let (log, _) = GroupLog::open(&path, GroupLogLimits::default())?;
        for index in 0..100 {
            log.append(record(index))?;
        }
    }
    let (_log, recovered) = GroupLog::open(&path, GroupLogLimits::default())?;
    assert_eq!(recovered.records, (0..100).map(record).collect::<Vec<_>>());
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

#[test]
fn a_batch_is_replayed_whole_or_not_at_all() -> TestResult {
    let directory = directory("batch")?;
    let path = directory.join("log");
    {
        let (log, _) = GroupLog::open(&path, GroupLogLimits::default())?;
        let sequence = log.append_batch((0..5).map(record).collect())?;
        log.wait_durable(sequence)?;
        assert!(
            log.append_batch(Vec::new()).is_err(),
            "an empty batch was accepted"
        );
    }
    let file = segment(&path, 0)?;
    let full = std::fs::metadata(&file)?.len();
    std::fs::OpenOptions::new()
        .write(true)
        .open(&file)?
        .set_len(full - 1)?;
    let (_log, recovered) = GroupLog::open(&path, GroupLogLimits::default())?;
    assert_eq!(recovered.records, Vec::<Vec<u8>>::new());
    assert_eq!(recovered.truncated_bytes, full - 1);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}
