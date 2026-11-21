//! Why the queue node did not open or could not snapshot.

use std::fmt;

use wf_storage::log::GroupLogError;
use wf_storage::snapshot::files::SnapshotError;

/// Why the node did not open.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NodeError {
    /// The shard count is 0 or above 1024.
    InvalidShards,
    /// The group log refused to open.
    Log(GroupLogError),
    /// A logged change at this position does not replay.
    Replay {
        /// Position among the recovered records.
        record: usize,
    },
    /// A runtime thread did not start.
    Thread,
    /// Snapshot storage refused a read, write or removal.
    Snapshot(SnapshotError),
    /// A snapshot body or shard image does not decode.
    SnapshotImage,
    /// A shard of the loaded snapshot never logged its marker, so the log
    /// does not match the snapshot.
    MissingMarker {
        /// The shard, under the snapshot's shard count.
        shard: u16,
    },
    /// A shard did not capture its records for a snapshot.
    Capture,
    /// The node is stopping.
    Stopped,
}

impl fmt::Display for NodeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidShards => formatter.write_str("shard count must be 1 through 1024"),
            Self::Log(error) => write!(formatter, "group log: {error}"),
            Self::Replay { record } => write!(formatter, "logged change {record} does not replay"),
            Self::Thread => formatter.write_str("a runtime thread did not start"),
            Self::Snapshot(error) => write!(formatter, "snapshot: {error}"),
            Self::SnapshotImage => formatter.write_str("a snapshot does not decode"),
            Self::MissingMarker { shard } => {
                write!(formatter, "the log lacks shard {shard}'s snapshot marker")
            }
            Self::Capture => formatter.write_str("a shard did not capture its snapshot"),
            Self::Stopped => formatter.write_str("the node is stopping"),
        }
    }
}

impl std::error::Error for NodeError {}

impl From<GroupLogError> for NodeError {
    fn from(error: GroupLogError) -> Self {
        Self::Log(error)
    }
}

impl From<SnapshotError> for NodeError {
    fn from(error: SnapshotError) -> Self {
        Self::Snapshot(error)
    }
}
