//! Touched-only canonical ordering/index checks and allocation.

use crate::claim::{ClaimRecord, LaneKey, LaneOrder, ScopedRecordId};
use crate::policy::definition::StateMode;
use crate::workflow::admission::{AdmissionLimits, add, record_bytes};
use crate::workflow::batch::{
    ApplyError, BatchAuthority, BatchDecision, BatchItem, BatchRejection, admitted_claim_bytes,
    admitted_claim_with, admitted_with, build_plan_with,
};
use crate::workflow::{Operation, TransitionPlan};

use super::authority::{IndexedAuthority, active, logical_state};

use super::{
    EntryOrder, IndexRejection, IndexRows, IndexedBatchItem, IndexedBatchPlan, IndexedDecision,
    IndexedGroupView, OrderingChange, OrderingMetadata, rows,
};

pub(super) fn decide<'view>(
    view: IndexedAuthority<'_, 'view>,
    items: &[IndexedBatchItem<'_>],
    order: EntryOrder,
    encoded_entry: &[u8],
    limits: AdmissionLimits,
) -> Result<IndexedDecision<'view>, ApplyError> {
    decide_with(view, items, order, encoded_entry, limits, admitted_with)
}

fn decide_with<'query, 'view, 'input>(
    view: IndexedAuthority<'query, 'view>,
    items: &[IndexedBatchItem<'input>],
    order: EntryOrder,
    encoded_entry: &[u8],
    limits: AdmissionLimits,
    admit: impl Fn(
        BatchAuthority<'query, 'view>,
        &[BatchItem<'input>],
        &[u8],
        AdmissionLimits,
    ) -> Result<Result<usize, BatchRejection>, ApplyError>,
) -> Result<IndexedDecision<'view>, ApplyError> {
    if items.is_empty() || items.len() > limits.batch_items() {
        return Ok(rejected(IndexRejection::Batch(BatchRejection::Admission(
            crate::workflow::admission::AdmissionError::BatchItems,
        ))));
    }
    let mut commands = Vec::new();
    commands
        .try_reserve_exact(items.len())
        .map_err(|_| ApplyError::AllocationFailed)?;
    commands.extend(items.iter().map(|item| item.item));
    let mut bytes = match admit(view.batch(), &commands, encoded_entry, limits)? {
        Ok(bytes) => bytes,
        Err(error) => return Ok(rejected(IndexRejection::Batch(error))),
    };
    for item in items {
        if let Some(error) = validate(view, *item, limits, &mut bytes)? {
            return Ok(rejected(error));
        }
    }
    if let Err(error) = limits.charge(bytes, items.len()) {
        return Ok(rejected(IndexRejection::Batch(BatchRejection::Admission(
            error,
        ))));
    }
    let mut ordering = Vec::new();
    ordering
        .try_reserve_exact(items.len())
        .map_err(|_| ApplyError::AllocationFailed)?;
    let batch = match build_plan_with(view.batch(), &commands)? {
        BatchDecision::Accepted(plan) => plan,
        BatchDecision::Rejected(error) => return Ok(rejected(IndexRejection::Batch(error))),
    };
    let mut consumed = 0_u32;
    if let Some(error) = build_ordering(view, items, &batch, order, &mut ordering, &mut consumed)? {
        return Ok(rejected(error));
    }
    Ok(IndexedDecision::Accepted(IndexedBatchPlan {
        batch,
        ordering,
        order,
        consumed,
    }))
}

fn build_ordering<'current>(
    view: IndexedAuthority<'_, 'current>,
    items: &[IndexedBatchItem<'_>],
    batch: &crate::workflow::batch::BatchPlan,
    order: EntryOrder,
    ordering: &mut Vec<OrderingChange<'current>>,
    consumed: &mut u32,
) -> Result<Option<IndexRejection>, ApplyError> {
    for (item, plan) in items.iter().zip(batch.items()) {
        let before = view.current(item.item)?;
        let before_mode =
            before.map_or(Ok(StateMode::Parallel), |entry| view.mode(entry.record))?;
        let (after, after_mode, next) = match next_ordering(
            view,
            *item,
            before,
            before_mode,
            plan.plan(),
            order,
            *consumed,
        )? {
            Ok(values) => values,
            Err(error) => return Ok(Some(error)),
        };
        *consumed = next;
        ordering.push(OrderingChange {
            before,
            before_mode,
            after,
            after_mode,
        });
    }
    Ok(None)
}

pub(super) fn next_ordering(
    view: IndexedAuthority<'_, '_>,
    item: IndexedBatchItem<'_>,
    before: Option<ClaimRecord<'_>>,
    before_mode: StateMode,
    plan: &TransitionPlan,
    order: EntryOrder,
    consumed: u32,
) -> Result<Result<(OrderingMetadata, StateMode, u32), IndexRejection>, ApplyError> {
    let priority = before.map_or(item.create_priority, |entry| entry.priority);
    let Some(write) = plan.write() else {
        let entry = before.ok_or(ApplyError::InvalidAuthority)?;
        return Ok(Ok((
            OrderingMetadata {
                priority,
                state_enter_seq: entry.state_enter_seq,
            },
            before_mode,
            consumed,
        )));
    };
    let record = &write.record;
    let after_mode = view.mode(record)?;
    if after_mode == StateMode::Fifo
        && priority != 0
        && active(record)
        && matches!(
            item.item.command.operation,
            Operation::Create(_)
                | Operation::Transition(_)
                | Operation::Retry(_)
                | Operation::RetryWithPolicy(_)
        )
    {
        return Ok(Err(IndexRejection::FifoPriority));
    }
    let allocate =
        before.is_none_or(|entry| entry.logical_state() != logical_state(record) && active(record));
    let (sequence, next) = if allocate {
        let ordinal = order
            .first_ordinal
            .checked_add(consumed)
            .ok_or(ApplyError::InvalidAuthority)?;
        let next = consumed
            .checked_add(1)
            .ok_or(ApplyError::InvalidAuthority)?;
        (
            (u128::from(order.identity.index().get()) << 32) + u128::from(ordinal),
            next,
        )
    } else {
        (
            before.ok_or(ApplyError::InvalidAuthority)?.state_enter_seq,
            consumed,
        )
    };
    Ok(Ok((
        OrderingMetadata {
            priority,
            state_enter_seq: sequence,
        },
        after_mode,
        next,
    )))
}

pub(crate) fn decide_selected_claims<'view>(
    view: IndexedGroupView<'view>,
    items: &[IndexedBatchItem<'_>],
    order: EntryOrder,
    encoded_entry: &[u8],
    limits: AdmissionLimits,
) -> Result<IndexedDecision<'view>, ApplyError> {
    decide_with(
        IndexedAuthority::Full(view),
        items,
        order,
        encoded_entry,
        limits,
        |view, items, encoded, limits| match view {
            BatchAuthority::Full(view) => admitted_claim_bytes(view, items, encoded, limits),
            BatchAuthority::Touched(_) => Err(ApplyError::InvalidAuthority),
        },
    )
}

pub(crate) fn decide_selected_claims_touched<'current>(
    view: super::TouchedIndexedGroupView<'_, 'current>,
    items: &[IndexedBatchItem<'_>],
    order: EntryOrder,
    encoded_entry: &[u8],
    limits: AdmissionLimits,
) -> Result<IndexedDecision<'current>, ApplyError> {
    decide_with(
        IndexedAuthority::Touched(view),
        items,
        order,
        encoded_entry,
        limits,
        admitted_claim_with,
    )
}

fn rejected(error: IndexRejection) -> IndexedDecision<'static> {
    IndexedDecision::Rejected(error)
}

pub(super) fn validate(
    view: IndexedAuthority<'_, '_>,
    item: IndexedBatchItem<'_>,
    limits: AdmissionLimits,
    bytes: &mut usize,
) -> Result<Option<IndexRejection>, ApplyError> {
    let Some(entry) = view.current(item.item)? else {
        return Ok((matches!(item.item.command.operation, Operation::Create(_))
            && item.create_priority > 2)
            .then_some(IndexRejection::InvalidPriority));
    };
    if entry.priority > 2 {
        return Err(ApplyError::InvalidAuthority);
    }
    match rows(entry, view.mode(entry.record)?) {
        None => {}
        Some(IndexRows::Parallel { due }) => {
            if !view.parallel_member(due, entry)?
                || match LaneKey::for_record(&entry) {
                    Some(lane) => view.lane_present(lane)?,
                    None => false,
                }
            {
                return Err(ApplyError::InvalidAuthority);
            }
        }
        Some(IndexRows::Fifo {
            lane,
            member,
            head_candidate,
        }) => {
            if let Some(head_candidate) = head_candidate {
                if view.member(lane, member)? != Some(entry)
                    || view.parallel_contains(head_candidate)?
                {
                    return Err(ApplyError::InvalidAuthority);
                }
            } else {
                let row = view
                    .member(lane, member)?
                    .ok_or(ApplyError::InvalidAuthority)?;
                if row.partition_key.is_empty()
                    || row.partition_key.len() > wf_types::bounds::MAX_RECORD_ID_BYTES
                {
                    return Err(ApplyError::InvalidAuthority);
                }
                let row_bytes = validate_head(view, lane, member, row, limits)?;
                if ScopedRecordId::for_record(&row) != ScopedRecordId::for_record(&entry) {
                    return Err(ApplyError::InvalidAuthority);
                }
                if let Err(error) = add(bytes, row_bytes).and_then(|()| limits.check_total(*bytes))
                {
                    return Ok(Some(IndexRejection::Batch(BatchRejection::Admission(
                        error,
                    ))));
                }
            }
            if matches!(
                item.item.command.operation,
                Operation::Claim(_) | Operation::Reclaim(_)
            ) {
                let (head_order, head) = view.head(lane)?;
                let head_bytes = validate_head(view, lane, head_order, head, limits)?;
                if let Err(error) = add(bytes, head_bytes).and_then(|()| limits.check_total(*bytes))
                {
                    return Ok(Some(IndexRejection::Batch(BatchRejection::Admission(
                        error,
                    ))));
                }
                if ScopedRecordId::for_record(&head) != ScopedRecordId::for_record(&entry) {
                    return Ok(Some(IndexRejection::NotFifoHead));
                }
            }
        }
    }
    Ok(None)
}

fn validate_head(
    view: IndexedAuthority<'_, '_>,
    lane: LaneKey<'_>,
    order: LaneOrder<'_>,
    head: ClaimRecord<'_>,
    limits: AdmissionLimits,
) -> Result<usize, ApplyError> {
    let mut head_bytes =
        record_bytes(head.record, limits).map_err(|_| ApplyError::InvalidAuthority)?;
    let id = ScopedRecordId::for_record(&head);
    let canonical = view.canonical(id)?;
    let canonical_bytes =
        record_bytes(canonical, limits).map_err(|_| ApplyError::InvalidAuthority)?;
    add(&mut head_bytes, canonical_bytes).map_err(|_| ApplyError::InvalidAuthority)?;
    if head.priority > 2
        || LaneKey::for_record(&head) != Some(lane)
        || LaneOrder::for_record(&head) != order
        || !view.canonical_matches(id, head)?
        || head.record != canonical
        || match crate::claim::DueOrder::for_record(&head) {
            Some(due) => view.parallel_contains(due)?,
            None => false,
        }
    {
        return Err(ApplyError::InvalidAuthority);
    }
    Ok(head_bytes)
}
