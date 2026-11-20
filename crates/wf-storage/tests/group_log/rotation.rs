//! Segments rotate by size or on request, and a checkpoint removes the
//! segments before it.

use std::path::Path;

use wf_storage::log::{DirectoryStorage, GroupLog, GroupLogError, GroupLogLimits, SegmentStorage};

use super::{TestResult, directory, record};

fn segments(path: &Path) -> TestResult<Vec<u64>> {
    let mut segments = DirectoryStorage::open(path)?.segments()?;
    segments.sort_unstable();
    Ok(segments)
}

fn append_waited(log: &GroupLog, indices: std::ops::Range<usize>) -> TestResult {
    for index in indices {
        let sequence = log.append(record(index))?;
        log.wait_durable(sequence)?;
    }
    Ok(())
}

#[test]
fn segments_rotate_at_the_size_limit_and_replay_in_order() -> TestResult {
    let directory = directory("size")?;
    let path = directory.join("log");
    let limits = GroupLogLimits {
        max_segment_bytes: 200,
        ..GroupLogLimits::default()
    };
    {
        let (log, _) = GroupLog::open(&path, limits)?;
        append_waited(&log, 0..20)?;
    }
    let present = segments(&path)?;
    assert!(present.len() > 3, "only {present:?} after 20 synced blocks");
    assert_eq!(
        present,
        (0..u64::try_from(present.len())?).collect::<Vec<_>>()
    );
    for id in &present {
        let size = std::fs::metadata(DirectoryStorage::open(&path)?.segment_path(*id))?.len();
        assert!(size <= 200, "segment {id} holds {size} bytes");
    }
    let (_log, recovered) = GroupLog::open(&path, limits)?;
    assert_eq!(recovered.records, (0..20).map(record).collect::<Vec<_>>());
    assert_eq!(recovered.segments, present);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

#[test]
fn a_checkpoint_removes_only_segments_before_the_rotation() -> TestResult {
    let directory = directory("checkpoint")?;
    let path = directory.join("log");
    {
        let (log, _) = GroupLog::open(&path, GroupLogLimits::default())?;
        append_waited(&log, 0..5)?;
        let checkpoint = log.rotate()?;
        assert_eq!(checkpoint, 1);
        append_waited(&log, 5..8)?;
        assert_eq!(segments(&path)?, vec![0, 1]);
        log.remove_before(checkpoint)?;
        assert_eq!(segments(&path)?, vec![1]);
        append_waited(&log, 8..9)?;
    }
    let (_log, recovered) = GroupLog::open(&path, GroupLogLimits::default())?;
    assert_eq!(recovered.records, (5..9).map(record).collect::<Vec<_>>());
    assert_eq!(recovered.segments, vec![1]);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

#[test]
fn a_rotation_keeps_earlier_queued_appends_in_the_old_segment() -> TestResult {
    let directory = directory("order")?;
    let path = directory.join("log");
    {
        let (log, _) = GroupLog::open(&path, GroupLogLimits::default())?;
        let queued: Vec<u64> = (0..50)
            .map(|index| log.append(record(index)))
            .collect::<Result<_, _>>()?;
        let checkpoint = log.rotate()?;
        for sequence in queued {
            log.wait_durable(sequence)?;
        }
        append_waited(&log, 50..51)?;
        log.remove_before(checkpoint)?;
    }
    let (_log, recovered) = GroupLog::open(&path, GroupLogLimits::default())?;
    assert_eq!(recovered.records, vec![record(50)]);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

#[test]
fn a_checkpoint_past_the_current_segment_is_refused() -> TestResult {
    let directory = directory("future")?;
    let path = directory.join("log");
    let (log, _) = GroupLog::open(&path, GroupLogLimits::default())?;
    append_waited(&log, 0..1)?;
    assert_eq!(log.remove_before(1), Err(GroupLogError::InvalidCheckpoint));
    log.remove_before(0)?;
    append_waited(&log, 1..2)?;
    drop(log);
    let (_log, recovered) = GroupLog::open(&path, GroupLogLimits::default())?;
    assert_eq!(recovered.records, vec![record(0), record(1)]);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}
