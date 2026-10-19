//! Snapshot provenance and a durable state pin before a separate log transaction.

use super::{CommittedRecoveryError, CommittedRecoveryLimits, join};
use wf_domain::workflow::control::{ControlState, EntryIdentity};
use wf_ports::log::{RaftLogState, RaftLogStore};
use wf_ports::snapshot::{SnapshotError, SnapshotLimits, StateSnapshotStore};
use wf_storage::{log::RedbLogStore, state::RedbStateStore};
use wf_types::identity::{GroupId, RecoveryId};

mod provenance;
mod replay;

/// Fixed failures; every error consumes both owners, preserving any durable prefix.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SnapshotRecoveryError {
    /// A required retrievable artifact is absent.
    MissingSnapshot,
    /// An unapplied artifact cannot authorize local log deletion.
    UnappliedSnapshot,
    /// Present DATA, local pin and log anchor do not describe the same boundary.
    AnchorMismatch,
    /// Existing snapshot storage or profile diagnosis.
    Snapshot(SnapshotError),
    /// Existing committed audit, replay or resource diagnosis.
    Recovery(CommittedRecoveryError),
}

impl std::fmt::Display for SnapshotRecoveryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingSnapshot => f.write_str("recovery snapshot missing"),
            Self::UnappliedSnapshot => f.write_str("recovery snapshot unapplied"),
            Self::AnchorMismatch => f.write_str("recovery snapshot anchor mismatch"),
            Self::Snapshot(error) => std::fmt::Display::fmt(error, f),
            Self::Recovery(error) => std::fmt::Display::fmt(error, f),
        }
    }
}
impl std::error::Error for SnapshotRecoveryError {}
impl From<SnapshotError> for SnapshotRecoveryError {
    fn from(error: SnapshotError) -> Self {
        Self::Snapshot(error)
    }
}
impl From<CommittedRecoveryError> for SnapshotRecoveryError {
    fn from(error: CommittedRecoveryError) -> Self {
        Self::Recovery(error)
    }
}

fn namespace(
    log: &RedbLogStore,
    state: &RedbStateStore,
    expected: (&GroupId, &RecoveryId),
) -> Result<usize, SnapshotRecoveryError> {
    if log.group() != expected.0
        || log.recovery() != expected.1
        || state.group() != expected.0
        || state.recovery() != expected.1
    {
        return Err(CommittedRecoveryError::NamespaceMismatch.into());
    }
    Ok(CommittedRecoveryLimits::base(expected.0, expected.1)?)
}

fn phase(
    limits: CommittedRecoveryLimits,
    snapshot: SnapshotLimits,
    base: usize,
) -> Result<(), SnapshotRecoveryError> {
    Ok(limits.admit(&[
        base,
        snapshot.maximum_metadata_bytes(),
        snapshot.maximum_temporary_bytes(),
    ])?)
}

fn anchor(
    log: RaftLogState,
    control: ControlState,
    id: [u8; 32],
    pin: Option<[u8; 32]>,
) -> Result<bool, SnapshotRecoveryError> {
    if let Some(position) = log.last_purged() {
        if control.last_entry().map(EntryIdentity::log) != Some(position)
            || log.purged_snapshot_id() != Some(id)
            || pin != Some(id)
        {
            return Err(SnapshotRecoveryError::AnchorMismatch);
        }
        return Ok(true);
    }
    if pin.is_some_and(|pinned| pinned != id) {
        return Err(SnapshotRecoveryError::AnchorMismatch);
    }
    Ok(false)
}

/// Proves the local snapshot against committed history, pins it, then purges.
/// This fixed-pin predecessor cannot advance or replace an existing purge base.
/// # Errors
/// Consumes both owners on every profile, provenance or operational refusal.
pub fn purge_snapshot(
    mut log: RedbLogStore,
    mut state: RedbStateStore,
    group: &GroupId,
    recovery: &RecoveryId,
    limits: CommittedRecoveryLimits,
    snapshot_limits: SnapshotLimits,
) -> Result<(RedbLogStore, RedbStateStore), SnapshotRecoveryError> {
    let base = namespace(&log, &state, (group, recovery))?;
    let metadata = log.state().map_err(CommittedRecoveryError::Log)?;
    phase(limits, snapshot_limits, base)?;
    let snapshot = state
        .current_snapshot(&mut std::io::sink(), snapshot_limits)?
        .ok_or(SnapshotRecoveryError::MissingSnapshot)?;
    let saved = snapshot.control();
    let member = snapshot
        .membership()
        .entry()
        .zip(snapshot.membership().stamp());
    let id = *snapshot.id();
    drop(snapshot);
    phase(limits, snapshot_limits, base)?;
    let pin = state.snapshot_pin(snapshot_limits)?;
    let anchored = anchor(metadata, saved, id, pin)?;
    let position = saved
        .last_entry()
        .ok_or(SnapshotRecoveryError::UnappliedSnapshot)?
        .log();
    let mut current_member = None;
    let current = join::observe(
        &mut state,
        (group, recovery),
        limits,
        base,
        &mut current_member,
    )?;
    provenance::boundary([saved, current], metadata, limits, true)?;
    let mut work = [0, limits.maximum_log_body_bytes()];
    provenance::audit(
        &mut log,
        (group, recovery),
        limits,
        [saved, current],
        [member, current_member],
        &mut work,
        (base, anchored),
    )?;
    if log
        .state()
        .map_err(CommittedRecoveryError::Log)?
        .committed()
        != metadata.committed()
    {
        return Err(CommittedRecoveryError::IncompatibleCheckpoint.into());
    }
    phase(limits, snapshot_limits, base)?;
    state.pin_snapshot(id, snapshot_limits)?;
    log.purge_through(position, id)
        .map_err(CommittedRecoveryError::Log)?;
    Ok((log, state))
}

/// Audits from the exact frozen base and replays only the durable committed tail.
/// No snapshot or purge is manufactured; unpurged absence uses the old recovery.
/// # Errors
/// Drops both owners; a completed replay prefix remains durable for fresh reopen.
pub fn recover_snapshotted(
    mut log: RedbLogStore,
    mut state: RedbStateStore,
    group: &GroupId,
    recovery: &RecoveryId,
    limits: CommittedRecoveryLimits,
    snapshot_limits: SnapshotLimits,
) -> Result<(RedbLogStore, RedbStateStore), SnapshotRecoveryError> {
    let base = namespace(&log, &state, (group, recovery))?;
    let metadata = log.state().map_err(CommittedRecoveryError::Log)?;
    phase(limits, snapshot_limits, base)?;
    let Some(snapshot) = state.current_snapshot(&mut std::io::sink(), snapshot_limits)? else {
        if metadata.last_purged().is_some() {
            return Err(SnapshotRecoveryError::MissingSnapshot);
        }
        return Ok(super::recover_committed(
            log, state, group, recovery, limits,
        )?);
    };
    let saved = snapshot.control();
    let member = snapshot
        .membership()
        .entry()
        .zip(snapshot.membership().stamp());
    let id = *snapshot.id();
    drop(snapshot);
    phase(limits, snapshot_limits, base)?;
    let pin = state.snapshot_pin(snapshot_limits)?;
    let anchored = anchor(metadata, saved, id, pin)?;
    let mut current_member = None;
    let current = join::observe(
        &mut state,
        (group, recovery),
        limits,
        base,
        &mut current_member,
    )?;
    provenance::boundary([saved, current], metadata, limits, false)?;
    let mut work = [0, limits.maximum_log_body_bytes()];
    provenance::audit(
        &mut log,
        (group, recovery),
        limits,
        [saved, current],
        [member, current_member],
        &mut work,
        (base, anchored),
    )?;
    replay::apply(
        &mut log,
        &mut state,
        (group, recovery),
        limits,
        (base, metadata.committed()),
        (current, current_member),
        &mut work,
    )?;
    Ok((log, state))
}
