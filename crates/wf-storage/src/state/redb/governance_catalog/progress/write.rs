//! One consumed Immediate transaction for derived progress, outcome and control.
use super::super::super::{
    admission::{storage_error, table_error, total},
    commit as publication_control, publication,
    store::fence,
};
use super::{E, RedbStateTransaction, read};
use redb::{Durability, ReadableTable};
use wf_domain::{
    governance::catalog::CatalogProgressChange as C,
    workflow::{
        control::{ControlPlan, ControlState, EntryIdentity},
        request::{RequestScope, RetainedOutcome, RetainedStatus},
    },
};
use wf_ports::{
    log::catalog_progress_request_intent,
    state::{GroupApplyTransaction, encode_catalog_progress_cursor},
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
    tx.require_catalog_progress()?;
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
        effects(&tx, scope, command)?;
        publication_control::write_outcome(&tx, scope, outcome)?;
    } else if outcome.is_some() {
        return Err(E::InvalidChanges);
    }
    publication_control::write_control(&tx, control.next())?;
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
            != catalog_progress_request_intent(command).map_err(|_| E::InvalidChanges)?
        || outcome.response_version() != 13
        || outcome.response() != b"FWPR\0\x01"
        || outcome.status() != RetainedStatus::Accepted
        || Some(outcome.original_position()) != control.next().last_entry().map(EntryIdentity::log)
        || outcome.original_time() != control.execution_time()
    {
        return Err(E::InvalidChanges);
    }
    Ok(())
}
fn effects(tx: &RedbStateTransaction<'_>, scope: &RequestScope, command: C<'_>) -> Result<(), E> {
    let key = read::key(tx, scope, command.catalog)?;
    tx.step()?;
    let mut table = tx.write.open_table(tx.rows_table).map_err(table_error)?;
    tx.step()?;
    let exists = if let Some(value) = table.get(key.as_slice()).map_err(|e| storage_error(&e))? {
        tx.recovery_row(&key, value.value())?;
        tx.admit_read(total(128, [value.value().len()])?)?;
        true
    } else {
        false
    };
    if let Some(cursor) = command.cursor {
        drop(table);
        let size = cursor.len().checked_add(10).ok_or(E::Limit)?;
        let owned = size.checked_add(128).ok_or(E::Limit)?;
        tx.temporary(owned)?;
        let bytes =
            encode_catalog_progress_cursor(cursor, size, owned).map_err(|error| match error {
                wf_ports::log::CodecError::Allocation => E::Allocation,
                _ => E::InvalidChanges,
            })?;
        publication::put(tx, &key, &bytes)?;
    } else if exists {
        tx.step()?;
        if table
            .remove(key.as_slice())
            .map_err(|e| storage_error(&e))?
            .is_none()
        {
            return Err(E::Corrupt);
        }
    }
    Ok(())
}
