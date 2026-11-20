//! Recovery cuts a torn tail of the last segment and refuses damage anywhere
//! else, and refuses a missing middle segment.

use std::fs::OpenOptions;
use std::io::{Seek, SeekFrom, Write};
use std::path::Path;

use wf_storage::log::{GroupLog, GroupLogError, GroupLogLimits};

use super::{TestResult, directory, record, segment};

/// Writes `blocks` separately synced blocks of `per_block` records each: a
/// batch is never split, and waiting before the next batch keeps them apart.
fn write_blocks(path: &Path, blocks: usize, per_block: usize) -> TestResult {
    let (log, _) = GroupLog::open(path, GroupLogLimits::default())?;
    for block in 0..blocks {
        let batch = (0..per_block)
            .map(|index| record(block * per_block + index))
            .collect();
        let sequence = log.append_batch(batch)?;
        log.wait_durable(sequence)?;
    }
    Ok(())
}

fn length(path: &Path) -> TestResult<u64> {
    Ok(std::fs::metadata(path)?.len())
}

#[test]
fn a_cut_last_block_is_dropped_and_the_segment_is_truncated() -> TestResult {
    let directory = directory("torn")?;
    let source = directory.join("log");
    write_blocks(&source, 1, 3)?;
    let first_block = length(&segment(&source, 0)?)?;
    write_blocks(&source, 1, 3)?;
    let full = length(&segment(&source, 0)?)?;
    for cut in [full - 1, first_block + 10, first_block + 1] {
        let copy = directory.join(format!("cut-{cut}"));
        std::fs::create_dir(&copy)?;
        let file = segment(&copy, 0)?;
        std::fs::copy(segment(&source, 0)?, &file)?;
        OpenOptions::new().write(true).open(&file)?.set_len(cut)?;
        let (log, recovered) = GroupLog::open(&copy, GroupLogLimits::default())?;
        assert_eq!(
            recovered.records,
            (0..3).map(record).collect::<Vec<_>>(),
            "cut at {cut}"
        );
        assert_eq!(recovered.truncated_bytes, cut - first_block);
        assert_eq!(length(&file)?, first_block);
        let next = log.append(record(99))?;
        log.wait_durable(next)?;
        drop(log);
        let (_log, reopened) = GroupLog::open(&copy, GroupLogLimits::default())?;
        assert_eq!(reopened.records.len(), 4);
    }
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

#[test]
fn garbage_after_the_last_block_is_a_torn_tail() -> TestResult {
    let directory = directory("garbage")?;
    let path = directory.join("log");
    write_blocks(&path, 2, 2)?;
    let file = segment(&path, 0)?;
    let full = length(&file)?;
    OpenOptions::new()
        .append(true)
        .open(&file)?
        .write_all(&[0xA5; 100])?;
    let (_log, recovered) = GroupLog::open(&path, GroupLogLimits::default())?;
    assert_eq!(recovered.records.len(), 4);
    assert_eq!(recovered.truncated_bytes, 100);
    assert_eq!(length(&file)?, full);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

#[test]
fn a_bad_block_before_a_valid_block_is_corruption_not_a_tail() -> TestResult {
    let directory = directory("corrupt")?;
    let path = directory.join("log");
    write_blocks(&path, 2, 2)?;
    let file = segment(&path, 0)?;
    let full = length(&file)?;
    {
        let mut writer = OpenOptions::new().write(true).open(&file)?;
        // The last byte of the first block's payload.
        writer.seek(SeekFrom::Start(full / 2 - 1))?;
        writer.write_all(&[0xFF])?;
    }
    let error = GroupLog::open(&path, GroupLogLimits::default())
        .err()
        .ok_or("corrupt log opened")?;
    assert_eq!(
        error,
        GroupLogError::Corrupt {
            segment: 0,
            offset: 0
        }
    );
    assert_eq!(
        length(&file)?,
        full,
        "corruption must not be truncated away"
    );
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

#[test]
fn damage_at_the_end_of_an_earlier_segment_is_corruption() -> TestResult {
    let directory = directory("earlier")?;
    let path = directory.join("log");
    {
        let (log, _) = GroupLog::open(&path, GroupLogLimits::default())?;
        let first = log.append(record(0))?;
        log.wait_durable(first)?;
        log.rotate()?;
        let second = log.append(record(1))?;
        log.wait_durable(second)?;
    }
    let file = segment(&path, 0)?;
    let full = length(&file)?;
    OpenOptions::new()
        .write(true)
        .open(&file)?
        .set_len(full - 1)?;
    let error = GroupLog::open(&path, GroupLogLimits::default())
        .err()
        .ok_or("damaged earlier segment opened")?;
    assert_eq!(
        error,
        GroupLogError::Corrupt {
            segment: 0,
            offset: 0
        }
    );
    assert_eq!(length(&file)?, full - 1, "an earlier segment is never cut");
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

#[test]
fn a_missing_middle_segment_is_refused() -> TestResult {
    let directory = directory("gap")?;
    let path = directory.join("log");
    {
        let (log, _) = GroupLog::open(&path, GroupLogLimits::default())?;
        for index in 0..3 {
            let sequence = log.append(record(index))?;
            log.wait_durable(sequence)?;
            log.rotate()?;
        }
    }
    std::fs::remove_file(segment(&path, 1)?)?;
    let error = GroupLog::open(&path, GroupLogLimits::default())
        .err()
        .ok_or("log with a gap opened")?;
    assert_eq!(error, GroupLogError::MissingSegment { segment: 1 });
    std::fs::remove_dir_all(directory)?;
    Ok(())
}
