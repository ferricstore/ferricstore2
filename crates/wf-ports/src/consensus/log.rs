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
    /// [`Self::apply`] included, and calls `done` with its bytes; `false`,
    /// without calling `done`, while the machine serves as the leader's
    /// (its state may hold rounds not yet committed).
    fn snapshot(&self, done: Captured) -> bool;

    /// Replaces the machine's state with a captured one; `done` once it
    /// holds it.
    fn install(&self, snapshot: Vec<u8>, done: Applied);
}

/// Called once with a machine's captured bytes, or why it failed.
pub type Captured = Box<dyn FnOnce(Result<Vec<u8>, ConsensusLogError>) + Send>;

/// Durable snapshots of a group's machine, newest kept.
pub trait SnapshotStore: Send + Sync {
    /// Saves `parts`, joined, as snapshot `id` (ids grow), durably, and
    /// removes earlier ones.
    ///
    /// # Errors
    /// The storage's refusal or failure.
    fn save(&self, id: u64, parts: &[&[u8]]) -> Result<(), ConsensusLogError>;

    /// The newest snapshot and its id, if any.
    ///
    /// # Errors
    /// The storage's refusal or failure.
    fn newest(&self) -> Result<Option<(u64, Vec<u8>)>, ConsensusLogError>;
}
