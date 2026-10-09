//! Bounded, deterministic selection from a coherent borrowed authoritative view.

use std::collections::BTreeSet;
use std::ops::Bound::{Excluded, Unbounded};

use crate::workflow::WorkflowStatus;

use super::request::valid_component;
use super::{
    AuthoritativeClaimView, ClaimRecord, ClaimSelection, DueOrder, LaneKey, LaneOrder,
    ScopedRecordId, SelectedClaim, SelectionAction, SelectionContinuation, SelectionError,
    SelectionRequest, SelectionStop,
};

/// Selects from replicated authoritative indexes, without mutating them or minting leases.
///
/// The apply adapter must satisfy the view's completeness/coherence obligations and
/// atomically persist subsequent record, lane, due, outcome and history mutations.
/// Candidate scans and direct lane-head reads have independent explicit budgets.
///
/// # Errors
/// Rejects invalid admission, snapshot continuation, or consumed authoritative index
/// identity as a whole call. A malformed row never permits a successful partial return.
pub fn select<'a>(
    view: &AuthoritativeClaimView<'a>,
    request: SelectionRequest<'a>,
    continuation: Option<&SelectionContinuation<'a>>,
) -> Result<ClaimSelection<'a>, SelectionError> {
    request.validate()?;
    if view.group != request.group {
        return Err(SelectionError::GroupMismatch);
    }
    if view.fifo.len() > request.head_budget {
        return Err(SelectionError::HeadBudget);
    }
    let mut cursor = cursor_for(view, request, continuation)?;
    let heads = read_heads(view)?;
    let fifo_after = cursor.fifo_after;
    let mut fifo = heads
        .into_iter()
        .filter(|head| fifo_after.is_none_or(|after| head.order > after))
        .peekable();
    let start = cursor.parallel_after.map_or(Unbounded, Excluded);
    let mut parallel = view.parallel.range((start, Unbounded)).peekable();
    let mut records = Vec::new();
    records
        .try_reserve_exact(request.limit)
        .map_err(|_| SelectionError::AllocationFailed)?;
    let mut identities = BTreeSet::new();
    let mut scanned = 0;
    while scanned < request.scan_budget && records.len() < request.limit {
        let parallel_first = match (parallel.peek(), fifo.peek()) {
            (Some((order, _)), Some(head)) => **order <= head.order,
            (Some(_), None) => true,
            (None, Some(_)) => false,
            (None, None) => break,
        };
        let entry = if parallel_first {
            let Some((order, entry)) = parallel.next() else {
                break;
            };
            validate_parallel(view, *order, *entry)?;
            cursor.parallel_after = Some(*order);
            *entry
        } else {
            let Some(head) = fifo.next() else { break };
            cursor.fifo_after = Some(head.order);
            head.entry
        };
        scanned += 1;
        if !identities.insert((entry.partition_key, entry.record.id().as_bytes())) {
            return Err(SelectionError::DuplicateIdentity);
        }
        if let Some(action) = action_for(request, entry) {
            records.push(SelectedClaim {
                record: entry.record,
                partition_key: entry.partition_key,
                action,
            });
        }
    }
    let stop = if parallel.peek().is_none() && fifo.peek().is_none() {
        SelectionStop::Complete
    } else if records.len() == request.limit {
        SelectionStop::ClaimLimit
    } else {
        SelectionStop::ScanBudget
    };
    Ok(ClaimSelection {
        records,
        scanned,
        head_reads: view.fifo.len(),
        stop,
        continuation: (stop != SelectionStop::Complete).then_some(cursor),
    })
}

#[derive(Clone, Copy)]
struct Head<'a> {
    order: DueOrder<'a>,
    entry: ClaimRecord<'a>,
}

fn read_heads<'a>(view: &AuthoritativeClaimView<'a>) -> Result<Vec<Head<'a>>, SelectionError> {
    let mut heads = Vec::new();
    heads
        .try_reserve_exact(view.fifo.len())
        .map_err(|_| SelectionError::AllocationFailed)?;
    let mut identities = BTreeSet::new();
    for (lane_key, lane) in view.fifo {
        if !valid_component(lane_key.workflow_type)
            || !valid_component(lane_key.state)
            || !valid_component(lane_key.partition_key)
        {
            return Err(SelectionError::InvalidIndex);
        }
        let Some((lane_order, entry)) = lane.first_key_value() else {
            continue;
        };
        validate_record(view, *entry)?;
        if entry.priority != 0 {
            return Err(SelectionError::InvalidFifo);
        }
        if LaneKey::for_record(entry) != Some(*lane_key)
            || LaneOrder::for_record(entry) != *lane_order
        {
            return Err(SelectionError::InvalidIndex);
        }
        if !identities.insert((entry.partition_key, entry.record.id().as_bytes())) {
            return Err(SelectionError::DuplicateIdentity);
        }
        if let Some(order) = DueOrder::for_record(entry) {
            heads.push(Head {
                order,
                entry: *entry,
            });
        }
    }
    heads.sort_unstable_by_key(|head| head.order);
    Ok(heads)
}

fn cursor_for<'a>(
    view: &AuthoritativeClaimView<'a>,
    request: SelectionRequest<'a>,
    continuation: Option<&SelectionContinuation<'a>>,
) -> Result<SelectionContinuation<'a>, SelectionError> {
    if let Some(cursor) = continuation {
        if cursor.request != request
            || cursor.group != view.group
            || cursor.revision != view.revision
        {
            return Err(SelectionError::StaleContinuation);
        }
        Ok(*cursor)
    } else {
        Ok(SelectionContinuation {
            group: view.group,
            revision: view.revision,
            request,
            parallel_after: None,
            fifo_after: None,
        })
    }
}

fn validate_parallel(
    view: &AuthoritativeClaimView<'_>,
    order: DueOrder<'_>,
    entry: ClaimRecord<'_>,
) -> Result<(), SelectionError> {
    validate_record(view, entry)?;
    if DueOrder::for_record(&entry) != Some(order) {
        return Err(SelectionError::InvalidIndex);
    }
    if LaneKey::for_record(&entry).is_some_and(|lane| view.fifo.contains_key(&lane)) {
        return Err(SelectionError::InvalidFifo);
    }
    Ok(())
}

fn validate_record(
    view: &AuthoritativeClaimView<'_>,
    entry: ClaimRecord<'_>,
) -> Result<(), SelectionError> {
    if entry.priority > 2 {
        return Err(SelectionError::InvalidPriority);
    }
    if !valid_component(entry.partition_key) || !valid_component(entry.logical_state()) {
        return Err(SelectionError::InvalidIndex);
    }
    if view.records.get(&ScopedRecordId::for_record(&entry)) != Some(&entry) {
        return Err(SelectionError::InvalidIndex);
    }
    Ok(())
}

fn action_for(request: SelectionRequest<'_>, entry: ClaimRecord<'_>) -> Option<SelectionAction> {
    if !request.matches(entry)
        || entry
            .record
            .next_run_at()
            .is_none_or(|due| due > request.now)
    {
        return None;
    }
    match entry.record.status() {
        WorkflowStatus::Scheduled if entry.record.lease().is_none() => Some(SelectionAction::Claim),
        WorkflowStatus::Running
            if request.reclaim_expired
                && entry
                    .record
                    .lease()
                    .is_some_and(|lease| lease.deadline <= request.now) =>
        {
            Some(SelectionAction::Reclaim)
        }
        _ => None,
    }
}
