//! Consumed coherent authority, with actual lengths admitted before full equality.

use super::admission::{TimeoutCharge, admit_group, admit_visit, charge_trace};
use super::{
    ActiveIndexKey, ActiveRecord, ActiveTimeoutRejection, ActiveTimeoutView, TimeoutLimits,
    active_rows,
};
use crate::claim::{LaneKey, ScopedRecordId};
use crate::policy::definition::StateMode;
use crate::workflow::admission::{AdmissionLimits, record_bytes};
use crate::workflow::batch::ApplyError;

pub(super) fn validate_root<'v>(
    view: ActiveTimeoutView<'v>,
    id: ScopedRecordId<'_>,
) -> Result<Option<&'v ActiveRecord>, Result<ActiveTimeoutRejection, ApplyError>> {
    let Some(record) = view.records.get(&id).copied() else {
        if view.indexed.batch.records.contains_key(&id) || view.indexed.records.contains_key(&id) {
            return Err(Err(ApplyError::InvalidAuthority));
        }
        return Ok(None);
    };
    if view.indexed.batch.placements.get(record.partition()) != Some(view.indexed.batch.group) {
        return Err(Ok(ActiveTimeoutRejection::CrossGroup));
    }
    Ok(Some(record))
}

pub(super) fn validate_dependency_placement(
    view: ActiveTimeoutView<'_>,
    record: &ActiveRecord,
) -> Result<(), ApplyError> {
    if view.indexed.batch.placements.get(record.partition()) == Some(view.indexed.batch.group) {
        Ok(())
    } else {
        Err(ApplyError::InvalidAuthority)
    }
}

pub(super) fn validate_consumed_record(
    view: ActiveTimeoutView<'_>,
    record: &ActiveRecord,
    limits: AdmissionLimits,
    charge: &mut TimeoutCharge,
    work: TimeoutLimits,
) -> Result<StateMode, Result<ActiveTimeoutRejection, ApplyError>> {
    let invalid = || Err(ApplyError::InvalidAuthority);
    admit_consumed_groups(record, limits, charge, work)?;
    let r = super::record::validate_active_data(record, limits).map_err(|_| invalid())?;
    let id = ScopedRecordId {
        partition_key: record.partition().as_bytes(),
        id: record.core().id().as_bytes(),
    };
    admit_visit(charge, id.partition_key.len() + id.id.len(), work)?;
    let canonical = view.indexed.batch.records.get(&id).ok_or_else(invalid)?;
    let entry = view.indexed.records.get(&id).ok_or_else(invalid)?;
    let a = record_bytes(canonical, limits).map_err(|_| invalid())?;
    let b = record_bytes(entry.record, limits).map_err(|_| invalid())?;
    let bytes = r
        .checked_mul(2)
        .and_then(|n| n.checked_add(a))
        .and_then(|n| n.checked_add(b))
        .ok_or_else(invalid)?;
    charge_trace(
        charge,
        0,
        u64::try_from(bytes).map_err(|_| invalid())?,
        work,
    )?;
    if *canonical != record.core()
        || entry.record != record.core()
        || entry.partition_key != record.partition().as_bytes()
        || entry.priority != record.ordering().priority
        || entry.state_enter_seq != record.ordering().state_enter_seq
    {
        return Err(invalid());
    }
    admit_visit(charge, record.core().workflow_type().len(), work)?;
    let policy = view.indexed.policies.get(record.core().workflow_type());
    if let Some(policy) = policy {
        if policy.workflow_type().len() > limits.inline_bytes() {
            return Err(invalid());
        }
        admit_visit(
            charge,
            policy.workflow_type().len() + record.core().workflow_type().len(),
            work,
        )?;
        if policy.workflow_type() != record.core().workflow_type() {
            return Err(invalid());
        }
    }
    admit_visit(charge, entry.logical_state().len(), work)?;
    let mode = policy.map_or(StateMode::Parallel, |p| p.state_mode(entry.logical_state()));
    let rows = active_rows(record, mode).map_err(Err)?;
    validate_lifecycle_rows(view, record, charge, work)?;
    let names = id.partition_key.len()
        + id.id.len()
        + record.core().workflow_type().len()
        + entry.logical_state().len();
    let row = if let Some(due) = rows.parallel {
        admit_visit(charge, names, work)?;
        let row = view.indexed.parallel.get(&due).ok_or_else(invalid)?;
        if LaneKey::for_record(entry).is_some_and(|lane| view.indexed.fifo.contains_key(&lane)) {
            return Err(invalid());
        }
        Some(row)
    } else if let Some((lane, member, due)) = rows.fifo {
        admit_visit(charge, names, work)?;
        let row = view
            .indexed
            .fifo
            .get(&lane)
            .and_then(|members| members.get(&member))
            .ok_or_else(invalid)?;
        if due.is_some_and(|due| view.indexed.parallel.contains_key(&due)) || entry.priority != 0 {
            return Err(invalid());
        }
        Some(row)
    } else {
        None
    };
    if let Some(row) = row {
        let bytes = record_bytes(row.record, limits)
            .map_err(|_| invalid())?
            .checked_add(b)
            .and_then(|n| n.checked_add(row.partition_key.len()))
            .and_then(|n| n.checked_add(entry.partition_key.len()))
            .ok_or_else(invalid)?;
        admit_visit(charge, bytes, work)?;
        if row != entry {
            return Err(invalid());
        }
    }
    validate_history_authority(view, id, charge, work)?;
    Ok(mode)
}

pub(super) fn admit_consumed_groups(
    record: &ActiveRecord,
    limits: AdmissionLimits,
    charge: &mut TimeoutCharge,
    work: TimeoutLimits,
) -> Result<(), Result<ActiveTimeoutRejection, ApplyError>> {
    if record.child_groups().len() > limits.batch_items() {
        return Err(Err(ApplyError::InvalidAuthority));
    }
    for (id, group) in record.child_groups() {
        if id.len() > limits.inline_bytes() || group.children.len() > limits.batch_items() {
            return Err(Err(ApplyError::InvalidAuthority));
        }
        admit_group(charge, id.len(), group.children.len(), work)?;
    }
    Ok(())
}

pub(super) fn validate_lifecycle_rows(
    view: ActiveTimeoutView<'_>,
    record: &ActiveRecord,
    charge: &mut TimeoutCharge,
    work: TimeoutLimits,
) -> Result<(), Result<ActiveTimeoutRejection, ApplyError>> {
    let rows = active_rows(record, StateMode::Parallel).map_err(Err)?;
    for (key, score) in rows.semantic.into_iter().flatten() {
        let names = record.partition().as_bytes().len()
            + record.core().id().as_bytes().len()
            + match key {
                ActiveIndexKey::Due {
                    workflow_type,
                    state,
                    ..
                }
                | ActiveIndexKey::State {
                    workflow_type,
                    state,
                    ..
                } => workflow_type.len() + state.len(),
                ActiveIndexKey::DueAny { workflow_type, .. }
                | ActiveIndexKey::Inflight { workflow_type, .. } => workflow_type.len(),
                ActiveIndexKey::Worker { worker, .. } => worker.len(),
                ActiveIndexKey::Parent { value, .. }
                | ActiveIndexKey::Root { value, .. }
                | ActiveIndexKey::Correlation { value, .. } => value.len(),
                ActiveIndexKey::ActiveTimeout(_) | ActiveIndexKey::TerminalRetention(_) => 0,
            };
        admit_visit(charge, names, work)?;
        if view.lifecycle.get(&key) != Some(&score) {
            return Err(Err(ApplyError::InvalidAuthority));
        }
    }
    Ok(())
}

pub(super) fn validate_history_authority(
    view: ActiveTimeoutView<'_>,
    id: ScopedRecordId<'_>,
    charge: &mut TimeoutCharge,
    work: TimeoutLimits,
) -> Result<(), Result<ActiveTimeoutRejection, ApplyError>> {
    admit_visit(charge, id.partition_key.len() + id.id.len(), work)?;
    let history = view
        .history
        .get(&id)
        .ok_or(Err(ApplyError::InvalidAuthority))?;
    u64::try_from(history.len()).map_err(|_| Err(ApplyError::InvalidAuthority))?;
    Ok(())
}
