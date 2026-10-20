//! Scalar transition preconditions and custody on the same snapshot owner.

use super::{SnapshotError, SnapshotLimits, SnapshotMetadata, StateSnapshotStore};
use crate::log::RaftLogState;
use wf_types::version::LogIdentity;

/// Checked mathematical input, without a consensus authority certificate.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SnapshotTransition {
    before: RaftLogState,
    position: LogIdentity,
    id: [u8; 32],
    received: bool,
}

fn leader(id: LogIdentity) -> (u64, u64) {
    (id.term(), id.leader_node())
}

fn valid_before(state: RaftLogState) -> bool {
    for (old, new) in [
        (state.committed(), state.last_log()),
        (state.last_purged(), state.committed()),
    ] {
        if let Some(old) = old {
            let Some(new) = new else {
                return false;
            };
            if old.index() > new.index()
                || leader(old) > leader(new)
                || (old.index() == new.index() && old != new)
            {
                return false;
            }
        }
    }
    true
}

impl SnapshotTransition {
    /// Prepares a locally proved, already committed snapshot boundary.
    /// # Errors
    /// Refuses a boundary outside the supplied committed and purged interval.
    pub fn local(
        before: RaftLogState,
        position: LogIdentity,
        id: [u8; 32],
    ) -> Result<Self, SnapshotError> {
        if !valid_before(before) {
            return Err(SnapshotError::TransitionConflict);
        }
        let committed = before
            .committed()
            .ok_or(SnapshotError::TransitionConflict)?;
        if position.index() > committed.index()
            || (position.index() == committed.index() && position != committed)
            || leader(position) > leader(committed)
        {
            return Err(SnapshotError::TransitionConflict);
        }
        if let Some(old) = before.last_purged()
            && (position.index() < old.index()
                || leader(position) < leader(old)
                || (position.index() == old.index()
                    && (position != old || before.purged_snapshot_id() != Some(id))))
        {
            return Err(SnapshotError::TransitionConflict);
        }
        Ok(Self {
            before,
            position,
            id,
            received: false,
        })
    }
    /// Carries a trusted caller's newer received snapshot decision.
    /// # Errors
    /// Refuses committed regression; this does not authenticate the caller.
    pub fn received(
        before: RaftLogState,
        position: LogIdentity,
        id: [u8; 32],
    ) -> Result<Self, SnapshotError> {
        if !valid_before(before) {
            return Err(SnapshotError::TransitionConflict);
        }
        for old in [before.committed(), before.last_purged()]
            .into_iter()
            .flatten()
        {
            if position.index() <= old.index() || leader(position) < leader(old) {
                return Err(SnapshotError::TransitionConflict);
            }
        }
        Ok(Self {
            before,
            position,
            id,
            received: true,
        })
    }
    /// Exact quiescent log precondition, never an independent checkpoint.
    #[must_use]
    pub const fn before(self) -> RaftLogState {
        self.before
    }
    /// Full target identity.
    #[must_use]
    pub const fn position(self) -> LogIdentity {
        self.position
    }
    /// Exact immutable canonical DATA identity.
    #[must_use]
    pub const fn id(&self) -> &[u8; 32] {
        &self.id
    }
    /// Whether the caller supplies received-snapshot authority.
    #[must_use]
    pub const fn is_received(self) -> bool {
        self.received
    }
}

/// Explicit catalog capability on the same exclusive state owner.
pub trait StateSnapshotTransitionStore: StateSnapshotStore {
    /// Ready-first scalar mode observation, with no database work or allocation.
    /// # Errors
    /// Reports actual fatal owner readiness failures.
    fn supports_snapshot_transitions(&self) -> Result<bool, SnapshotError>;
    /// Retrieves original immutable DATA by exact identity.
    /// # Errors
    /// Refuses unsupported layout, storage, caller output or resource failures.
    fn snapshot_by_id<W: std::io::Write>(
        &mut self,
        expected_id: [u8; 32],
        output: &mut W,
        limits: SnapshotLimits,
    ) -> Result<Option<SnapshotMetadata>, SnapshotError>;
    /// Observes the durable scalar intent without materializing membership.
    /// # Errors
    /// Refuses unsupported layout, storage or resource failures.
    fn snapshot_transition(
        &mut self,
        limits: SnapshotLimits,
    ) -> Result<Option<SnapshotTransition>, SnapshotError>;
    /// Durably retains the local target before separate log publication.
    /// # Errors
    /// Refuses conflicting facts, another intent or operational failures.
    fn prepare_snapshot_transition(
        &mut self,
        transition: SnapshotTransition,
        limits: SnapshotLimits,
    ) -> Result<(), SnapshotError>;
    /// Atomically publishes received LIVE, immutable DATA and authorized intent.
    /// # Errors
    /// Preserves candidate versus active storage origin and existing limits.
    fn prepare_received_snapshot(
        &mut self,
        stage: Self::Stage,
        transition: SnapshotTransition,
        limits: SnapshotLimits,
    ) -> Result<(), SnapshotError>;
    /// Retires only the non-target generation after exact log acknowledgement.
    /// # Errors
    /// Refuses a stale acknowledgement, storage or resource failure.
    fn finish_snapshot_transition(
        &mut self,
        observed_log: RaftLogState,
        limits: SnapshotLimits,
    ) -> Result<(), SnapshotError>;
}
