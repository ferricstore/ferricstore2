//! Node-local group-committed log (ADR-011).
//!
//! Every shard group appends to one log. A single writer thread takes all
//! queued batches, writes them as blocks with one write and one sync, and only
//! then publishes them as durable, so one device flush covers every group's
//! appends. A batch is never split across blocks, so recovery replays it whole
//! or not at all.
//!
//! The log is a run of numbered segments. The writer starts a new segment when
//! the next write would pass the size limit, or when asked to rotate. After a
//! checkpoint covers everything before a rotation, [`GroupLog::remove_before`]
//! deletes the older segments, oldest first, so a crash leaves only a missing
//! prefix.
//!
//! Recovery keeps every complete block. A bad end of the last segment is a
//! torn tail from the final unsynced write and is cut. Damage anywhere else, or
//! a missing middle segment, refuses to open instead of dropping acknowledged
//! records. A failed write, sync or segment creation fences the log until it
//! is reopened.

mod block;
mod recovery;
mod storage;
mod writer;

use std::fmt;
use std::path::Path;
use std::sync::Arc;
use std::thread::JoinHandle;

pub use storage::{DirectoryStorage, SegmentStorage, SegmentWriter};
use writer::{Item, Shared, Writer};

/// Size limits for records, blocks and segments.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GroupLogLimits {
    /// Largest accepted record.
    pub max_record_bytes: usize,
    /// Largest block, header included; one batch must fit in one block.
    pub max_block_bytes: usize,
    /// Segment size that starts a new segment; one write is never split, so a
    /// segment can exceed it by one write.
    pub max_segment_bytes: u64,
    /// Record bytes queued for the writer at which appenders wait for it to
    /// take them (ADR-017); one batch may take the queue past it.
    pub max_queued_bytes: usize,
}

impl Default for GroupLogLimits {
    fn default() -> Self {
        Self {
            max_record_bytes: 16 * 1024 * 1024,
            max_block_bytes: 64 * 1024 * 1024,
            max_segment_bytes: 128 * 1024 * 1024,
            max_queued_bytes: 64 * 1024 * 1024,
        }
    }
}

/// What recovery found when the log was opened.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Recovered {
    /// Every record of every complete block, in append order.
    pub records: Vec<Vec<u8>>,
    /// Bytes of torn tail cut from the end of the last segment.
    pub truncated_bytes: u64,
    /// The segments present, oldest first.
    pub segments: Vec<u64>,
    /// Where each segment's records start in `records`, by position in
    /// `segments`.
    pub starts: Vec<usize>,
}

/// A refused open, append, wait, rotation or removal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GroupLogError {
    /// Storage could not be listed, read or opened, or a removal failed.
    Unavailable,
    /// A write, sync, truncation or segment creation failed; later appends
    /// are refused, and earlier appends that were not yet durable may or may
    /// not survive a reopen.
    Uncertain,
    /// A damaged block in a segment that recovery may not cut.
    Corrupt {
        /// The damaged segment.
        segment: u64,
        /// Offset of the first damaged block in it.
        offset: u64,
    },
    /// A segment between two present segments is missing.
    MissingSegment {
        /// The first missing segment.
        segment: u64,
    },
    /// A record or batch exceeds the limits, or the limits are unusable.
    TooLarge,
    /// A batch has no records.
    EmptyBatch,
    /// A removal named a segment after the current one.
    InvalidCheckpoint,
    /// The log is shutting down.
    Closed,
}

impl fmt::Display for GroupLogError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unavailable => formatter.write_str("group log unavailable"),
            Self::Uncertain => formatter.write_str("group log write or sync failed"),
            Self::Corrupt { segment, offset } => {
                write!(
                    formatter,
                    "group log segment {segment} corrupt at byte {offset}"
                )
            }
            Self::MissingSegment { segment } => {
                write!(formatter, "group log segment {segment} is missing")
            }
            Self::TooLarge => formatter.write_str("record or batch exceeds the group log limits"),
            Self::EmptyBatch => formatter.write_str("group log batch has no records"),
            Self::InvalidCheckpoint => {
                formatter.write_str("group log checkpoint is after the current segment")
            }
            Self::Closed => formatter.write_str("group log is closed"),
        }
    }
}

impl std::error::Error for GroupLogError {}

/// The open log. Dropping it writes and syncs every queued batch, then stops
/// the writer; a closed log's writer stops on its own (`close`).
pub struct GroupLog {
    shared: Arc<Shared>,
    limits: GroupLogLimits,
    writer: Option<JoinHandle<()>>,
}

impl GroupLog {
    /// Opens or creates the log in `directory`, recovers it and starts the
    /// writer.
    ///
    /// # Errors
    /// As [`Self::open_with`], and [`GroupLogError::Unavailable`] when the
    /// directory cannot be created.
    pub fn open(
        directory: &Path,
        limits: GroupLogLimits,
    ) -> Result<(Self, Recovered), GroupLogError> {
        let storage = DirectoryStorage::open(directory).map_err(|_| GroupLogError::Unavailable)?;
        Self::open_with(Arc::new(storage), limits)
    }

    /// Opens the log kept in `storage`, recovers it and starts the writer.
    ///
    /// # Errors
    /// Refuses unusable limits, unreadable storage, damaged or missing
    /// segments, and a failed truncation or segment creation.
    pub fn open_with(
        storage: Arc<dyn SegmentStorage>,
        limits: GroupLogLimits,
    ) -> Result<(Self, Recovered), GroupLogError> {
        let usable = limits.max_record_bytes > 0
            && limits.max_segment_bytes > 0
            && block::batch_fits(&[limits.max_record_bytes], limits.max_block_bytes) == Some(true);
        if !usable {
            return Err(GroupLogError::TooLarge);
        }
        let opened = recovery::recover(storage.as_ref(), limits.max_block_bytes)?;
        let (Some(&oldest), Some(&segment)) = (opened.segments.first(), opened.segments.last())
        else {
            return Err(GroupLogError::Unavailable);
        };
        let writer = Writer {
            storage,
            current: opened.current,
            segment,
            length: opened.current_length,
            oldest,
            max_block_bytes: limits.max_block_bytes,
            max_segment_bytes: limits.max_segment_bytes,
        };
        let shared = Arc::new(Shared::new(segment, limits.max_queued_bytes));
        let handle = writer::spawn(Arc::clone(&shared), writer)?;
        let log = Self {
            shared,
            limits,
            writer: Some(handle),
        };
        let recovered = Recovered {
            records: opened.records,
            truncated_bytes: opened.truncated_bytes,
            segments: opened.segments,
            starts: opened.starts,
        };
        Ok((log, recovered))
    }

    /// Queues one record and returns its sequence for [`Self::wait_durable`].
    ///
    /// # Errors
    /// Refuses an oversized record, a closed log and a fenced log.
    pub fn append(&self, record: Vec<u8>) -> Result<u64, GroupLogError> {
        self.append_batch(vec![record])
    }

    /// Queues records that recovery replays whole or not at all, and returns
    /// the batch's sequence for [`Self::wait_durable`].
    ///
    /// # Errors
    /// Refuses an empty or oversized batch, a closed log and a fenced log.
    pub fn append_batch(&self, records: Vec<Vec<u8>>) -> Result<u64, GroupLogError> {
        if records.is_empty() {
            return Err(GroupLogError::EmptyBatch);
        }
        let lengths: Vec<usize> = records.iter().map(Vec::len).collect();
        let records_fit = lengths
            .iter()
            .all(|length| *length <= self.limits.max_record_bytes);
        if !records_fit || block::batch_fits(&lengths, self.limits.max_block_bytes) != Some(true) {
            return Err(GroupLogError::TooLarge);
        }
        self.shared.enqueue(Item::Batch(records))
    }

    /// Blocks until the item with `sequence` and every earlier one is handled.
    ///
    /// # Errors
    /// Returns the writer's failure if it failed before reaching `sequence`.
    pub fn wait_durable(&self, sequence: u64) -> Result<(), GroupLogError> {
        self.shared.wait(sequence)
    }

    /// Starts a new segment after every earlier append is durable, and returns
    /// its number. Everything appended before this call lives in lower
    /// segments; pass the number to [`Self::remove_before`] once a checkpoint
    /// covers those appends.
    ///
    /// # Errors
    /// Refuses a closed or fenced log; a failed creation fences the log.
    pub fn rotate(&self) -> Result<u64, GroupLogError> {
        let sequence = self.shared.enqueue(Item::Rotate)?;
        self.shared.outcome(sequence)
    }

    /// Removes every segment numbered below `segment`, oldest first, after
    /// every earlier append is durable.
    ///
    /// # Errors
    /// [`GroupLogError::InvalidCheckpoint`] for a segment after the current
    /// one, [`GroupLogError::Unavailable`] when a removal fails (the remaining
    /// segments stay valid and a retry continues), and the refusals of a closed
    /// or fenced log.
    pub fn remove_before(&self, segment: u64) -> Result<(), GroupLogError> {
        let sequence = self.shared.enqueue(Item::RemoveBefore(segment))?;
        self.shared.outcome(sequence).map(drop)
    }

    /// The limits the log was opened with.
    #[must_use]
    pub const fn limits(&self) -> GroupLogLimits {
        self.limits
    }

    /// The segment new appends go to: the newest present segment.
    #[must_use]
    pub fn current_segment(&self) -> u64 {
        self.shared.segment()
    }

    /// Closes the log for good, as its owner stops: queued records are
    /// dropped unwritten, every append, rotation, removal and wait not yet
    /// answered is refused with [`GroupLogError::Closed`] at once, and the
    /// writer stops before its next step, so the segments stay as they are
    /// for whatever opens them next. Only a write already under way
    /// finishes; its records were never reported durable.
    pub fn close(&self) {
        self.shared.close();
    }

    /// Syncs performed since open; fewer syncs than waited appends shows that
    /// appends were grouped.
    #[must_use]
    pub fn sync_count(&self) -> u64 {
        self.shared.syncs()
    }
}

impl Drop for GroupLog {
    fn drop(&mut self) {
        if self.shared.closed() {
            // A closed log's writer changes nothing more, but may still
            // wait on its disk: dropping does not wait for it.
            return;
        }
        self.shared.finish();
        if let Some(writer) = self.writer.take() {
            // The writer publishes its outcome before it exits; a join error
            // has nothing further to report from drop.
            let _joined = writer.join();
        }
    }
}
