//! Consumed Immediate publication of Blank or complete applied membership.

use super::super::{
    membership,
    store::{RedbStateTransaction, fence},
};
use redb::Durability;
use wf_ports::state::{GroupApplyTransaction, GroupControlChanges, StateTransactionError as Error};

pub(crate) fn apply(
    tx: RedbStateTransaction<'_>,
    changes: &GroupControlChanges<'_>,
) -> Result<(), Error> {
    let fenced = tx.fenced;
    publish(tx, changes).inspect_err(|error| fence(fenced, *error))
}

fn publish(
    mut tx: RedbStateTransaction<'_>,
    changes: &GroupControlChanges<'_>,
) -> Result<(), Error> {
    tx.ready()?;
    if changes.group() != tx.group || changes.recovery() != tx.recovery {
        return Err(Error::InvalidChanges);
    }
    let current = tx.control()?;
    if current != changes.expected() {
        return Err(Error::KnownAborted);
    }
    wf_ports::log::measure_control_entry(
        changes.encoded(),
        tx.group,
        tx.recovery,
        tx.limits.membership_limits(),
    )
    .map_err(|error| match error {
        wf_ports::consensus::RaftMembershipError::Limit => Error::Limit,
        _ => Error::InvalidChanges,
    })?;
    membership::validate(&tx, current)?;
    let plan = changes.control();
    if plan.is_log_replay() {
        return tx
            .write
            .abort()
            .map_err(|error| super::storage_error(&error));
    }
    if let Some(body) = changes.membership_body() {
        let log = plan.next().last_entry().ok_or(Error::InvalidChanges)?.log();
        membership::write(&tx, Some(log), Some(body))?;
    }
    super::write_control(&tx, plan.next())?;
    tx.write
        .set_durability(Durability::Immediate)
        .map_err(|_| Error::Unavailable)?;
    tx.write.commit().map_err(|_| Error::Uncertain)
}
