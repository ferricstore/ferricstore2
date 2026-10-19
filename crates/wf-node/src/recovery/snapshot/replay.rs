//! Real committed drivers, with the audit's remaining point/body account.

use super::super::{CommittedRecoveryError as Error, CommittedRecoveryLimits, join};
use wf_domain::workflow::control::{ControlState, EntryIdentity, EntryStamp};
use wf_ports::log::RaftLogStore;
use wf_storage::{log::RedbLogStore, state::RedbStateStore};
use wf_types::{
    identity::{GroupId, RecoveryId},
    version::LogIdentity,
};

pub(super) fn apply(
    log: &mut RedbLogStore,
    state: &mut RedbStateStore,
    namespace: (&GroupId, &RecoveryId),
    limits: CommittedRecoveryLimits,
    context: (usize, Option<LogIdentity>),
    initial: (ControlState, Option<(EntryIdentity, EntryStamp)>),
    work: &mut [usize; 2],
) -> Result<(), Error> {
    let (mut progress, mut latest) = initial;
    let start = progress
        .last_entry()
        .map_or(0, |entry| u128::from(entry.log().index().get()) + 1);
    let stop = context.1.map_or(0, |id| u128::from(id.index().get()) + 1);
    for position in start..stop {
        let index = u64::try_from(position).map_err(|_| Error::Limit)?;
        let entry = join::point(log, index, limits, context.0, work)?;
        if position + 1 == stop && Some(entry.id()) != context.1 {
            return Err(Error::IdentityMismatch);
        }
        progress =
            super::super::replay::apply(state, &entry, namespace, limits, context.0, progress)?;
        if entry.body().starts_with(b"FWMB") {
            latest = Some((
                progress.last_entry().ok_or(Error::ProgressMismatch)?,
                progress.last_entry_stamp().ok_or(Error::ProgressMismatch)?,
            ));
        }
    }
    let mut observed_member = None;
    let observed = join::observe(state, namespace, limits, context.0, &mut observed_member)?;
    if observed != progress {
        return Err(Error::ProgressMismatch);
    }
    if observed_member != latest {
        return Err(Error::MembershipMismatch);
    }
    if log.state().map_err(Error::Log)?.committed() != context.1 {
        return Err(Error::IncompatibleCheckpoint);
    }
    Ok(())
}
