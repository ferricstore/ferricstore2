//! Trusted received installation; integrity alone never supplies commitment.

use super::super::join;
use super::{
    CommittedRecoveryError, CommittedRecoveryLimits, SnapshotRecoveryError as Error, advance,
    phase, provenance, replay,
};
use wf_domain::workflow::control::{ControlState, EntryIdentity, EntryStamp};
use wf_ports::{
    log::{RaftLogState, RaftLogStore},
    snapshot::{
        SnapshotError, SnapshotLimits, SnapshotTransition, StateSnapshotStore,
        StateSnapshotTransitionStore,
    },
};
use wf_storage::{log::RedbLogStore, snapshot::RedbSnapshotStage, state::RedbStateStore};
use wf_types::identity::{GroupId, RecoveryId};

fn stage_namespace(
    candidate: &RedbSnapshotStage,
    namespace: (&GroupId, &RecoveryId),
) -> Result<usize, Error> {
    if candidate.group() != namespace.0 || candidate.recovery() != namespace.1 {
        return Err(CommittedRecoveryError::NamespaceMismatch.into());
    }
    Ok(super::super::total(&[
        512,
        candidate.group().as_bytes().len(),
        candidate.recovery().as_bytes().len(),
    ])?)
}

pub(super) fn matching(
    log: &mut RedbLogStore,
    namespace: (&GroupId, &RecoveryId),
    limits: CommittedRecoveryLimits,
    base: usize,
    saved: ControlState,
    before: RaftLogState,
    work: &mut [usize; 2],
) -> Result<(), Error> {
    let position = saved.last_entry().ok_or(Error::UnappliedSnapshot)?.log();
    if before
        .last_log()
        .is_some_and(|tail| tail.index() >= position.index())
        && before
            .last_purged()
            .is_none_or(|p| p.index() < position.index())
    {
        let entry = join::point(log, position.index().get(), limits, base, work)?;
        if entry.id() == position {
            let mut member = None;
            join::audit(&entry, namespace, limits, base, saved, &mut member)?;
        }
    }
    Ok(())
}

fn retry(
    log: &mut RedbLogStore,
    state: &mut RedbStateStore,
    namespace: (&GroupId, &RecoveryId),
    limits: CommittedRecoveryLimits,
    snapshot: SnapshotLimits,
    initial: (ControlState, Option<(EntryIdentity, EntryStamp)>),
    context: (usize, &mut [usize; 2]),
) -> Result<(), Error> {
    let (base, work) = context;
    let metadata = log.state().map_err(CommittedRecoveryError::Log)?;
    let id = metadata.purged_snapshot_id().ok_or(Error::AnchorMismatch)?;
    let mut member = None;
    let mut actual = [0; 32];
    let saved = advance::data(
        state,
        Some(id),
        limits,
        snapshot,
        base,
        &mut member,
        &mut actual,
    )?
    .ok_or(Error::MissingSnapshot)?;
    if saved != initial.0 || member != initial.1 || actual != id {
        return Err(Error::AnchorMismatch);
    }
    phase(limits, snapshot, base)?;
    if state.snapshot_pin(snapshot)? != Some(id) {
        return Err(Error::AnchorMismatch);
    }
    let mut current_member = None;
    let current = join::observe(state, namespace, limits, base, &mut current_member)?;
    provenance::boundary([saved, current], metadata, limits, false)?;
    provenance::from(
        log,
        namespace,
        limits,
        [saved, current],
        &[member, member, current_member],
        work,
        (base, metadata.last_purged()),
    )?;
    replay::apply(
        log,
        state,
        namespace,
        limits,
        (base, metadata.committed()),
        (current, current_member),
        work,
    )?;
    Ok(())
}

/// Installs a caller-authorized newer snapshot before acknowledging its log base.
/// The caller must bind actual accepted consensus metadata to this original DATA.
/// # Errors
/// Consumes all owners; earlier durable intent/publication survives for recovery.
pub fn install_received_snapshot(
    mut log: RedbLogStore,
    mut state: RedbStateStore,
    mut candidate: RedbSnapshotStage,
    group: &GroupId,
    recovery: &RecoveryId,
    limits: CommittedRecoveryLimits,
    snapshot: SnapshotLimits,
) -> Result<(RedbLogStore, RedbStateStore), Error> {
    let namespace = (group, recovery);
    let base = super::namespace(&log, &state, namespace)?;
    let stage_owner = stage_namespace(&candidate, namespace)?;
    if !state.supports_snapshot_transitions()? {
        return Err(SnapshotError::State(
            wf_ports::state::StateTransactionError::UnsupportedLayout,
        )
        .into());
    }
    let base = super::super::total(&[base, 512, stage_owner])?;
    phase(limits, snapshot, base)?;
    if state.snapshot_transition(snapshot)?.is_some() {
        return Err(SnapshotError::TransitionPending.into());
    }
    let before = log.state().map_err(CommittedRecoveryError::Log)?;
    phase(limits, snapshot, base)?;
    let metadata = candidate.metadata(snapshot)?;
    let saved = metadata.control();
    let id = *metadata.id();
    let member = metadata
        .membership()
        .entry()
        .zip(metadata.membership().stamp());
    drop(metadata);
    let entry = saved.last_entry().ok_or(Error::UnappliedSnapshot)?;
    let mut work = [0, limits.maximum_log_body_bytes()];
    if before.last_purged() == Some(entry.log()) && before.purged_snapshot_id() == Some(id) {
        retry(
            &mut log,
            &mut state,
            namespace,
            limits,
            snapshot,
            (saved, member),
            (base, &mut work),
        )?;
        return Ok((log, state));
    }
    let transition = SnapshotTransition::received(before, entry.log(), id)?;
    let mut current_member = None;
    let current = join::observe(&mut state, namespace, limits, base, &mut current_member)?;
    if let Some(applied) = current.last_entry() {
        let committed = before
            .committed()
            .ok_or(CommittedRecoveryError::IncompatibleCheckpoint)?;
        if applied.log().index() > committed.index()
            || (applied.log().index() == committed.index() && applied.log() != committed)
        {
            return Err(CommittedRecoveryError::IncompatibleCheckpoint.into());
        }
    }
    matching(&mut log, namespace, limits, base, saved, before, &mut work)?;
    phase(limits, snapshot, base)?;
    state.prepare_received_snapshot(candidate, transition, snapshot)?;
    log.install_snapshot_boundary(before, entry.log(), *entry.prepared().as_bytes(), id)
        .map_err(CommittedRecoveryError::Log)?;
    let observed = log.state().map_err(CommittedRecoveryError::Log)?;
    if !advance::acknowledged(observed, transition) {
        return Err(Error::AnchorMismatch);
    }
    phase(limits, snapshot, base)?;
    state.finish_snapshot_transition(observed, snapshot)?;
    Ok((log, state))
}

/// Moves a private original DATA store into a fresh unpurged log/state pair.
/// Trusted accepted-snapshot authority remains the caller's responsibility.
/// # Errors
/// Drops owners on failure; a purged log requires its original state/artifact.
pub fn restore_received_snapshot(
    mut log: RedbLogStore,
    mut candidate: RedbSnapshotStage,
    group: &GroupId,
    recovery: &RecoveryId,
    limits: CommittedRecoveryLimits,
    snapshot: SnapshotLimits,
) -> Result<(RedbLogStore, RedbStateStore), Error> {
    let namespace = (group, recovery);
    stage_namespace(&candidate, namespace)?;
    if log.group() != group || log.recovery() != recovery {
        return Err(CommittedRecoveryError::NamespaceMismatch.into());
    }
    let base = super::super::total(&[CommittedRecoveryLimits::base(group, recovery)?, 512])?;
    phase(limits, snapshot, base)?;
    let before = log.state().map_err(CommittedRecoveryError::Log)?;
    if before.last_purged().is_some() {
        return Err(Error::MissingSnapshot);
    }
    let metadata = candidate.metadata(snapshot)?;
    let saved = metadata.control();
    let id = *metadata.id();
    drop(metadata);
    let entry = saved.last_entry().ok_or(Error::UnappliedSnapshot)?;
    let transition = SnapshotTransition::received(before, entry.log(), id)?;
    let mut work = [0, limits.maximum_log_body_bytes()];
    matching(&mut log, namespace, limits, base, saved, before, &mut work)?;
    let state = candidate.into_store()?;
    phase(limits, snapshot, base)?;
    let mut state = state.upgrade_snapshot_layout(snapshot)?;
    phase(limits, snapshot, base)?;
    state.prepare_snapshot_transition(transition, snapshot)?;
    log.install_snapshot_boundary(before, entry.log(), *entry.prepared().as_bytes(), id)
        .map_err(CommittedRecoveryError::Log)?;
    let observed = log.state().map_err(CommittedRecoveryError::Log)?;
    if !advance::acknowledged(observed, transition) {
        return Err(Error::AnchorMismatch);
    }
    phase(limits, snapshot, base)?;
    state.finish_snapshot_transition(observed, snapshot)?;
    Ok((log, state))
}
