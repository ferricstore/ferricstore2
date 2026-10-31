//! Exact native maintenance effects, original receipt and control share one writer.
use super::{
    super::{
        admission::{storage_error, table_error},
        commit, publication,
        store::{RedbStateTransaction, fence},
    },
    read,
};
use redb::Durability;
use wf_domain::{
    governance::release::{
        ReleaseMaintenancePlan as P, ReleaseMaintenanceReply as R, ReleaseOutboxState,
        prepare_release_maintenance,
    },
    workflow::{
        control::{ControlPlan, ControlState, EntryIdentity, EntryStamp, plan_control},
        request::{RequestScope, RetainedOutcome, RetainedStatus},
    },
};
use wf_ports::{
    log::{
        CodecLimits, release_maintenance_request_intent, verify_release_maintenance_reply,
        verify_release_maintenance_request,
    },
    state::{GroupApplyTransaction, StateTransactionError as E},
};

pub(crate) fn current(
    tx: &RedbStateTransaction<'_>,
    scope: &RequestScope,
) -> Result<ReleaseOutboxState, E> {
    tx.checked(|| {
        read::require(tx, scope)?;
        tx.step()?;
        let table = tx.write.open_table(tx.rows_table).map_err(table_error)?;
        Ok(read::meta(tx, &table, scope)?.unwrap_or(ReleaseOutboxState { head: 1, tail: 0 }))
    })
}
pub(crate) fn apply(
    tx: RedbStateTransaction<'_>,
    scope: &RequestScope,
    expected: ControlState,
    control: ControlPlan,
    plan: Option<&P<'_>>,
    outcome: Option<&RetainedOutcome>,
    frame: (
        &[u8],
        &wf_domain::governance::release::ReleaseMaintenanceRequest,
    ),
) -> Result<(), E> {
    let fenced = tx.fenced;
    publish(tx, scope, expected, control, plan, outcome, frame).inspect_err(|e| fence(fenced, *e))
}
fn publish(
    mut tx: RedbStateTransaction<'_>,
    scope: &RequestScope,
    expected: ControlState,
    control: ControlPlan,
    plan: Option<&P<'_>>,
    outcome: Option<&RetainedOutcome>,
    (encoded, command): (
        &[u8],
        &wf_domain::governance::release::ReleaseMaintenanceRequest,
    ),
) -> Result<(), E> {
    tx.ready()?;
    read::require(&tx, scope)?;
    if tx.control()? != expected {
        return Err(E::KnownAborted);
    }
    let next = control.next();
    let entry = next.last_entry().ok_or(E::InvalidChanges)?;
    let stamp = next.last_entry_stamp().ok_or(E::InvalidChanges)?;
    if plan_control(expected, entry, stamp).map_err(|_| E::InvalidChanges)? != control {
        return Err(E::InvalidChanges);
    }
    let EntryStamp::Stamped(stamp) = stamp else {
        return Err(E::InvalidChanges);
    };
    let (request, intent, digest) = verify_release_maintenance_request(
        encoded,
        scope,
        stamp,
        command,
        CodecLimits::new(tx.admission),
    )
    .map_err(|_| E::InvalidChanges)?;
    if digest != entry.prepared() {
        return Err(E::InvalidChanges);
    }
    if control.is_log_replay() {
        if plan.is_some() || outcome.is_some() {
            return Err(E::InvalidChanges);
        }
        return tx.write.abort().map_err(|e| storage_error(&e));
    }
    if let Some(plan) = plan {
        let outcome = outcome.ok_or(E::InvalidChanges)?;
        if plan.request() != command
            || outcome.request().as_bytes() != request
            || outcome.intent() != intent
        {
            return Err(E::InvalidChanges);
        }
        validate(&tx, scope, control, plan, outcome)?;
        effects(&tx, scope, plan)?;
        commit::write_outcome(&tx, scope, outcome)?;
    } else if outcome.is_some() {
        return Err(E::InvalidChanges);
    }
    commit::write_control(&tx, next)?;
    tx.write
        .set_durability(Durability::Immediate)
        .map_err(|_| E::Unavailable)?;
    tx.write.commit().map_err(|_| E::Uncertain)
}
fn validate(
    tx: &RedbStateTransaction<'_>,
    scope: &RequestScope,
    control: ControlPlan,
    plan: &P<'_>,
    outcome: &RetainedOutcome,
) -> Result<(), E> {
    tx.temporary(1024)?;
    let status = match plan.reply() {
        R::Accepted { .. } => RetainedStatus::Accepted,
        R::Rejected { reason, .. } => RetainedStatus::WorkflowReleaseRejected(reason),
    };
    if outcome.scope() != scope
        || outcome.intent()
            != release_maintenance_request_intent(plan.request()).map_err(|_| E::InvalidChanges)?
        || outcome.response_version() != 15
        || outcome.status() != status
        || Some(outcome.original_position()) != control.next().last_entry().map(EntryIdentity::log)
        || outcome.original_time() != control.execution_time()
    {
        return Err(E::InvalidChanges);
    }
    verify_release_maintenance_reply(plan.reply(), outcome.response())
        .map_err(|_| E::InvalidChanges)
}
fn effects(tx: &RedbStateTransaction<'_>, scope: &RequestScope, plan: &P<'_>) -> Result<(), E> {
    let context = read::require(tx, scope)?;
    tx.step()?;
    let table = tx.write.open_table(tx.rows_table).map_err(table_error)?;
    let actual = read::meta(tx, &table, scope)?.unwrap_or(ReleaseOutboxState { head: 1, tail: 0 });
    if actual != plan.original() {
        return Err(E::KnownAborted);
    }
    if prepare_release_maintenance(actual, plan.request())
        .map_err(|_| E::Corrupt)?
        .reply()
        != plan.reply()
    {
        return Err(E::InvalidChanges);
    }
    drop(table);
    if let Some(completion) = plan.completion() {
        complete(tx, scope, completion, context)?;
    }
    if let Some(ack) = plan.acknowledgement() {
        acknowledge(tx, scope, ack, context)?;
    }
    Ok(())
}
fn complete(
    tx: &RedbStateTransaction<'_>,
    scope: &RequestScope,
    completion: &wf_domain::governance::release::ReleaseCompletionPlan<'_>,
    context: (u64, wf_types::version::TimestampMillis),
) -> Result<(), E> {
    for sequence in completion.sequences() {
        let key = read::queue_key(tx, scope, 22, Some(sequence))?;
        let marker = read::queue_key(tx, scope, 23, Some(sequence))?;
        {
            tx.step()?;
            let table = tx.write.open_table(tx.rows_table).map_err(table_error)?;
            read::read(tx, &table, &key, |bytes| {
                read::intent(bytes, context).map(|_| ())
            })?
            .ok_or(E::Corrupt)?;
            read::read(tx, &table, &marker, read::completion)?;
        }
        tx.temporary(6)?;
        publication::put(tx, &marker, b"FWRC\0\x01")?;
    }
    Ok(())
}
fn acknowledge(
    tx: &RedbStateTransaction<'_>,
    scope: &RequestScope,
    ack: &wf_domain::governance::release::ReleaseAckPlan,
    context: (u64, wf_types::version::TimestampMillis),
) -> Result<(), E> {
    tx.step()?;
    let mut table = tx.write.open_table(tx.rows_table).map_err(table_error)?;
    for sequence in ack.sequences() {
        let key = read::queue_key(tx, scope, 22, Some(sequence))?;
        read::read(tx, &table, &key, |bytes| {
            read::intent(bytes, context).map(|_| ())
        })?
        .ok_or(E::Corrupt)?;
        let marker = read::queue_key(tx, scope, 23, Some(sequence))?;
        let completed = read::read(tx, &table, &marker, read::completion)?.is_some();
        tx.step()?;
        if table
            .remove(key.as_slice())
            .map_err(|e| storage_error(&e))?
            .is_none()
        {
            return Err(E::Corrupt);
        }
        if completed {
            tx.step()?;
            table
                .remove(marker.as_slice())
                .map_err(|e| storage_error(&e))?;
        }
    }
    drop(table);
    let key = read::queue_key(tx, scope, 21, None)?;
    tx.temporary(22)?;
    let bytes = wf_ports::log::encode_release_meta(ack.next(), 22).map_err(read::codec_error)?;
    publication::put(tx, &key, &bytes)
}
