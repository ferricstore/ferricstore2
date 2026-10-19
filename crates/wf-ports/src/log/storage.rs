//! Synchronous durable log values; no consensus or application authority.

use std::{fmt, ops::Bound};

use wf_types::identity::{GroupId, RecoveryId};
use wf_types::version::LogIdentity;

/// Default multi-leader profile vote, ordered by term, node, then commitment.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RaftVote {
    term: u64,
    node: u64,
    committed: bool,
}

impl RaftVote {
    /// Preserves an explicit vote without electing a leader.
    #[must_use]
    pub const fn new(term: u64, node: u64, committed: bool) -> Self {
        Self {
            term,
            node,
            committed,
        }
    }
    /// Reads the voting term.
    #[must_use]
    pub const fn term(self) -> u64 {
        self.term
    }
    /// Reads the leader node.
    #[must_use]
    pub const fn node(self) -> u64 {
        self.node
    }
    /// Reads whether the vote is committed.
    #[must_use]
    pub const fn committed(self) -> bool {
        self.committed
    }
}

/// One owned opaque log body; construction neither admits nor decodes it.
pub struct RaftLogEntry {
    id: LogIdentity,
    body: Vec<u8>,
}

impl RaftLogEntry {
    /// Moves the caller's body without cloning or changing capacity.
    #[must_use]
    pub const fn new(id: LogIdentity, body: Vec<u8>) -> Self {
        Self { id, body }
    }
    /// Reads the full term/node/index identity.
    #[must_use]
    pub const fn id(&self) -> LogIdentity {
        self.id
    }
    /// Borrows the original opaque bytes.
    #[must_use]
    pub fn body(&self) -> &[u8] {
        &self.body
    }
    /// Transfers both owners without a conversion allocation.
    #[must_use]
    pub fn into_parts(self) -> (LogIdentity, Vec<u8>) {
        (self.id, self.body)
    }
}

/// Observed scalar metadata, without a storage-origin certificate.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RaftLogState {
    vote: Option<RaftVote>,
    committed: Option<LogIdentity>,
    last_log: Option<LogIdentity>,
    anchor: Option<(LogIdentity, [u8; 32])>,
}

impl RaftLogState {
    /// Preserves explicitly supplied observations; the adapter validates them.
    #[must_use]
    pub const fn new(
        vote: Option<RaftVote>,
        committed: Option<LogIdentity>,
        last_log: Option<LogIdentity>,
    ) -> Self {
        Self {
            vote,
            committed,
            last_log,
            anchor: None,
        }
    }
    /// Preserves supplied scalar observations, including a mathematical anchor.
    /// The caller establishes cross-database provenance separately.
    #[must_use]
    pub const fn new_with_anchor(
        vote: Option<RaftVote>,
        committed: Option<LogIdentity>,
        last_log: Option<LogIdentity>,
        anchor: Option<(LogIdentity, [u8; 32])>,
    ) -> Self {
        Self {
            vote,
            committed,
            last_log,
            anchor,
        }
    }
    /// Reads the persisted vote.
    #[must_use]
    pub const fn vote(self) -> Option<RaftVote> {
        self.vote
    }
    /// Reads the exact durable committed boundary.
    #[must_use]
    pub const fn committed(self) -> Option<LogIdentity> {
        self.committed
    }
    /// Reads the last physical identity, or the purged identity when empty.
    #[must_use]
    pub const fn last_log(self) -> Option<LogIdentity> {
        self.last_log
    }
    /// Reads the full inclusive purged boundary.
    #[must_use]
    pub const fn last_purged(self) -> Option<LogIdentity> {
        match self.anchor {
            Some((position, _)) => Some(position),
            None => None,
        }
    }
    /// Reads the exact artifact reference, without certifying its provenance.
    #[must_use]
    pub const fn purged_snapshot_id(self) -> Option<[u8; 32]> {
        match self.anchor {
            Some((_, id)) => Some(id),
            None => None,
        }
    }
}

/// Logical owner and validation-work caps, separate from engine/cache memory.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RaftLogLimits {
    entry_bytes: usize,
    batch_entries: usize,
    append_bytes: usize,
    read_entries: usize,
    read_bytes: usize,
    recovery_entries: usize,
    recovery_bytes: usize,
    workspace_bytes: usize,
}

impl Default for RaftLogLimits {
    fn default() -> Self {
        Self {
            entry_bytes: 16_777_216,
            batch_entries: 1_000,
            append_bytes: 67_108_864,
            read_entries: 1_000,
            read_bytes: 67_108_864,
            recovery_entries: 1_000_000,
            recovery_bytes: 1_073_741_824,
            workspace_bytes: 268_435_456,
        }
    }
}

impl RaftLogLimits {
    /// Validates positive limits; each tuple is (count, bytes).
    /// Workspace may be smaller than an operation's prospective owner peak.
    /// # Errors
    /// Rejects zero, overflowing minima, or byte caps below one maximal row.
    pub fn new(
        entry_bytes: usize,
        append: (usize, usize),
        read: (usize, usize),
        recovery: (usize, usize),
        workspace_bytes: usize,
    ) -> Result<Self, RaftLogError> {
        let owner = entry_bytes
            .checked_add(512)
            .ok_or(RaftLogError::InvalidLimits)?;
        let wire = entry_bytes
            .checked_add(70)
            .ok_or(RaftLogError::InvalidLimits)?;
        if entry_bytes == 0
            || append.0 == 0
            || read.0 == 0
            || recovery.0 == 0
            || workspace_bytes == 0
            || append.1 < owner
            || read.1 < owner
            || recovery.1 < wire
        {
            return Err(RaftLogError::InvalidLimits);
        }
        Ok(Self {
            entry_bytes,
            batch_entries: append.0,
            append_bytes: append.1,
            read_entries: read.0,
            read_bytes: read.1,
            recovery_entries: recovery.0,
            recovery_bytes: recovery.1,
            workspace_bytes,
        })
    }
    /// Reads the maximal opaque body size.
    #[must_use]
    pub const fn maximum_entry_bytes(self) -> usize {
        self.entry_bytes
    }
    /// Reads the input batch count cap.
    #[must_use]
    pub const fn maximum_batch_entries(self) -> usize {
        self.batch_entries
    }
    /// Reads A = 512 per input entry plus all body bytes.
    #[must_use]
    pub const fn maximum_append_bytes(self) -> usize {
        self.append_bytes
    }
    /// Reads the returned page count cap.
    #[must_use]
    pub const fn maximum_read_entries(self) -> usize {
        self.read_entries
    }
    /// Reads R = 512 per returned entry plus all body bytes.
    #[must_use]
    pub const fn maximum_read_bytes(self) -> usize {
        self.read_bytes
    }
    /// Reads the recovery/truncation census count cap.
    #[must_use]
    pub const fn maximum_recovery_entries(self) -> usize {
        self.recovery_entries
    }
    /// Reads the recovery/truncation full-wire byte cap.
    #[must_use]
    pub const fn maximum_recovery_bytes(self) -> usize {
        self.recovery_bytes
    }
    /// Reads the prospective logical owner peak cap.
    #[must_use]
    pub const fn maximum_workspace_bytes(self) -> usize {
        self.workspace_bytes
    }
}

/// Fixed operational diagnoses; never includes an opaque stored command.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RaftLogError {
    /// The explicit limits are invalid.
    InvalidLimits,
    /// The normalized half-open interval is inverted.
    InvalidRange,
    /// A logical owner or validation-work cap was exceeded.
    Limit,
    /// An admitted host reservation failed.
    Allocation,
    /// Persisted framing, inventory or log invariants are inconsistent.
    Corrupt,
    /// Caller namespace, vote or proposed log conflicts with durable data.
    Conflict,
    /// The engine cannot currently provide authority.
    Unavailable,
    /// Durable completion was not proved; reopen is required.
    Uncertain,
    /// This handle has been fenced after an operational failure.
    Fenced,
}

impl fmt::Display for RaftLogError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidLimits => "invalid durable log limits",
            Self::InvalidRange => "invalid durable log range",
            Self::Limit => "durable log limit",
            Self::Allocation => "durable log allocation failed",
            Self::Corrupt => "corrupt durable log",
            Self::Conflict => "durable log conflict",
            Self::Unavailable => "durable log unavailable",
            Self::Uncertain => "durable log commit uncertain",
            Self::Fenced => "durable log handle fenced",
        })
    }
}
impl std::error::Error for RaftLogError {}

/// Serialized synchronous durability capability, independent of a Raft library.
pub trait RaftLogStore {
    /// Borrows the fixed group namespace.
    fn group(&self) -> &GroupId;
    /// Borrows the fixed recovery namespace.
    fn recovery(&self) -> &RecoveryId;
    /// Reads validated scalar metadata.
    /// # Errors
    /// Refuses fenced, unavailable or inconsistent authority and profile limits.
    fn state(&mut self) -> Result<RaftLogState, RaftLogError>;
    /// Durably preserves a lexicographically nondecreasing vote.
    /// # Errors
    /// Refuses regression, operational failure or uncertain completion.
    fn save_vote(&mut self, vote: RaftVote) -> Result<(), RaftLogError>;
    /// Advances to an exact existing identity without regression.
    /// # Errors
    /// Refuses an absent/mismatched boundary, limits or operational failure.
    fn save_committed(&mut self, id: LogIdentity) -> Result<(), RaftLogError>;
    /// Moves an internally consecutive batch, comparing every existing overlap.
    /// Exact replay is idempotent; extension starts immediately after the tail.
    /// # Errors
    /// Refuses limits, gaps, identity/body/leader conflict or operational failure.
    fn append(&mut self, entries: Vec<RaftLogEntry>) -> Result<(), RaftLogError>;
    /// Reads one exact point after admitting 512 plus its actual body bytes.
    /// Ready/fence precedes cap checks; caps below 512 refuse before any probe.
    /// A 512-byte cap admits an empty body; None means genuine point absence.
    /// # Errors
    /// Refuses caller/backend owner limits before copying, or operational failure.
    fn read_entry(
        &mut self,
        index: u64,
        maximum_owned_bytes: usize,
    ) -> Result<Option<RaftLogEntry>, RaftLogError>;
    /// Returns a bounded consecutive prefix; empty proves interval exhaustion.
    /// # Errors
    /// Refuses inverted ranges, first-row limits, corruption or engine failure.
    fn read_page(
        &mut self,
        start: Bound<u64>,
        end: Bound<u64>,
    ) -> Result<Vec<RaftLogEntry>, RaftLogError>;
    /// Deletes only a fully validated suffix strictly above committed.
    /// # Errors
    /// Refuses a protected boundary, census limits or operational failure.
    fn truncate_from(&mut self, index: u64) -> Result<(), RaftLogError>;
    /// Atomically deletes an inclusive committed prefix and saves its anchor.
    /// The server must first prove provenance and durably pin the artifact in
    /// the separate state store; this primitive cannot certify that database.
    /// # Errors
    /// Refuses conflicting boundaries, validation limits or operational failure.
    fn purge_through(
        &mut self,
        position: LogIdentity,
        snapshot_id: [u8; 32],
    ) -> Result<(), RaftLogError>;
}
