//! Iterative scalar/reference discovery before final variable-owner admission.

use super::admission::{admit_roots, admit_step, admit_visit, charge_materialization};
use super::authority::{validate_consumed_record, validate_dependency_placement, validate_root};
use super::releases::{plan_release_append, release_occurrences};
use super::{ActiveTimeoutRejection, ActiveTimeoutView, TimeoutError, TimeoutLimits};
use crate::claim::ScopedRecordId;
use crate::history::EventKind;
use crate::lineage::ChildStatus;
use crate::value::{Counter, DomainError};
use crate::workflow::MAX_EXACT_MILLIS;
use crate::workflow::admission::AdmissionLimits;
use crate::workflow::batch::ApplyError;
use crate::workflow::indexed::EntryOrder;
use std::collections::{BTreeMap, BTreeSet};
use wf_types::version::TimestampMillis;

mod closure;
mod parent;
pub(super) mod state;
use closure::trace_closed_children;
use parent::trace_parent;
use state::{ShadowRecord, TimeoutFrame, TimeoutStep, TimeoutTrace, TraceValueRef};

pub(super) fn discover_trace<'v>(
    view: ActiveTimeoutView<'v>,
    roots: &[ScopedRecordId<'_>],
    now: TimestampMillis,
    order: EntryOrder,
    limits: AdmissionLimits,
    work: TimeoutLimits,
) -> Result<TimeoutTrace<'v>, Result<ActiveTimeoutRejection, ApplyError>> {
    let charge = admit_roots(roots, now.get(), limits)?;
    let outbox = plan_release_append(*view.outbox, 0)
        .map_err(Err)?
        .map_err(Ok)?;
    let mut trace = TimeoutTrace {
        view,
        now,
        order,
        owners: Vec::new(),
        lookup: BTreeMap::new(),
        groups: Vec::new(),
        steps: Vec::new(),
        frames: Vec::new(),
        path: BTreeSet::new(),
        trims: Vec::new(),
        hot_evictions: Vec::new(),
        heads: Vec::new(),
        values: Vec::new(),
        releases: Vec::new(),
        expected_outbox: outbox,
        next_outbox: outbox,
        expired_roots: 0,
        next_ordinal: Some(order.first_ordinal),
        consumed_ordinals: 0,
        charge,
    };
    for root in roots {
        let Some(owner) = load_shadow(&mut trace, *root, true, limits, work)? else {
            continue;
        };
        trace.frames.push(TimeoutFrame::ExpireRoot { owner });
        while let Some(frame) = trace.frames.pop() {
            admit_visit(&mut trace.charge, 0, work)?;
            dispatch_frame(&mut trace, frame, limits, work)?;
        }
    }
    super::history::plan_history_trim(&mut trace, limits, work)?;
    super::history::plan_hot_evictions(&mut trace, work)?;
    super::indexes::plan_fifo_heads(&mut trace, limits, work)?;
    let count =
        u64::try_from(trace.releases.len()).map_err(|_| Err(ApplyError::InvalidAuthority))?;
    for _ in &trace.releases {
        admit_visit(&mut trace.charge, 0, work)?;
    }
    trace.next_outbox = plan_release_append(trace.expected_outbox, count)
        .map_err(Err)?
        .map_err(Ok)?;
    charge_materialization(&mut trace, limits, work)?;
    Ok(trace)
}

pub(super) fn load_shadow(
    trace: &mut TimeoutTrace<'_>,
    id: ScopedRecordId<'_>,
    root: bool,
    limits: AdmissionLimits,
    work: TimeoutLimits,
) -> Result<Option<usize>, Result<ActiveTimeoutRejection, ApplyError>> {
    admit_visit(
        &mut trace.charge,
        id.partition_key.len() + id.id.len(),
        work,
    )?;
    if let Some(owner) = trace.lookup.get(&id) {
        return Ok(Some(*owner));
    }
    let original = if root {
        validate_root(trace.view, id)?
    } else {
        trace.view.records.get(&id).copied()
    };
    let Some(original) = original else {
        if trace.view.indexed.batch.records.contains_key(&id)
            || trace.view.indexed.records.contains_key(&id)
        {
            return Err(Err(ApplyError::InvalidAuthority));
        }
        return Ok(None);
    };
    admit_visit(
        &mut trace.charge,
        original.partition().as_bytes().len() + original.core().id().as_bytes().len(),
        work,
    )?;
    if original.partition().as_bytes() != id.partition_key
        || original.core().id().as_bytes() != id.id
    {
        return Err(Err(ApplyError::InvalidAuthority));
    }
    validate_dependency_placement(trace.view, original).map_err(Err)?;
    validate_consumed_record(trace.view, original, limits, &mut trace.charge, work)?;
    if trace.owners.len() >= limits.batch_items() {
        return Err(Ok(ActiveTimeoutRejection::Admission(
            crate::workflow::admission::AdmissionError::BatchItems,
        )));
    }
    let owner = trace.owners.len();
    trace.owners.push(ShadowRecord {
        original,
        latest_step: None,
        group_updates: Vec::new(),
    });
    trace.lookup.insert(
        ScopedRecordId {
            partition_key: original.partition().as_bytes(),
            id: original.core().id().as_bytes(),
        },
        owner,
    );
    Ok(Some(owner))
}

pub(super) fn trace_timeout(
    trace: &mut TimeoutTrace<'_>,
    owner: usize,
    work: TimeoutLimits,
) -> Result<(), Result<ActiveTimeoutRejection, ApplyError>> {
    let original = trace.owners[owner].original;
    let previous = trace.owners[owner].latest_step;
    let state = previous.map_or(original.core().state(), |s| trace.steps[s].state);
    if matches!(state, b"completed" | b"failed" | b"cancelled") {
        return Ok(());
    }
    let Some(duration) = original.captured().max_active_ms() else {
        return Ok(());
    };
    if original
        .core()
        .created_at()
        .checked_add(duration)
        .ok_or(Err(ApplyError::InvalidAuthority))?
        > trace.now
    {
        return Ok(());
    }
    let id = ScopedRecordId {
        partition_key: original.partition().as_bytes(),
        id: original.core().id().as_bytes(),
    };
    admit_visit(
        &mut trace.charge,
        id.partition_key.len() + id.id.len(),
        work,
    )?;
    if !trace.path.insert(id) {
        return Err(Err(ApplyError::InvalidAuthority));
    }
    let version = previous
        .map_or(original.core().version(), |s| trace.steps[s].version)
        .checked_add(1)
        .ok_or(Ok(ActiveTimeoutRejection::Domain(
            DomainError::CounterOverflow(Counter::Version),
        )))?;
    let step = TimeoutStep {
        owner,
        previous,
        group_checkpoint: trace.owners[owner].group_updates.len(),
        kind: EventKind::Failed,
        version,
        state: b"failed",
        updated_at: trace.now,
        next_run_at: None,
        lease: None,
        ttl_ms: None,
        retention_until: Some(TimestampMillis::new(
            trace
                .now
                .get()
                .saturating_add(original.captured().retention_ttl_ms())
                .min(MAX_EXACT_MILLIS),
        )),
        error_ref: Some(TraceValueRef::Timeout { owner, version }),
        timeout: Some(TimeoutError {
            max_active_ms: duration,
        }),
        ordering: previous.map_or(original.ordering(), |s| trace.steps[s].ordering),
        held_reservation: if previous.is_none_or(|s| trace.steps[s].state == b"running") {
            original.governance()
        } else {
            None
        },
    };
    admit_step(trace, step, work)?;
    let held_step = trace.steps.len();
    trace.steps.push(step);
    trace.owners[owner].latest_step = Some(held_step);
    trace.values.push((owner, version, duration));
    if let Some(reservation) = release_occurrences(step.state, step.held_reservation) {
        trace.releases.push((owner, reservation, trace.now));
    }
    trace.expired_roots = trace
        .expired_roots
        .checked_add(1)
        .ok_or(Err(ApplyError::InvalidAuthority))?;
    trace.frames.extend([
        TimeoutFrame::LeavePath { owner },
        TimeoutFrame::NotifyParent {
            child: owner,
            held_step,
            status: ChildStatus::Failed,
        },
        TimeoutFrame::CancelClosed { owner, held_step },
    ]);
    Ok(())
}

pub(super) fn trace_cancel(
    trace: &mut TimeoutTrace<'_>,
    owner: usize,
    work: TimeoutLimits,
) -> Result<(), Result<ActiveTimeoutRejection, ApplyError>> {
    let original = trace.owners[owner].original;
    let previous = trace.owners[owner].latest_step;
    let state = previous.map_or(original.core().state(), |s| trace.steps[s].state);
    if matches!(state, b"completed" | b"failed" | b"cancelled") {
        return Ok(());
    }
    let id = ScopedRecordId {
        partition_key: original.partition().as_bytes(),
        id: original.core().id().as_bytes(),
    };
    admit_visit(
        &mut trace.charge,
        id.partition_key.len() + id.id.len(),
        work,
    )?;
    if !trace.path.insert(id) {
        return Err(Err(ApplyError::InvalidAuthority));
    }
    let version = previous
        .map_or(original.core().version(), |s| trace.steps[s].version)
        .checked_add(1)
        .ok_or(Ok(ActiveTimeoutRejection::Domain(
            DomainError::CounterOverflow(Counter::Version),
        )))?;
    let step = TimeoutStep {
        owner,
        previous,
        group_checkpoint: trace.owners[owner].group_updates.len(),
        kind: EventKind::Cancelled,
        version,
        state: b"cancelled",
        updated_at: trace.now,
        next_run_at: None,
        lease: None,
        ttl_ms: None,
        retention_until: Some(TimestampMillis::new(
            trace
                .now
                .get()
                .saturating_add(original.captured().retention_ttl_ms())
                .min(MAX_EXACT_MILLIS),
        )),
        error_ref: previous.map_or_else(
            || {
                original
                    .values()
                    .error
                    .as_ref()
                    .map(TraceValueRef::Original)
            },
            |s| trace.steps[s].error_ref,
        ),
        timeout: None,
        ordering: previous.map_or(original.ordering(), |s| trace.steps[s].ordering),
        held_reservation: if previous.is_none_or(|s| trace.steps[s].state == b"running") {
            original.governance()
        } else {
            None
        },
    };
    admit_step(trace, step, work)?;
    let held_step = trace.steps.len();
    trace.steps.push(step);
    trace.owners[owner].latest_step = Some(held_step);
    if let Some(reservation) = release_occurrences(step.state, step.held_reservation) {
        trace.releases.push((owner, reservation, trace.now));
    }
    trace.frames.extend([
        TimeoutFrame::LeavePath { owner },
        TimeoutFrame::CancelClosed { owner, held_step },
    ]);
    Ok(())
}

pub(super) fn dispatch_frame(
    trace: &mut TimeoutTrace<'_>,
    frame: TimeoutFrame,
    limits: AdmissionLimits,
    work: TimeoutLimits,
) -> Result<(), Result<ActiveTimeoutRejection, ApplyError>> {
    match frame {
        TimeoutFrame::ExpireRoot { owner } => trace_timeout(trace, owner, work)?,
        TimeoutFrame::CancelChild { owner } => trace_cancel(trace, owner, work)?,
        TimeoutFrame::CancelClosed { owner, held_step } => {
            trace_closed_children(trace, owner, held_step, limits, work)?;
        }
        TimeoutFrame::NotifyParent {
            child,
            held_step,
            status,
        } => trace_parent(trace, child, held_step, status, limits, work)?,
        TimeoutFrame::PropagateParent { owner, held_step } => {
            let status = match trace.steps[held_step].state {
                b"completed" => Some(ChildStatus::Completed),
                b"failed" => Some(ChildStatus::Failed),
                b"cancelled" => Some(ChildStatus::Cancelled),
                _ => None,
            };
            if let Some(status) = status {
                trace.frames.push(TimeoutFrame::NotifyParent {
                    child: owner,
                    held_step,
                    status,
                });
            }
        }
        TimeoutFrame::FinishCancelledGroups {
            owner,
            held_step,
            patches,
        } => {
            trace.owners[owner].group_updates.extend(patches);
            let mut step = trace.steps[held_step];
            step.previous = trace.owners[owner].latest_step;
            step.group_checkpoint = trace.owners[owner].group_updates.len();
            step.kind = EventKind::ChildrenCancelled;
            step.version = step
                .previous
                .map_or(trace.owners[owner].original.core().version(), |s| {
                    trace.steps[s].version
                })
                .checked_add(1)
                .ok_or(Ok(ActiveTimeoutRejection::Domain(
                    DomainError::CounterOverflow(Counter::Version),
                )))?;
            step.timeout = None;
            admit_step(trace, step, work)?;
            let position = trace.steps.len();
            trace.steps.push(step);
            trace.owners[owner].latest_step = Some(position);
            if let Some(reservation) = release_occurrences(step.state, step.held_reservation) {
                trace.releases.push((owner, reservation, trace.now));
            }
        }
        TimeoutFrame::LeavePath { owner } => {
            let original = trace.owners[owner].original;
            admit_visit(
                &mut trace.charge,
                original.partition().as_bytes().len() + original.core().id().as_bytes().len(),
                work,
            )?;
            trace.path.remove(&ScopedRecordId {
                partition_key: original.partition().as_bytes(),
                id: original.core().id().as_bytes(),
            });
        }
    }
    Ok(())
}
