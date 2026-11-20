//! Opening the group log: every segment is read in order. Segment numbers
//! must be contiguous; removed checkpoints leave only a missing prefix. Only
//! the last segment may end in a torn tail, which is cut; damage anywhere
//! else refuses to open.

use super::storage::{SegmentStorage, SegmentWriter};
use super::{GroupLogError, block};

/// The recovered log and the writer's starting point.
pub(super) struct Opened {
    pub(super) records: Vec<Vec<u8>>,
    pub(super) truncated_bytes: u64,
    pub(super) segments: Vec<u64>,
    pub(super) current: Box<dyn SegmentWriter>,
    pub(super) current_length: u64,
}

fn length(bytes: usize) -> Result<u64, GroupLogError> {
    u64::try_from(bytes).map_err(|_| GroupLogError::Unavailable)
}

pub(super) fn recover(
    storage: &dyn SegmentStorage,
    max_block_bytes: usize,
) -> Result<Opened, GroupLogError> {
    let mut segments = storage.segments().map_err(|_| GroupLogError::Unavailable)?;
    segments.sort_unstable();
    for pair in segments.windows(2) {
        if let [earlier, later] = pair {
            let expected = earlier.checked_add(1).ok_or(GroupLogError::Unavailable)?;
            if *later != expected {
                return Err(GroupLogError::MissingSegment { segment: expected });
            }
        }
    }
    let Some((&last, earlier)) = segments.split_last() else {
        let current = storage.create(0).map_err(|_| GroupLogError::Uncertain)?;
        return Ok(Opened {
            records: Vec::new(),
            truncated_bytes: 0,
            segments: vec![0],
            current,
            current_length: 0,
        });
    };
    let mut records = Vec::new();
    for &segment in earlier {
        let bytes = storage
            .read(segment)
            .map_err(|_| GroupLogError::Unavailable)?;
        let (segment_records, good) = scan(&bytes, segment, max_block_bytes)?;
        if good != bytes.len() {
            let offset = length(good)?;
            return Err(GroupLogError::Corrupt { segment, offset });
        }
        records.extend(segment_records);
    }
    let bytes = storage.read(last).map_err(|_| GroupLogError::Unavailable)?;
    let (last_records, good) = scan(&bytes, last, max_block_bytes)?;
    records.extend(last_records);
    let current_length = length(good)?;
    let cut = bytes
        .len()
        .checked_sub(good)
        .ok_or(GroupLogError::Unavailable)?;
    let truncated_bytes = length(cut)?;
    if truncated_bytes > 0 {
        storage
            .truncate(last, current_length)
            .map_err(|_| GroupLogError::Uncertain)?;
    }
    let current = storage
        .reopen(last)
        .map_err(|_| GroupLogError::Unavailable)?;
    Ok(Opened {
        records,
        truncated_bytes,
        segments,
        current,
        current_length,
    })
}

fn scan(
    bytes: &[u8],
    segment: u64,
    max_block_bytes: usize,
) -> Result<(Vec<Vec<u8>>, usize), GroupLogError> {
    block::scan(bytes, max_block_bytes).map_err(|damaged| match length(damaged) {
        Ok(offset) => GroupLogError::Corrupt { segment, offset },
        Err(error) => error,
    })
}
