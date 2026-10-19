//! Borrowed membership validation and the same exclusive guard capability.

use super::{StateStoreLimits, admission, store::RedbStateTransaction};
use crate::state::value;
use redb::ReadableTable;
use wf_domain::workflow::control::{ControlState, EntryStamp};
use wf_ports::consensus::{AppliedRaftMembership, RaftMembershipLimits};
use wf_ports::log::measure_control_entry;
use wf_ports::state::{
    GroupControlChanges, GroupControlTransaction, StateTransactionError as Error,
};
use wf_types::identity::{GroupId, RecoveryId};
use wf_types::version::LogIdentity;

pub(super) const KEY: &[u8] = b"membership";

pub(super) fn initial(
    group: &GroupId,
    recovery: &RecoveryId,
    limits: StateStoreLimits,
) -> Result<usize, Error> {
    if limits.membership_limits().maximum_owned_bytes() < 1024 {
        return Err(Error::Limit);
    }
    let size = admission::total(43, [group.as_bytes().len(), recovery.as_bytes().len()])?;
    admission::engine_pair(KEY.len(), size)?;
    if size > limits.maximum_temporary_bytes() {
        return Err(Error::Limit);
    }
    Ok(size)
}

pub(super) fn write(
    tx: &RedbStateTransaction<'_>,
    log: Option<LogIdentity>,
    body: Option<&[u8]>,
) -> Result<(), Error> {
    let profile = tx.limits.membership_limits();
    let mut size = admission::total(
        43,
        [tx.group.as_bytes().len(), tx.recovery.as_bytes().len()],
    )?;
    match (log, body) {
        (Some(_), Some(bytes)) => {
            let (_, owner) =
                measure_control_entry(bytes, tx.group, tx.recovery, profile).map_err(|error| {
                    match error {
                        wf_ports::consensus::RaftMembershipError::Limit => Error::Limit,
                        _ => Error::InvalidChanges,
                    }
                })?;
            if owner.is_none() {
                return Err(Error::InvalidChanges);
            }
            size = admission::total(size, [32, bytes.len()])?;
        }
        (None, None) => {
            initial(tx.group, tx.recovery, tx.limits)?;
        }
        _ => return Err(Error::InvalidChanges),
    }
    admission::engine_pair(KEY.len(), size)?;
    tx.temporary(size)?;
    let encoded = value::encode_applied_membership(tx.group, tx.recovery, log, body, profile, size)
        .map_err(admission::value_error)?;
    let mut table = tx
        .write
        .open_table(tx.metadata_table)
        .map_err(admission::table_error)?;
    table
        .insert(KEY, encoded.as_slice())
        .map_err(|error| admission::storage_error(&error))?;
    Ok(())
}

pub(super) fn validate_value(
    tx: &RedbStateTransaction<'_>,
    bytes: &[u8],
    control: ControlState,
) -> Result<(), Error> {
    let mut owner = 0;
    let Some((entry, stamp, _)) = value::inspect_membership(
        bytes,
        tx.group,
        tx.recovery,
        tx.limits.membership_limits(),
        &mut owner,
    )
    .map_err(admission::value_error)?
    else {
        return Ok(());
    };
    let applied = control.last_entry().ok_or(Error::Corrupt)?;
    if entry.log().index() > applied.log().index()
        || (entry.log().index() == applied.log().index()
            && (entry != applied || control.last_entry_stamp() != Some(stamp)))
    {
        return Err(Error::Corrupt);
    }
    if let EntryStamp::Stamped(time) = stamp
        && (control.time().hlc() < time.hlc() || control.time().wall() < time.wall())
    {
        return Err(Error::Corrupt);
    }
    Ok(())
}

pub(super) fn validate(tx: &RedbStateTransaction<'_>, control: ControlState) -> Result<(), Error> {
    tx.step()?;
    let table = tx
        .write
        .open_table(tx.metadata_table)
        .map_err(admission::table_error)?;
    let bytes = table
        .get(KEY)
        .map_err(|error| admission::storage_error(&error))?
        .ok_or(Error::Corrupt)?;
    tx.recovery_row(KEY, bytes.value())?;
    validate_value(tx, bytes.value(), control)
}

impl GroupControlTransaction for RedbStateTransaction<'_> {
    fn owner_limits(&self) -> (usize, usize) {
        (self.maximum_read, self.maximum_temporary)
    }

    fn applied_membership(
        &self,
        limits: RaftMembershipLimits,
    ) -> Result<AppliedRaftMembership, Error> {
        self.checked(|| {
            if !limits.is_within(self.limits.membership_limits()) {
                return Err(Error::InvalidChanges);
            }
            self.step()?;
            let table = self
                .write
                .open_table(self.metadata_table)
                .map_err(admission::table_error)?;
            let bytes = table
                .get(KEY)
                .map_err(|error| admission::storage_error(&error))?
                .ok_or(Error::Corrupt)?;
            self.recovery_row(KEY, bytes.value())?;
            let owner =
                value::measure_applied_membership(bytes.value(), self.group, self.recovery, limits)
                    .map_err(admission::value_error)?;
            self.admit_read(owner)?;
            value::decode_applied_membership(
                bytes.value(),
                self.group,
                self.recovery,
                limits,
                owner,
            )
            .map_err(admission::value_error)
        })
    }

    fn commit_control(self, changes: &GroupControlChanges<'_>) -> Result<(), Error> {
        super::commit::control::apply(self, changes)
    }
}
