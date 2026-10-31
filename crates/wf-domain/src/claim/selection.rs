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
    select_checked(
        view,
        request,
        continuation,
        |_, _, _| Ok::<_, SelectionError>(()),
        |_| true,
    )
    .map_err(|error| match error {
        SelectionFailure::Selection(error) | SelectionFailure::Guard(error) => error,
    })
}

pub(super) enum SelectionFailure<E> {
    Selection(SelectionError),
    Guard(E),
}

pub(super) fn select_checked<'a, E>(
    view: &AuthoritativeClaimView<'a>,
    request: SelectionRequest<'a>,
    continuation: Option<&SelectionContinuation<'a>>,
    mut guard: impl FnMut(
        Option<ClaimRecord<'a>>,
        Option<LaneKey<'a>>,
        Option<DueOrder<'a>>,
    ) -> Result<(), E>,
    physical: impl Fn(ClaimRecord<'a>) -> bool,
) -> Result<ClaimSelection<'a>, SelectionFailure<E>> {
    request.validate().map_err(SelectionFailure::Selection)?;
    if view.group != request.group {
        return Err(SelectionFailure::Selection(SelectionError::GroupMismatch));
    }
    if view.fifo.len() > request.head_budget {
        return Err(SelectionFailure::Selection(SelectionError::HeadBudget));
    }
    let mut cursor =
        cursor_for(view, request, continuation).map_err(SelectionFailure::Selection)?;
    let heads = read_heads(view, request, &mut guard)?;
    let fifo_after = cursor.fifo_after;
    let fifo = heads
        .into_iter()
        .filter(|head| fifo_after.is_none_or(|after| head.order > after));
    let start = cursor.parallel_after.map_or(Unbounded, Excluded);
    let parallel = view
        .parallel
        .range((start, Unbounded))
        .map(|(order, entry)| (*order, *entry));
    let (records, scanned, stop) = merge(
        &mut cursor,
        parallel,
        fifo,
        &mut guard,
        physical,
        |order, entry| validate_parallel(view, order, entry),
        |_| Ok(true),
    )?;
    Ok(ClaimSelection {
        records,
        scanned,
        head_reads: view.fifo.len(),
        stop,
        continuation: (stop != SelectionStop::Complete).then_some(cursor),
    })
}

pub(super) fn merge<'query, 'current: 'query, E>(
    cursor: &mut SelectionContinuation<'query>,
    parallel: impl Iterator<Item = (DueOrder<'current>, ClaimRecord<'current>)>,
    fifo: impl Iterator<Item = Head<'current>>,
    guard: &mut impl FnMut(
        Option<ClaimRecord<'current>>,
        Option<LaneKey<'current>>,
        Option<DueOrder<'current>>,
    ) -> Result<(), E>,
    physical: impl Fn(ClaimRecord<'current>) -> bool,
    mut validate: impl FnMut(DueOrder<'current>, ClaimRecord<'current>) -> Result<(), SelectionError>,
    mut bind: impl FnMut(ClaimRecord<'current>) -> Result<bool, E>,
) -> Result<(Vec<SelectedClaim<'current>>, usize, SelectionStop), SelectionFailure<E>> {
    let mut parallel = parallel.peekable();
    let mut fifo = fifo.peekable();
    let mut records = Vec::new();
    records
        .try_reserve_exact(cursor.request.limit)
        .map_err(|_| SelectionFailure::Selection(SelectionError::AllocationFailed))?;
    let mut identities = BTreeSet::new();
    let mut scanned = 0;
    while scanned < cursor.request.scan_budget && records.len() < cursor.request.limit {
        if parallel.peek().is_none() && fifo.peek().is_none() {
            break;
        }
        guard(None, None, None).map_err(SelectionFailure::Guard)?;
        if let Some((order, _)) = parallel.peek() {
            guard(None, None, Some(*order)).map_err(SelectionFailure::Guard)?;
        }
        let parallel_first = match (parallel.peek(), fifo.peek()) {
            (Some((order, _)), Some(head)) => *order <= head.order,
            (Some(_), None) => true,
            (None, Some(_)) => false,
            (None, None) => break,
        };
        let entry = if parallel_first {
            let Some((order, entry)) = parallel.next() else {
                break;
            };
            guard(Some(entry), None, None).map_err(SelectionFailure::Guard)?;
            validate(order, entry).map_err(SelectionFailure::Selection)?;
            cursor.parallel_after = Some(order);
            entry
        } else {
            let Some(head) = fifo.next() else { break };
            cursor.fifo_after = Some(head.order);
            head.entry
        };
        scanned += 1;
        if !identities.insert((entry.partition_key, entry.record.id().as_bytes())) {
            return Err(SelectionFailure::Selection(
                SelectionError::DuplicateIdentity,
            ));
        }
        if physical(entry)
            && let Some(action) = action_for(cursor.request, entry)
            && bind(entry).map_err(SelectionFailure::Guard)?
        {
            records.push(SelectedClaim {
                record: entry.record,
                partition_key: entry.partition_key,
                action,
            });
        }
    }
    let stop = if parallel.peek().is_none() && fifo.peek().is_none() {
        SelectionStop::Complete
    } else if records.len() == cursor.request.limit {
        SelectionStop::ClaimLimit
    } else {
        SelectionStop::ScanBudget
    };
    Ok((records, scanned, stop))
}

#[derive(Clone, Copy)]
pub(super) struct Head<'a> {
    pub(super) order: DueOrder<'a>,
    pub(super) entry: ClaimRecord<'a>,
}

fn read_heads<'a, E>(
    view: &AuthoritativeClaimView<'a>,
    request: SelectionRequest<'_>,
    guard: &mut impl FnMut(
        Option<ClaimRecord<'a>>,
        Option<LaneKey<'a>>,
        Option<DueOrder<'a>>,
    ) -> Result<(), E>,
) -> Result<Vec<Head<'a>>, SelectionFailure<E>> {
    let mut heads = Vec::new();
    heads
        .try_reserve_exact(view.fifo.len())
        .map_err(|_| SelectionFailure::Selection(SelectionError::AllocationFailed))?;
    let mut identities = BTreeSet::new();
    for (lane_key, lane) in view.fifo {
        guard(None, Some(*lane_key), None).map_err(SelectionFailure::Guard)?;
        if !valid_component(lane_key.workflow_type)
            || !valid_component(lane_key.state)
            || !valid_component(lane_key.partition_key)
        {
            return Err(SelectionFailure::Selection(SelectionError::InvalidIndex));
        }
        let Some((lane_order, entry)) = lane.first_key_value() else {
            continue;
        };
        guard(Some(*entry), None, None).map_err(SelectionFailure::Guard)?;
        validate_record(view, *entry).map_err(SelectionFailure::Selection)?;
        if LaneKey::for_record(entry) != Some(*lane_key)
            || LaneOrder::for_record(entry) != *lane_order
        {
            return Err(SelectionFailure::Selection(SelectionError::InvalidIndex));
        }
        if !identities.insert((entry.partition_key, entry.record.id().as_bytes())) {
            return Err(SelectionFailure::Selection(
                SelectionError::DuplicateIdentity,
            ));
        }
        if request
            .priority
            .is_none_or(|priority| priority == entry.priority)
            && let Some(order) = DueOrder::for_record(entry)
        {
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
