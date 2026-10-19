//! Bounded checkpoint consistency for separate log and state owners.
//!
//! This no-snapshot check neither replays a committed suffix nor restores
//! effective membership. Its returned pair is not service-readiness authority.

use std::fmt;

use sha2::{Digest, Sha256};
use wf_domain::workflow::admission::AdmissionLimits;
use wf_ports::log::{RaftLogError, RaftLogStore};
use wf_ports::state::{GroupApplyStore, GroupApplyTransaction, StateTransactionError};
use wf_types::identity::{GroupId, RecoveryId};

/// Explicit sequential control and point-owner budgets, separate from engine RSS.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CheckpointJoinLimits {
    steps: usize,
    temporary: usize,
    entry: usize,
    workspace: usize,
}

impl CheckpointJoinLimits {
    /// Sets control steps, temporary C, point-owner J and total workspace W.
    /// A small positive W may refuse an otherwise healthy checkpoint.
    /// # Errors
    /// Refuses zero limits or a point-owner cap below its fixed 512-byte fee.
    pub fn new(
        maximum_read_steps: usize,
        maximum_temporary_bytes: usize,
        maximum_owned_entry_bytes: usize,
        workspace_bytes: usize,
    ) -> Result<Self, CheckpointJoinError> {
        if [maximum_read_steps, maximum_temporary_bytes, workspace_bytes].contains(&0)
            || maximum_owned_entry_bytes < 512
        {
            return Err(CheckpointJoinError::InvalidLimits);
        }
        Ok(Self {
            steps: maximum_read_steps,
            temporary: maximum_temporary_bytes,
            entry: maximum_owned_entry_bytes,
            workspace: workspace_bytes,
        })
    }

    /// Reads the cumulative control probe allowance.
    #[must_use]
    pub const fn maximum_read_steps(self) -> usize {
        self.steps
    }
    /// Reads cumulative temporary ownership during the control guard.
    #[must_use]
    pub const fn maximum_temporary_bytes(self) -> usize {
        self.temporary
    }
    /// Reads 512 plus the maximum permitted copied point body.
    #[must_use]
    pub const fn maximum_owned_entry_bytes(self) -> usize {
        self.entry
    }
    /// Reads the complete logical owner peak allowance.
    #[must_use]
    pub const fn workspace_bytes(self) -> usize {
        self.workspace
    }

    fn admit(self, group: &GroupId, recovery: &RecoveryId) -> Result<(), CheckpointJoinError> {
        let bytes = [
            512,
            group.as_bytes().len(),
            recovery.as_bytes().len(),
            512,
            group.as_bytes().len(),
            recovery.as_bytes().len(),
            2_048,
            512,
            self.temporary.max(self.entry),
        ]
        .into_iter()
        .try_fold(0_usize, usize::checked_add)
        .ok_or(CheckpointJoinError::Limit)?;
        if bytes > self.workspace {
            return Err(CheckpointJoinError::Limit);
        }
        Ok(())
    }
}

impl Default for CheckpointJoinLimits {
    fn default() -> Self {
        Self {
            steps: 1,
            temporary: 2_048,
            entry: 16_777_728,
            workspace: 268_435_456,
        }
    }
}

/// Startup failure; no variant retains a command body or business rejection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CheckpointJoinError {
    /// A required positive limit or minimum point-owner fee is invalid.
    InvalidLimits,
    /// Prospective combined ownership exceeds W or checked arithmetic overflows.
    Limit,
    /// The log owner differs from the independently supplied group/recovery.
    NamespaceMismatch,
    /// Applied state is incompatible with committed under the no-snapshot rule.
    IncompatibleCheckpoint,
    /// The applied point is absent despite the committed-prefix observation.
    MissingEntry,
    /// The applied point has a different complete term/node/index identity.
    IdentityMismatch,
    /// SHA256 of the actual point body differs from the prepared checkpoint.
    BodyMismatch,
    /// The log's existing operational diagnosis is preserved.
    Log(RaftLogError),
    /// The state's existing operational diagnosis is preserved.
    State(StateTransactionError),
}

impl fmt::Display for CheckpointJoinError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::InvalidLimits => "invalid checkpoint join limits",
            Self::Limit => "checkpoint join workspace limit",
            Self::NamespaceMismatch => "checkpoint join namespace mismatch",
            Self::IncompatibleCheckpoint => "incompatible applied and committed checkpoint",
            Self::MissingEntry => "applied checkpoint log entry missing",
            Self::IdentityMismatch => "applied checkpoint log identity mismatch",
            Self::BodyMismatch => "applied checkpoint body digest mismatch",
            Self::Log(error) => return fmt::Display::fmt(error, formatter),
            Self::State(error) => return fmt::Display::fmt(error, formatter),
        };
        formatter.write_str(message)
    }
}
impl std::error::Error for CheckpointJoinError {}

/// Checks an actual persisted boundary and returns only the supplied owners.
///
/// Every error drops both owners; reopen is required even for a healthy profile
/// refusal. Unapplied or behind state may succeed but still requires replay.
/// Snapshots/purge and membership recovery are not supported by this check.
/// Control-guard owners end before the bounded point read, so W reserves their
/// sequential maximum without refunds inside either active phase.
/// # Errors
/// Refuses namespaces, limits, operational failures or checkpoint inconsistencies.
pub fn verify_checkpoint<L: RaftLogStore, S: GroupApplyStore>(
    mut log: L,
    mut state: S,
    group: &GroupId,
    recovery: &RecoveryId,
    limits: CheckpointJoinLimits,
) -> Result<(L, S), CheckpointJoinError> {
    if log.group() != group || log.recovery() != recovery {
        return Err(CheckpointJoinError::NamespaceMismatch);
    }
    limits.admit(group, recovery)?;
    let control = {
        let admission =
            AdmissionLimits::new(1, 1, 1).map_err(|_| CheckpointJoinError::InvalidLimits)?;
        let tx = state
            .begin_apply(
                group,
                recovery,
                admission,
                limits.steps,
                512,
                limits.temporary,
            )
            .map_err(CheckpointJoinError::State)?;
        tx.control().map_err(CheckpointJoinError::State)?
    };
    let metadata = log.state().map_err(CheckpointJoinError::Log)?;
    if metadata.last_purged().is_some() {
        return Err(CheckpointJoinError::IncompatibleCheckpoint);
    }
    let Some(applied) = control.last_entry() else {
        return Ok((log, state));
    };
    let committed = metadata
        .committed()
        .ok_or(CheckpointJoinError::IncompatibleCheckpoint)?;
    if applied.log().index() > committed.index()
        || (applied.log().index() == committed.index() && applied.log() != committed)
    {
        return Err(CheckpointJoinError::IncompatibleCheckpoint);
    }
    {
        let entry = log
            .read_entry(applied.log().index().get(), limits.entry)
            .map_err(CheckpointJoinError::Log)?
            .ok_or(CheckpointJoinError::MissingEntry)?;
        if entry.id() != applied.log() {
            return Err(CheckpointJoinError::IdentityMismatch);
        }
        let actual: [u8; 32] = Sha256::digest(entry.body()).into();
        if &actual != applied.prepared().as_bytes() {
            return Err(CheckpointJoinError::BodyMismatch);
        }
    }
    Ok((log, state))
}
