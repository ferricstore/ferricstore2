//! Bounded immutable state snapshots, without commitment or purge authority.

use crate::state::StateTransactionError;

mod metadata;
pub use metadata::SnapshotMetadata;
mod transition;
pub use transition::{SnapshotTransition, StateSnapshotTransitionStore};

/// Explicit data, cumulative work, caller-I/O and first-party owner ceilings.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SnapshotLimits {
    data: (usize, usize, usize),
    work: (usize, usize, usize),
    io: (usize, usize),
    owners: (usize, usize),
    workspace: usize,
}

impl SnapshotLimits {
    /// Creates positive limits. Small positive workspace remains valid.
    /// # Errors
    /// Returns `InvalidLimits` if any ceiling is zero.
    pub fn new(
        data: (usize, usize, usize),
        work: (usize, usize, usize),
        io: (usize, usize),
        owners: (usize, usize),
        workspace_bytes: usize,
    ) -> Result<Self, SnapshotError> {
        if [
            data.0,
            data.1,
            data.2,
            work.0,
            work.1,
            work.2,
            io.0,
            io.1,
            owners.0,
            owners.1,
            workspace_bytes,
        ]
        .contains(&0)
        {
            return Err(SnapshotError::InvalidLimits);
        }
        Ok(Self {
            data,
            work,
            io,
            owners,
            workspace: workspace_bytes,
        })
    }
    /// Maximum artifact rows, including the two metadata rows.
    #[must_use]
    pub const fn maximum_rows(self) -> usize {
        self.data.0
    }
    /// Maximum complete canonical stream bytes, including framing and footer.
    #[must_use]
    pub const fn maximum_encoded_bytes(self) -> usize {
        self.data.1
    }
    /// Maximum simultaneous physical key plus value bytes.
    #[must_use]
    pub const fn maximum_row_bytes(self) -> usize {
        self.data.2
    }
    /// Maximum reached rows, including revisits and retirement.
    #[must_use]
    pub const fn maximum_visited_rows(self) -> usize {
        self.work.0
    }
    /// Maximum reached full key/value bytes, including revisits.
    #[must_use]
    pub const fn maximum_visited_bytes(self) -> usize {
        self.work.1
    }
    /// Maximum open, probe, advance and mutation steps.
    #[must_use]
    pub const fn maximum_steps(self) -> usize {
        self.work.2
    }
    /// Maximum underlying I/O attempts, including Interrupted and EOF.
    #[must_use]
    pub const fn maximum_io_calls(self) -> usize {
        self.io.0
    }
    /// Maximum slice length passed to each underlying I/O call.
    #[must_use]
    pub const fn maximum_io_bytes(self) -> usize {
        self.io.1
    }
    /// Complete returned metadata owner allowance.
    #[must_use]
    pub const fn maximum_metadata_bytes(self) -> usize {
        self.owners.0
    }
    /// Cumulative first-party temporary owner allowance.
    #[must_use]
    pub const fn maximum_temporary_bytes(self) -> usize {
        self.owners.1
    }
    /// Complete logical first-party workspace allowance.
    #[must_use]
    pub const fn workspace_bytes(self) -> usize {
        self.workspace
    }
}

impl Default for SnapshotLimits {
    fn default() -> Self {
        Self {
            data: (1_000_000, 1_073_741_824, 67_108_864),
            work: (8_000_000, 4_000_000_000, 100_000_000),
            io: (10_000_000, 65_536),
            owners: (134_217_728, 134_217_728),
            workspace: 536_870_912,
        }
    }
}

/// Fixed snapshot diagnoses, separating candidate storage from active authority.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SnapshotError {
    /// A configured ceiling is zero.
    InvalidLimits,
    /// A healthy operation exceeds its explicit resource or data profile.
    Limit,
    /// A fallible first-party allocation failed.
    Allocation,
    /// Incoming framing, checksum or mathematical authority is invalid.
    Malformed,
    /// The stream uses an unsupported version.
    UnsupportedVersion,
    /// The stream does not match the independently supplied namespace.
    NamespaceMismatch,
    /// Requested artifact ceilings exceed the store's captured profile.
    ProfileMismatch,
    /// Immutable genesis differs from the live target.
    GenesisMismatch,
    /// Live installation would regress or conflict with applied control.
    StaleSnapshot,
    /// The sole artifact is pinned and cannot yet be replaced or cleared.
    SnapshotPinned,
    /// A durable transition retains custody until its log acknowledgement.
    TransitionPending,
    /// A supplied transition or acknowledgement conflicts with durable facts.
    TransitionConflict,
    /// Live installation would regress or conflict with applied membership.
    MembershipMismatch,
    /// A caller input failed.
    InputUnavailable,
    /// A caller output failed or made no progress.
    OutputUnavailable,
    /// A private candidate or stage-source operation failed.
    Stage(StateTransactionError),
    /// An active authority operation failed.
    State(StateTransactionError),
}

impl std::fmt::Display for SnapshotError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::InvalidLimits => "invalid snapshot limits",
            Self::Limit => "snapshot profile limit",
            Self::Allocation => "snapshot allocation failure",
            Self::Malformed => "malformed snapshot",
            Self::UnsupportedVersion => "unsupported snapshot version",
            Self::NamespaceMismatch => "snapshot namespace mismatch",
            Self::ProfileMismatch => "snapshot profile mismatch",
            Self::GenesisMismatch => "snapshot genesis mismatch",
            Self::StaleSnapshot => "stale snapshot",
            Self::SnapshotPinned => "snapshot pinned",
            Self::TransitionPending => "snapshot transition pending",
            Self::TransitionConflict => "snapshot transition conflict",
            Self::MembershipMismatch => "snapshot membership mismatch",
            Self::InputUnavailable => "snapshot input unavailable",
            Self::OutputUnavailable => "snapshot output unavailable",
            Self::Stage(_) => "snapshot stage storage failure",
            Self::State(_) => "snapshot active storage failure",
        })
    }
}
impl std::error::Error for SnapshotError {}

/// Serialized snapshot custody on the same authoritative state owner.
/// Caller I/O buffering and responsiveness are outside logical workspace limits.
pub trait StateSnapshotStore {
    /// A private validated candidate, without raw database or apply access.
    type Stage;
    /// Captures immutable data; output failure aborts artifact replacement.
    /// Accepted output bytes do not imply caller flush or fsync.
    /// # Errors
    /// Refuses profile, output or actual active-storage failures.
    fn capture_snapshot<W: std::io::Write>(
        &mut self,
        output: &mut W,
        limits: SnapshotLimits,
    ) -> Result<SnapshotMetadata, SnapshotError>;
    /// Exports the original retained artifact, regardless of later live applies.
    /// # Errors
    /// Refuses profile, output or actual active-storage failures.
    fn current_snapshot<W: std::io::Write>(
        &mut self,
        output: &mut W,
        limits: SnapshotLimits,
    ) -> Result<Option<SnapshotMetadata>, SnapshotError>;
    /// Atomically installs a consumed candidate without live control rollback.
    /// # Errors
    /// Refuses lineage, profiles or precisely classified stage/active failures.
    fn install_snapshot(
        &mut self,
        stage: Self::Stage,
        limits: SnapshotLimits,
    ) -> Result<SnapshotMetadata, SnapshotError>;
    /// Durably retains the exact validated current artifact before log purge.
    /// This local pin does not establish Raft commitment or lineage.
    /// # Errors
    /// Refuses an absent/different artifact, profiles or active storage failure.
    fn pin_snapshot(
        &mut self,
        expected_id: [u8; 32],
        limits: SnapshotLimits,
    ) -> Result<(), SnapshotError>;
    /// Reads the validated local pin without materializing snapshot metadata.
    /// # Errors
    /// Refuses profiles, malformed authority or active storage failure.
    fn snapshot_pin(&mut self, limits: SnapshotLimits) -> Result<Option<[u8; 32]>, SnapshotError>;
}
