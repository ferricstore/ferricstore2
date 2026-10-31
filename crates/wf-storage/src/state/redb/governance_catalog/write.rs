//! One consumed Immediate writer joins membership, original outcome and control.

use super::super::{
    admission::{storage_error, table_error},
    commit, publication,
    store::{RedbStateTransaction, fence},
};
use super::{E, read, require};
use redb::Durability;
use wf_domain::{
    governance::catalog::{CatalogMembershipAction as A, CatalogMembershipChange as C},
    workflow::{
        control::{ControlPlan, ControlState, EntryIdentity},
        request::{RequestScope, RetainedOutcome, RetainedStatus},
    },
};
use wf_ports::{log::catalog_membership_request_intent, state::GroupApplyTransaction};

pub(crate) fn apply(
    tx: RedbStateTransaction<'_>,
    scope: &RequestScope,
    expected: ControlState,
    control: ControlPlan,
    command: Option<C<'_>>,
    outcome: Option<&RetainedOutcome>,
) -> Result<(), E> {
    let fenced = tx.fenced;
    publish(tx, scope, expected, control, command, outcome)
        .inspect_err(|error| fence(fenced, *error))
}
fn publish(
    mut tx: RedbStateTransaction<'_>,
    scope: &RequestScope,
    expected: ControlState,
    control: ControlPlan,
    command: Option<C<'_>>,
    outcome: Option<&RetainedOutcome>,
) -> Result<(), E> {
    tx.ready()?;
    let revisions = require(&tx, scope)?;
    if tx.control()? != expected {
        return Err(E::KnownAborted);
    }
    if control.is_log_replay() {
        if command.is_some() || outcome.is_some() {
            return Err(E::InvalidChanges);
        }
        return tx.write.abort().map_err(|e| storage_error(&e));
    }
    if let Some(command) = command {
        let outcome = outcome.ok_or(E::InvalidChanges)?;
        validate(scope, control, command, outcome)?;
        effects(
            &tx,
            scope,
            command,
            control.next().last_entry().ok_or(E::InvalidChanges)?,
            revisions.then_some(expected),
        )?;
        commit::write_outcome(&tx, scope, outcome)?;
    } else if outcome.is_some() {
        return Err(E::InvalidChanges);
    }
    commit::write_control(&tx, control.next())?;
    tx.write
        .set_durability(Durability::Immediate)
        .map_err(|_| E::Unavailable)?;
    tx.write.commit().map_err(|_| E::Uncertain)
}
fn validate(
    scope: &RequestScope,
    control: ControlPlan,
    command: C<'_>,
    outcome: &RetainedOutcome,
) -> Result<(), E> {
    command.validate(65_535).map_err(|_| E::InvalidChanges)?;
    if outcome.scope() != scope
        || outcome.intent()
            != catalog_membership_request_intent(command).map_err(|_| E::InvalidChanges)?
        || outcome.response_version() != 12
        || outcome.response() != b"FWGR\0\x01"
        || outcome.status() != RetainedStatus::Accepted
        || Some(outcome.original_position()) != control.next().last_entry().map(EntryIdentity::log)
        || outcome.original_time() != control.execution_time()
    {
        return Err(E::InvalidChanges);
    }
    Ok(())
}
fn effects(
    tx: &RedbStateTransaction<'_>,
    scope: &RequestScope,
    command: C<'_>,
    identity: EntryIdentity,
    revision_control: Option<ControlState>,
) -> Result<(), E> {
    for member in command.unique_members() {
        let key = read::key(tx, scope, command.catalog, Some(member))?;
        tx.step()?;
        let mut table = tx.write.open_table(tx.rows_table).map_err(table_error)?;
        let exists = read::contains(tx, &table, &key, revision_control)?;
        match command.action {
            A::Register if !exists || revision_control.is_some() => {
                drop(table);
                if revision_control.is_some() {
                    publication::put(
                        tx,
                        &key,
                        &wf_ports::state::encode_catalog_member_revision(identity),
                    )?;
                } else {
                    publication::put(tx, &key, b"FWGM\0\x01")?;
                }
            }
            A::Unregister if exists => {
                tx.step()?;
                if table
                    .remove(key.as_slice())
                    .map_err(|e| storage_error(&e))?
                    .is_none()
                {
                    return Err(E::Corrupt);
                }
            }
            _ => (),
        }
    }
    Ok(())
}
