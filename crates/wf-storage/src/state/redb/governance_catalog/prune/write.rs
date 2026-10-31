//! Consumed Immediate revision check, remove/no-op, original reply and control.
use super::super::super::{
    admission::{storage_error, table_error},
    commit,
    store::fence,
};
use super::super::{E, RedbStateTransaction, read};
use redb::Durability;
use wf_domain::{
    governance::catalog::{CatalogPruneReply as P, CatalogPruneRequest as C},
    workflow::{
        control::{ControlPlan, ControlState, EntryIdentity},
        request::{RequestScope, RetainedOutcome, RetainedStatus},
    },
};
use wf_ports::{
    log::{catalog_prune_request_intent, encode_catalog_prune_reply},
    state::GroupApplyTransaction,
};

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
    if scope.group() != tx.group || scope.recovery() != tx.recovery {
        return Err(E::NamespaceConflict);
    }
    tx.require_catalog_prune()?;
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
        let reply = super::decide(&tx, scope, command, expected)?;
        validate(scope, command, reply, outcome, control)?;
        if reply == P::Removed {
            remove(&tx, scope, command)?;
        }
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
    command: C<'_>,
    reply: P,
    outcome: &RetainedOutcome,
    control: ControlPlan,
) -> Result<(), E> {
    if outcome.scope() != scope
        || outcome.intent()
            != catalog_prune_request_intent(command).map_err(|_| E::InvalidChanges)?
        || outcome.response_version() != 14
        || outcome.response() != encode_catalog_prune_reply(reply)
        || outcome.status() != RetainedStatus::Accepted
        || Some(outcome.original_position()) != control.next().last_entry().map(EntryIdentity::log)
        || outcome.original_time() != control.execution_time()
    {
        return Err(E::InvalidChanges);
    }
    Ok(())
}
fn remove(tx: &RedbStateTransaction<'_>, scope: &RequestScope, command: C<'_>) -> Result<(), E> {
    let key = read::key(tx, scope, command.catalog, Some(command.member))?;
    tx.step()?;
    let mut table = tx.write.open_table(tx.rows_table).map_err(table_error)?;
    tx.step()?;
    if table
        .remove(key.as_slice())
        .map_err(|e| storage_error(&e))?
        .is_none()
    {
        return Err(E::Corrupt);
    }
    Ok(())
}
