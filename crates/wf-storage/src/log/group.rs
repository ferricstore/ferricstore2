//! Node-local group-committed log (ADR-011).
//!
//! Every shard group appends to one file. A single writer thread takes all
//! queued batches, writes them as blocks with one write and one `sync_data`,
//! and only then publishes them as durable, so one device flush covers every
//! group's appends. A batch is never split across blocks, so recovery replays
//! it whole or not at all.
//!
//! Recovery keeps every complete block. A bad last block is a torn tail from
//! the final unsynced write and is cut; a bad block followed by a valid one
//! means synced data was damaged, so the log refuses to open instead of
//! dropping acknowledged records. Segment rotation and checkpoint truncation
//! are not implemented yet: the file grows until they are.

mod block;
mod writer;

use std::fmt;
use std::fs::{File, OpenOptions};
use std::io::{ErrorKind, Read};
use std::path::Path;
use std::sync::Arc;
use std::thread::JoinHandle;

use writer::Shared;

/// Size limits for one record and one block.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GroupLogLimits {
    /// Largest accepted record.
    pub max_record_bytes: usize,
    /// Largest block, header included; one batch must fit in one block.
    pub max_block_bytes: usize,
}

impl Default for GroupLogLimits {
    fn default() -> Self {
        Self {
            max_record_bytes: 16 * 1024 * 1024,
            max_block_bytes: 64 * 1024 * 1024,
        }
    }
}

/// What recovery found when the log was opened.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Recovered {
    /// Every record of every complete block, in append order.
    pub records: Vec<Vec<u8>>,
    /// Bytes of torn tail cut from the end of the file.
    pub truncated_bytes: u64,
}

/// A refused open, append or wait.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GroupLogError {
    /// The file could not be opened, read or created.
    Unavailable,
    /// A write or sync failed; later appends are refused and earlier ones
    /// that were not yet durable may or may not survive a restart.
    Uncertain,
    /// A damaged block precedes a valid one at this byte offset.
    Corrupt {
        /// Offset of the first damaged block.
        offset: u64,
    },
    /// A record or batch exceeds the limits, or the limits are unusable.
    TooLarge,
    /// A batch has no records.
    EmptyBatch,
    /// The log is shutting down.
    Closed,
}

impl fmt::Display for GroupLogError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unavailable => formatter.write_str("group log unavailable"),
            Self::Uncertain => formatter.write_str("group log write or sync failed"),
            Self::Corrupt { offset } => write!(formatter, "group log corrupt at byte {offset}"),
            Self::TooLarge => formatter.write_str("record or batch exceeds the group log limits"),
            Self::EmptyBatch => formatter.write_str("group log batch has no records"),
            Self::Closed => formatter.write_str("group log is closed"),
        }
    }
}

impl std::error::Error for GroupLogError {}

/// The open log. Dropping it writes and syncs every queued batch, then stops
/// the writer.
pub struct GroupLog {
    shared: Arc<Shared>,
    limits: GroupLogLimits,
    writer: Option<JoinHandle<()>>,
}

impl GroupLog {
    /// Opens or creates the log at `path`, recovers it and starts the writer.
    ///
    /// # Errors
    /// Refuses unusable limits, an unreadable file, mid-log corruption and a
    /// failed truncation or directory sync.
    pub fn open(path: &Path, limits: GroupLogLimits) -> Result<(Self, Recovered), GroupLogError> {
        if limits.max_record_bytes == 0
            || block::batch_fits(&[limits.max_record_bytes], limits.max_block_bytes) != Some(true)
        {
            return Err(GroupLogError::TooLarge);
        }
        let (mut file, created) = open_file(path)?;
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)
            .map_err(|_| GroupLogError::Unavailable)?;
        let (records, good) = block::scan(&bytes, limits.max_block_bytes)?;
        let cut = bytes
            .len()
            .checked_sub(good)
            .ok_or(GroupLogError::Unavailable)?;
        let truncated_bytes = u64::try_from(cut).map_err(|_| GroupLogError::Unavailable)?;
        if truncated_bytes > 0 {
            let length = u64::try_from(good).map_err(|_| GroupLogError::Unavailable)?;
            file.set_len(length)
                .and_then(|()| file.sync_data())
                .map_err(|_| GroupLogError::Uncertain)?;
        }
        if created {
            sync_parent(path)?;
        }
        let shared = Arc::new(Shared::new(limits.max_block_bytes));
        let writer = writer::spawn(Arc::clone(&shared), file)?;
        let log = Self {
            shared,
            limits,
            writer: Some(writer),
        };
        Ok((
            log,
            Recovered {
                records,
                truncated_bytes,
            },
        ))
    }

    /// Queues one record and returns its sequence for [`Self::wait_durable`].
    ///
    /// # Errors
    /// Refuses an oversized record, a closed log and a log whose writer failed.
    pub fn append(&self, record: Vec<u8>) -> Result<u64, GroupLogError> {
        self.append_batch(vec![record])
    }

    /// Queues records that recovery replays whole or not at all, and returns
    /// the batch's sequence for [`Self::wait_durable`].
    ///
    /// # Errors
    /// Refuses an empty or oversized batch, a closed log and a log whose
    /// writer failed.
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
        self.shared.enqueue(records)
    }

    /// Blocks until the batch with `sequence` and every earlier one is synced.
    ///
    /// # Errors
    /// Returns the writer's failure if it failed before reaching `sequence`.
    pub fn wait_durable(&self, sequence: u64) -> Result<(), GroupLogError> {
        self.shared.wait(sequence)
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
        self.shared.close();
        if let Some(writer) = self.writer.take() {
            // The writer publishes its outcome before it exits; a join error
            // has nothing further to report from drop.
            let _joined = writer.join();
        }
    }
}

fn open_file(path: &Path) -> Result<(File, bool), GroupLogError> {
    let mut options = OpenOptions::new();
    options.read(true).append(true);
    match options.clone().create_new(true).open(path) {
        Ok(file) => Ok((file, true)),
        Err(error) if error.kind() == ErrorKind::AlreadyExists => options
            .open(path)
            .map(|file| (file, false))
            .map_err(|_| GroupLogError::Unavailable),
        Err(_) => Err(GroupLogError::Unavailable),
    }
}

/// Makes a new file's directory entry durable.
fn sync_parent(path: &Path) -> Result<(), GroupLogError> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    File::open(parent)
        .and_then(|directory| directory.sync_all())
        .map_err(|_| GroupLogError::Uncertain)
}
