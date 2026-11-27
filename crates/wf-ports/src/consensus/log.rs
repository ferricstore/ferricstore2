//! The consensus log port (ADR-018): a log of identified entries with a
//! durable vote, suffix truncation and prefix purge, in consensus-neutral
//! types. `wf-storage` implements it; `wf-consensus` adapts it to its Raft
//! library; `wf-node` wires the two.

use std::fmt;
use std::ops::Range;
use std::sync::Arc;

/// An entry's identity: the term and node of the leader that wrote it, and
/// its index.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EntryId {
    /// Leader term.
    pub term: u64,
    /// Leader node.
    pub node: u64,
    /// Position in the log.
    pub index: u64,
}

/// A refused or failed log operation, with the storage's description.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConsensusLogError {
    message: String,
}

impl ConsensusLogError {
    /// An error with this description.
    #[must_use]
    pub const fn new(message: String) -> Self {
        Self { message }
    }
}

impl fmt::Display for ConsensusLogError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for ConsensusLogError {}

/// Called once with the outcome of an append's sync.
pub type Durable = Box<dyn FnOnce(Result<(), ConsensusLogError>) + Send>;

/// An entry's id and bytes.
pub type Entry = (EntryId, Arc<[u8]>);

/// A durable log of identified entries.
pub trait ConsensusLog: Send + Sync {
    /// The last purged id and the last entry's id (the purged one when no
    /// entry is kept).
    fn bounds(&self) -> (Option<EntryId>, Option<EntryId>);

    /// The last saved vote.
    fn vote(&self) -> Option<Vec<u8>>;

    /// Saves `vote`; returns once durable.
    ///
    /// # Errors
    /// The storage's refusal or failure.
    fn save_vote(&self, vote: &[u8]) -> Result<(), ConsensusLogError>;

    /// Whether the member keeping this log may vote: it settled into a new
    /// group, or caught up after it lost its log (ADR-019). A log whose
    /// storage was lost lost this mark with it.
    fn voter(&self) -> bool;

    /// Marks that the member keeping this log may vote; returns once
    /// durable.
    ///
    /// # Errors
    /// The storage's refusal or failure.
    fn save_voter(&self) -> Result<(), ConsensusLogError>;

    /// Appends entries that follow the last one (or replace a suffix from an
    /// index already held); readers see them at once and `done` is called
    /// once they are durable.
    ///
    /// # Errors
    /// A gap, or the storage's refusal; `done` is then not called.
    fn append(
        &self,
        entries: Vec<(EntryId, Vec<u8>)>,
        done: Durable,
    ) -> Result<(), ConsensusLogError>;

    /// The kept entries with indexes in `range`, in order.
    fn entries(&self, range: Range<u64>) -> Vec<Entry>;

    /// Drops every entry after index `after` (all with `None`); returns once
    /// durable.
    ///
    /// # Errors
    /// The storage's refusal or failure.
    fn truncate_after(&self, after: Option<u64>) -> Result<(), ConsensusLogError>;

    /// Drops the entries up to `upto`; returns once durable.
    ///
    /// # Errors
    /// The storage's refusal or failure.
    fn purge(&self, upto: EntryId) -> Result<(), ConsensusLogError>;

    /// Closes the log as its member stops: from now on every change is
    /// refused and changes nothing, and a change still waiting on its disk
    /// is refused at once, so work still in flight leaves the storage as it
    /// was for whatever opens it next. A change refused this way was never
    /// reported durable, though it may still reach the disk. Reading goes
    /// on.
    fn close(&self);
}

/// Called once a machine applied what it was given, or failed to.
pub type Applied = Box<dyn FnOnce(Result<(), ConsensusLogError>) + Send>;

/// The replicated state a group's committed entries change (ADR-018): the
/// consensus adapter hands it other members' proposals in commit order, and
/// tells it when this member starts and stops leading.
pub trait ReplicatedMachine: Send + Sync {
    /// Applies committed proposals in order; `done` once applied.
    fn apply(&self, proposals: Vec<Arc<[u8]>>, done: Applied);

    /// This member leads from now on, with every earlier entry applied;
    /// `done` once it serves.
    fn promote(&self, done: Applied);

    /// This member no longer leads.
    fn demote(&self);

    /// Starts capturing the machine as it is now, every proposal handed to
    /// [`Self::apply`] included, and calls `done` with the capture, which
    /// writes it out ([`Capture::write_to`]); `false`, without calling
    /// `done`, while the machine serves as the leader's (its state may hold
    /// rounds not yet committed).
    fn snapshot(&self, done: Captured) -> bool;

    /// Replaces the machine's state with the one `snapshot` holds, as a
    /// capture wrote it, reading it as it goes; `done` once it holds it.
    /// The new state takes effect only once the whole of `snapshot` was
    /// read and checked; a read error leaves the machine as it was.
    fn install(&self, snapshot: Box<dyn SnapshotSource>, done: Applied);
}

/// A machine captured at one position, written out once.
pub trait Capture: Send {
    /// Writes the capture to `out`, a part at a time, so the whole machine
    /// is never in memory at once.
    ///
    /// # Errors
    /// A part that cannot be captured, and `out`'s failures.
    fn write_to(self: Box<Self>, out: &mut dyn std::io::Write) -> Result<(), ConsensusLogError>;
}

/// Called once with a machine's capture, or why it failed.
pub type Captured = Box<dyn FnOnce(Result<Box<dyn Capture>, ConsensusLogError>) + Send>;

/// A snapshot being written; nothing is visible until it is finished.
pub trait SnapshotSink: std::io::Write + Send {
    /// Makes what was written durable as a finished snapshot, not yet the
    /// newest ([`SnapshotStore::adopt`]); returns its length in bytes.
    ///
    /// # Errors
    /// The storage's refusal or failure; the snapshot is then dropped.
    fn finish(self: Box<Self>) -> Result<u64, ConsensusLogError>;
}

/// A finished snapshot being read. Its bytes are checked as they are read:
/// the read that reaches its end fails when they differ from what was
/// written.
pub trait SnapshotSource: std::io::Read + Send {
    /// Its length in bytes.
    fn length(&self) -> u64;
}

/// Durable snapshots of a group's machine, written and read as streams.
pub trait SnapshotStore: Send + Sync {
    /// Starts writing snapshot `id` (its last log index).
    ///
    /// # Errors
    /// The storage's refusal or failure.
    fn create(&self, id: u64) -> Result<Box<dyn SnapshotSink>, ConsensusLogError>;

    /// Makes finished snapshot `id` the newest and removes every other
    /// snapshot; readers already open keep reading theirs.
    ///
    /// # Errors
    /// The storage's refusal or failure, or no finished snapshot `id`.
    fn adopt(&self, id: u64) -> Result<(), ConsensusLogError>;

    /// The newest adopted snapshot's id, if any.
    ///
    /// # Errors
    /// The storage's refusal or failure.
    fn newest(&self) -> Result<Option<u64>, ConsensusLogError>;

    /// Opens finished snapshot `id`, adopted or not.
    ///
    /// # Errors
    /// The storage's refusal or failure, or no finished snapshot `id`.
    fn open(&self, id: u64) -> Result<Box<dyn SnapshotSource>, ConsensusLogError>;

    /// Closes the store as its member stops: from now on, creating,
    /// finishing and adopting a snapshot are refused and change nothing,
    /// so work still in flight leaves the storage as it was for whatever
    /// opens it next. Reading goes on.
    fn close(&self);
}
