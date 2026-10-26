//! One cumulative point pool; frozen-S authority is checked before current A.

use super::super::{CommittedRecoveryError as Error, CommittedRecoveryLimits, join};
use wf_domain::workflow::control::{ControlState, EntryIdentity, EntryStamp};
use wf_ports::log::RaftLogState;
use wf_storage::log::RedbLogStore;
use wf_types::identity::{GroupId, RecoveryId};

pub(super) fn boundary(
    controls: [ControlState; 2],
    metadata: RaftLogState,
    limits: &CommittedRecoveryLimits,
    purge: bool,
) -> Result<(), Error> {
    let [saved, current] = controls;
    if let Some(entry) = current.last_entry() {
        let committed = metadata.committed().ok_or(Error::IncompatibleCheckpoint)?;
        if entry.log().index() > committed.index()
            || (entry.log().index() == committed.index() && entry.log() != committed)
        {
            return Err(Error::IncompatibleCheckpoint);
        }
    }
    if let Some(entry) = saved.last_entry() {
        let applied = current.last_entry().ok_or(Error::IncompatibleCheckpoint)?;
        if entry.log().index() > applied.log().index()
            || (entry.log().index() == applied.log().index() && saved != current)
        {
            return Err(Error::IncompatibleCheckpoint);
        }
    }
    if current.time().hlc() < saved.time().hlc() || current.time().wall() < saved.time().wall() {
        return Err(Error::ClockMismatch);
    }
    let start = metadata
        .last_purged()
        .map_or(0, |id| u128::from(id.index().get()) + 1);
    let stop = if purge {
        current.last_entry().map(EntryIdentity::log)
    } else {
        metadata.committed()
    }
    .map_or(0, |id| u128::from(id.index().get()) + 1);
    let count = stop
        .checked_sub(start)
        .ok_or(Error::IncompatibleCheckpoint)?;
    if count > limits.work.0 as u128 {
        return Err(Error::Limit);
    }
    Ok(())
}

pub(super) fn from(
    log: &mut RedbLogStore,
    namespace: (&GroupId, &RecoveryId),
    limits: &CommittedRecoveryLimits,
    controls: [ControlState; 2],
    memberships: &[Option<(EntryIdentity, EntryStamp)>; 3],
    work: &mut [usize; 2],
    context: (usize, Option<wf_types::version::LogIdentity>),
) -> Result<(), Error> {
    let [saved, current] = controls;
    let saved_end = saved
        .last_entry()
        .map_or(0, |e| u128::from(e.log().index().get()) + 1);
    let end = current
        .last_entry()
        .map_or(0, |e| u128::from(e.log().index().get()) + 1);
    let start = context.1.map_or(0, |p| u128::from(p.index().get()) + 1);
    let [seed, snapshot_member, current_member] = *memberships;
    let mut latest = if context.1.is_some() { seed } else { None };
    if start == saved_end && latest != snapshot_member {
        return Err(Error::MembershipMismatch);
    }
    for position in start..end {
        let entry = join::point(
            log,
            u64::try_from(position).map_err(|_| Error::Limit)?,
            limits,
            context.0,
            work,
        )?;
        join::audit(
            &entry,
            namespace,
            limits,
            context.0,
            if position < saved_end { saved } else { current },
            &mut latest,
        )?;
        if position + 1 == saved_end && latest != snapshot_member {
            return Err(Error::MembershipMismatch);
        }
    }
    if latest != current_member {
        return Err(Error::MembershipMismatch);
    }
    Ok(())
}

pub(super) fn audit(
    log: &mut RedbLogStore,
    namespace: (&GroupId, &RecoveryId),
    limits: &CommittedRecoveryLimits,
    controls: [ControlState; 2],
    memberships: [Option<(EntryIdentity, EntryStamp)>; 2],
    work: &mut [usize; 2],
    context: (usize, bool),
) -> Result<(), Error> {
    let [saved, current] = controls;
    let [snapshot_member, current_member] = memberships;
    let saved_end = saved
        .last_entry()
        .map_or(0, |entry| u128::from(entry.log().index().get()) + 1);
    let end = current
        .last_entry()
        .map_or(0, |entry| u128::from(entry.log().index().get()) + 1);
    let start = if context.1 { saved_end } else { 0 };
    let mut latest = if context.1 { snapshot_member } else { None };
    if saved_end == 0 && snapshot_member.is_some() {
        return Err(Error::MembershipMismatch);
    }
    for position in start..end {
        let index = u64::try_from(position).map_err(|_| Error::Limit)?;
        let entry = join::point(log, index, limits, context.0, work)?;
        let bound = if position < saved_end { saved } else { current };
        join::audit(&entry, namespace, limits, context.0, bound, &mut latest)?;
        if position + 1 == saved_end && latest != snapshot_member {
            return Err(Error::MembershipMismatch);
        }
    }
    if latest != current_member {
        return Err(Error::MembershipMismatch);
    }
    Ok(())
}
