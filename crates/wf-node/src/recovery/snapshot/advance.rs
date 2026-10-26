//! Repeated local compaction and intent-first restart under one point account.

use super::super::join;
use super::{
    CommittedRecoveryError, CommittedRecoveryLimits, SnapshotRecoveryError as Error, phase,
    provenance, replay,
};
use wf_domain::workflow::control::{ControlState, EntryIdentity, EntryStamp};
use wf_ports::{
    log::{RaftLogState, RaftLogStore},
    snapshot::{
        SnapshotLimits, SnapshotTransition, StateSnapshotStore, StateSnapshotTransitionStore,
    },
};
use wf_storage::{log::RedbLogStore, state::RedbStateStore};
use wf_types::identity::{GroupId, RecoveryId};

pub(super) fn data(
    state: &mut RedbStateStore,
    expected: Option<[u8; 32]>,
    limits: &CommittedRecoveryLimits,
    snapshot: SnapshotLimits,
    base: usize,
    member: &mut Option<(EntryIdentity, EntryStamp)>,
    id: &mut [u8; 32],
) -> Result<Option<ControlState>, Error> {
    phase(limits, snapshot, base)?;
    let observed = if let Some(id) = expected {
        state.snapshot_by_id(id, &mut std::io::sink(), snapshot)?
    } else {
        state.current_snapshot(&mut std::io::sink(), snapshot)?
    };
    let Some(observed) = observed else {
        return Ok(None);
    };
    let control = observed.control();
    *member = observed
        .membership()
        .entry()
        .zip(observed.membership().stamp());
    *id = *observed.id();
    drop(observed);
    Ok(Some(control))
}

pub(super) fn acknowledged(observed: RaftLogState, t: SnapshotTransition) -> bool {
    observed.last_purged() == Some(t.position())
        && observed.purged_snapshot_id() == Some(*t.id())
        && observed.vote() == t.before().vote()
        && if t.is_received() {
            observed.committed() == Some(t.position())
                && (observed.last_log() == Some(t.position())
                    || observed.last_log() == t.before().last_log())
        } else {
            observed.committed() == t.before().committed()
                && observed.last_log() == t.before().last_log()
        }
}

pub(super) fn run(
    mut log: RedbLogStore,
    mut state: RedbStateStore,
    namespace: (&GroupId, &RecoveryId),
    limits: &CommittedRecoveryLimits,
    snapshot: SnapshotLimits,
    purge: bool,
    base: usize,
) -> Result<(RedbLogStore, RedbStateStore), Error> {
    let base = super::super::total(&[base, 512])?;
    phase(limits, snapshot, base)?;
    let pending = state.snapshot_transition(snapshot)?;
    let mut metadata = log.state().map_err(CommittedRecoveryError::Log)?;
    let mut work = [0, limits.work.1];
    let (mut member, mut id) = (None, [0; 32]);
    let saved = data(
        &mut state,
        pending.map(|t| *t.id()),
        limits,
        snapshot,
        base,
        &mut member,
        &mut id,
    )?;
    if let Some(t) = pending {
        let saved = saved.ok_or(Error::MissingSnapshot)?;
        if resume_pending(
            &mut log,
            &mut state,
            namespace,
            &(limits, snapshot),
            &(t, saved),
            (base, &mut metadata, &mut work),
        )? {
            return recover(
                log,
                state,
                namespace,
                limits,
                (base, purge),
                (saved, member),
                &mut work,
            );
        }
    }
    let mut current_member = None;
    let current = join::observe(&mut state, namespace, limits, base, &mut current_member)?;
    let Some(saved) = saved else {
        if purge || metadata.last_purged().is_some() {
            return Err(Error::MissingSnapshot);
        }
        plain(
            &mut log,
            &mut state,
            namespace,
            limits,
            (base, metadata),
            (current, current_member),
            &mut work,
        )?;
        return Ok((log, state));
    };
    if purge && metadata.last_purged().is_none() && saved.last_entry().is_none() {
        return Err(Error::UnappliedSnapshot);
    }
    let mut seed = None;
    old_base(
        &mut state,
        metadata,
        &(limits, snapshot),
        base,
        (saved, id),
        &mut seed,
    )?;
    provenance::boundary([saved, current], metadata, limits, purge)?;
    provenance::from(
        &mut log,
        namespace,
        limits,
        [saved, current],
        &[seed, member, current_member],
        &mut work,
        (base, metadata.last_purged()),
    )?;
    if log.state().map_err(CommittedRecoveryError::Log)? != metadata {
        return Err(Error::AnchorMismatch);
    }
    if purge || pending.is_some() {
        publish(
            &mut log,
            &mut state,
            &(metadata, saved, id),
            &(limits, snapshot),
            base,
            pending,
        )?;
    }
    if !purge {
        replay::apply(
            &mut log,
            &mut state,
            namespace,
            limits,
            (base, metadata.committed()),
            (current, current_member),
            &mut work,
        )?;
    }
    Ok((log, state))
}

fn resume_pending(
    log: &mut RedbLogStore,
    state: &mut RedbStateStore,
    namespace: (&GroupId, &RecoveryId),
    profiles: &(&CommittedRecoveryLimits, SnapshotLimits),
    initial: &(SnapshotTransition, ControlState),
    context: (usize, &mut RaftLogState, &mut [usize; 2]),
) -> Result<bool, Error> {
    let (limits, snapshot) = *profiles;
    let (t, saved) = *initial;
    let (base, metadata, work) = context;
    if *metadata == t.before() {
        if t.is_received() {
            super::received::matching(log, namespace, limits, base, saved, *metadata, work)?;
            log.install_snapshot_boundary(
                t.before(),
                t.position(),
                *saved
                    .last_entry()
                    .ok_or(Error::UnappliedSnapshot)?
                    .prepared()
                    .as_bytes(),
                *t.id(),
            )
            .map_err(CommittedRecoveryError::Log)?;
            *metadata = log.state().map_err(CommittedRecoveryError::Log)?;
        }
    } else if !acknowledged(*metadata, t) {
        return Err(Error::AnchorMismatch);
    }
    if acknowledged(*metadata, t) {
        phase(limits, snapshot, base)?;
        state.finish_snapshot_transition(*metadata, snapshot)?;
        return Ok(true);
    }
    Ok(false)
}

fn old_base(
    state: &mut RedbStateStore,
    metadata: RaftLogState,
    profiles: &(&CommittedRecoveryLimits, SnapshotLimits),
    base: usize,
    candidate: (ControlState, [u8; 32]),
    seed: &mut Option<(EntryIdentity, EntryStamp)>,
) -> Result<(), Error> {
    let (limits, snapshot) = *profiles;
    if let Some(p) = metadata.last_purged() {
        let old_id = metadata.purged_snapshot_id().ok_or(Error::AnchorMismatch)?;
        let mut actual = [0; 32];
        let old = data(
            state,
            Some(old_id),
            limits,
            snapshot,
            base,
            seed,
            &mut actual,
        )?
        .ok_or(Error::MissingSnapshot)?;
        if old.last_entry().map(EntryIdentity::log) != Some(p) || actual != old_id {
            return Err(Error::AnchorMismatch);
        }
        phase(limits, snapshot, base)?;
        if state.snapshot_pin(snapshot)? != Some(old_id) {
            return Err(Error::AnchorMismatch);
        }
        let entry = candidate.0.last_entry().ok_or(Error::AnchorMismatch)?;
        if entry.log().index() < p.index()
            || (entry.log().index() == p.index() && (candidate.0 != old || candidate.1 != old_id))
        {
            return Err(Error::AnchorMismatch);
        }
        if entry.log().index() > p.index()
            && (candidate.0.time().hlc() < old.time().hlc()
                || candidate.0.time().wall() < old.time().wall())
        {
            return Err(CommittedRecoveryError::ClockMismatch.into());
        }
    }
    Ok(())
}

fn publish(
    log: &mut RedbLogStore,
    state: &mut RedbStateStore,
    initial: &(RaftLogState, ControlState, [u8; 32]),
    profiles: &(&CommittedRecoveryLimits, SnapshotLimits),
    base: usize,
    pending: Option<SnapshotTransition>,
) -> Result<(), Error> {
    let (limits, snapshot) = *profiles;
    let (metadata, saved, id) = *initial;
    let position = saved.last_entry().ok_or(Error::UnappliedSnapshot)?.log();
    let t = SnapshotTransition::local(metadata, position, id)?;
    if pending.is_some_and(|old| old != t) {
        return Err(Error::AnchorMismatch);
    }
    if pending.is_none() {
        phase(limits, snapshot, base)?;
        state.prepare_snapshot_transition(t, snapshot)?;
    }
    log.purge_through(t.position(), *t.id())
        .map_err(CommittedRecoveryError::Log)?;
    let observed = log.state().map_err(CommittedRecoveryError::Log)?;
    if !acknowledged(observed, t) {
        return Err(Error::AnchorMismatch);
    }
    phase(limits, snapshot, base)?;
    state.finish_snapshot_transition(observed, snapshot)?;
    Ok(())
}

fn plain(
    log: &mut RedbLogStore,
    state: &mut RedbStateStore,
    namespace: (&GroupId, &RecoveryId),
    limits: &CommittedRecoveryLimits,
    context: (usize, RaftLogState),
    initial: (ControlState, Option<(EntryIdentity, EntryStamp)>),
    work: &mut [usize; 2],
) -> Result<(), Error> {
    let (base, metadata) = context;
    let (current, member) = initial;
    provenance::boundary([current, current], metadata, limits, false)?;
    provenance::from(
        log,
        namespace,
        limits,
        [current, current],
        &[None, member, member],
        work,
        (base, None),
    )?;
    replay::apply(
        log,
        state,
        namespace,
        limits,
        (base, metadata.committed()),
        (current, member),
        work,
    )?;
    Ok(())
}

fn recover(
    mut log: RedbLogStore,
    mut state: RedbStateStore,
    namespace: (&GroupId, &RecoveryId),
    limits: &CommittedRecoveryLimits,
    context: (usize, bool),
    initial: (ControlState, Option<(EntryIdentity, EntryStamp)>),
    work: &mut [usize; 2],
) -> Result<(RedbLogStore, RedbStateStore), Error> {
    let (saved, member) = initial;
    let metadata = log.state().map_err(CommittedRecoveryError::Log)?;
    let mut latest = None;
    let current = join::observe(&mut state, namespace, limits, context.0, &mut latest)?;
    provenance::boundary([saved, current], metadata, limits, context.1)?;
    provenance::from(
        &mut log,
        namespace,
        limits,
        [saved, current],
        &[member, member, latest],
        work,
        (context.0, metadata.last_purged()),
    )?;
    if !context.1 {
        replay::apply(
            &mut log,
            &mut state,
            namespace,
            limits,
            (context.0, metadata.committed()),
            (current, latest),
            work,
        )?;
    }
    Ok((log, state))
}
