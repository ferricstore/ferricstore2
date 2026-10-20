//! One consumed writer atomically publishes changes and their retained authority.

use super::{
    admission::{engine_pair, storage_error, total, value_error},
    publication,
    store::{RedbStateTransaction, fence},
};
use crate::state::{key, value};
use redb::Durability;
use wf_domain::workflow::control::{ControlState, EntryStamp};
use wf_domain::workflow::indexed::IndexedBatchPlan;
use wf_domain::workflow::request::{RequestScope, RetainedOutcome};
use wf_ports::log::{claim_retained_summary, record_batch_retained_summary};
use wf_ports::state::{GroupApplyChanges, GroupApplyTransaction, StateTransactionError};

pub(super) mod claim;
pub(super) mod control;
mod independent;

pub(super) fn apply(
    tx: RedbStateTransaction<'_>,
    changes: &GroupApplyChanges<'_, '_>,
) -> Result<(), StateTransactionError> {
    let fenced = tx.fenced;
    publish(tx, changes).inspect_err(|error| fence(fenced, *error))
}

fn publish(
    mut tx: RedbStateTransaction<'_>,
    changes: &GroupApplyChanges<'_, '_>,
) -> Result<(), StateTransactionError> {
    tx.ready()?;
    let scope = changes.scope();
    if scope.group() != tx.group || scope.recovery() != tx.recovery {
        return Err(StateTransactionError::InvalidChanges);
    }
    if tx.control()? != changes.expected() {
        return Err(StateTransactionError::KnownAborted);
    }
    let Some(control) = changes.control() else {
        return tx.write.abort().map_err(|e| storage_error(&e));
    };
    if let Some(business) = changes.business() {
        validate_versions(&tx, scope, business)?;
        let lanes =
            publication::affected_lanes(&tx, business.deltas().len(), business.deltas().map(Ok))?;
        for lane in &lanes {
            publication::head_due(&tx, scope, *lane, false)?;
        }
        for delta in business.deltas() {
            publication::index_delta(&tx, scope, &delta)?;
        }
        write_records(&tx, scope, business)?;
        for lane in lanes {
            publication::head_due(&tx, scope, lane, true)?;
        }
    }
    if let Some(business) = changes.independent_business() {
        independent::apply(&tx, scope, business)?;
    }
    if let Some(outcome) = changes.outcome() {
        write_outcome(&tx, scope, outcome)?;
    }
    write_control(&tx, control)?;
    tx.write
        .set_durability(Durability::Immediate)
        .map_err(|_| StateTransactionError::Unavailable)?;
    // Once consumed, an engine error is not an unchanged-predecessor proof.
    tx.write
        .commit()
        .map_err(|_| StateTransactionError::Uncertain)
}

fn validate_versions(
    tx: &RedbStateTransaction<'_>,
    scope: &RequestScope,
    business: &IndexedBatchPlan<'_>,
) -> Result<(), StateTransactionError> {
    for item in business.batch().items() {
        if let Some(write) = item.plan().write() {
            publication::validate_version(
                tx,
                scope,
                item.partition(),
                write.record.id(),
                write.expected_version,
            )?;
        }
    }
    Ok(())
}

fn write_records(
    tx: &RedbStateTransaction<'_>,
    scope: &RequestScope,
    business: &IndexedBatchPlan<'_>,
) -> Result<(), StateTransactionError> {
    for (item, ordering) in business.batch().items().iter().zip(business.metadata()) {
        if let Some(write) = item.plan().write() {
            publication::write_record(tx, scope, item.partition(), &write.record, ordering)?;
            if let Some(history) = item.plan().history() {
                publication::write_history(
                    tx,
                    scope,
                    item.partition(),
                    write.record.id(),
                    history,
                )?;
            }
        }
    }
    Ok(())
}

fn write_control(
    tx: &RedbStateTransaction<'_>,
    control: ControlState,
) -> Result<(), StateTransactionError> {
    let mut size = total(
        33,
        [tx.group.as_bytes().len(), tx.recovery.as_bytes().len()],
    )?;
    if control.last_entry().is_some() {
        size = total(size, [57])?;
        if matches!(control.last_entry_stamp(), Some(EntryStamp::Stamped(_))) {
            size = total(size, [18])?;
        }
    }
    let key = tx.output_key(
        &[tx.group.as_bytes(), tx.recovery.as_bytes()],
        0,
        size,
        |cap| key::encode_control_key(tx.group, tx.recovery, cap),
    )?;
    engine_pair(key.len(), size)?;
    tx.temporary(size)?;
    let encoded =
        value::encode_control(tx.group, tx.recovery, control, size).map_err(value_error)?;
    publication::put(tx, &key, &encoded)
}

fn write_outcome(
    tx: &RedbStateTransaction<'_>,
    scope: &RequestScope,
    outcome: &RetainedOutcome,
) -> Result<(), StateTransactionError> {
    if outcome.response().len() > tx.admission.decision_bytes() {
        return Err(StateTransactionError::Limit);
    }
    if outcome.intent().version() == 1 && outcome.response_version() == 3 {
        let (status, _) = claim_retained_summary(
            outcome.intent(),
            3,
            outcome.response(),
            outcome.original_time(),
            tx.admission,
        )
        .map_err(|error| match error {
            wf_ports::log::CodecError::ByteLimit
            | wf_ports::log::CodecError::ItemLimit
            | wf_ports::log::CodecError::ValueLimit => StateTransactionError::Limit,
            _ => StateTransactionError::InvalidChanges,
        })?;
        if status != outcome.status() {
            return Err(StateTransactionError::InvalidChanges);
        }
    }
    if outcome.intent().version() == 6
        && record_batch_retained_summary(
            outcome.intent(),
            outcome.response_version(),
            outcome.response(),
        )
        .map_err(|_| StateTransactionError::InvalidChanges)?
        .1
        .is_some_and(|count| count > tx.admission.batch_items())
    {
        return Err(StateTransactionError::Limit);
    }
    let size = total(
        102,
        [
            scope.group().as_bytes().len(),
            scope.recovery().as_bytes().len(),
            scope.tenant().as_bytes().len(),
            outcome.request().as_bytes().len(),
            outcome.response().len(),
        ],
    )?;
    let key = tx.output_key(
        &[
            scope.group().as_bytes(),
            scope.recovery().as_bytes(),
            scope.tenant().as_bytes(),
            outcome.request().as_bytes(),
        ],
        0,
        size,
        |cap| key::encode_outcome_key(scope, outcome.request(), cap),
    )?;
    engine_pair(key.len(), size)?;
    tx.temporary(size)?;
    let encoded = value::encode_outcome(outcome, size).map_err(value_error)?;
    publication::put(tx, &key, &encoded)
}
