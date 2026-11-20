//! Recovery cuts a torn tail and refuses corruption before a later valid block.

use std::fs::OpenOptions;
use std::io::{Seek, SeekFrom, Write};
use std::path::Path;

use wf_storage::log::{GroupLog, GroupLogError, GroupLogLimits};

use super::{TestResult, directory, record};

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
fn a_cut_last_block_is_dropped_and_the_file_is_truncated() -> TestResult {
    let directory = directory("torn")?;
    let path = directory.join("node.log");
    write_blocks(&path, 1, 3)?;
    let first_block = length(&path)?;
    write_blocks(&path, 1, 3)?;
    let full = length(&path)?;
    for cut in [full - 1, first_block + 10, first_block + 1] {
        let copy = directory.join(format!("cut-{cut}.log"));
        std::fs::copy(&path, &copy)?;
        OpenOptions::new().write(true).open(&copy)?.set_len(cut)?;
        let (log, recovered) = GroupLog::open(&copy, GroupLogLimits::default())?;
        assert_eq!(
            recovered.records,
            (0..3).map(record).collect::<Vec<_>>(),
            "cut at {cut}"
        );
        assert_eq!(recovered.truncated_bytes, cut - first_block);
        assert_eq!(length(&copy)?, first_block);
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
    let path = directory.join("node.log");
    write_blocks(&path, 2, 2)?;
    let full = length(&path)?;
    OpenOptions::new()
        .append(true)
        .open(&path)?
        .write_all(&[0xA5; 100])?;
    let (_log, recovered) = GroupLog::open(&path, GroupLogLimits::default())?;
    assert_eq!(recovered.records.len(), 4);
    assert_eq!(recovered.truncated_bytes, 100);
    assert_eq!(length(&path)?, full);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

#[test]
fn a_bad_block_before_a_valid_block_is_corruption_not_a_tail() -> TestResult {
    let directory = directory("corrupt")?;
    let path = directory.join("node.log");
    write_blocks(&path, 2, 2)?;
    let full = length(&path)?;
    {
        let mut file = OpenOptions::new().write(true).open(&path)?;
        // The last byte of the first block's payload.
        file.seek(SeekFrom::Start(full / 2 - 1))?;
        file.write_all(&[0xFF])?;
    }
    let error = GroupLog::open(&path, GroupLogLimits::default())
        .err()
        .ok_or("corrupt log opened")?;
    assert_eq!(error, GroupLogError::Corrupt { offset: 0 });
    assert_eq!(
        length(&path)?,
        full,
        "corruption must not be truncated away"
    );
    std::fs::remove_dir_all(directory)?;
    Ok(())
}
