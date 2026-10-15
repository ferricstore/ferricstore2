//! Touched-only canonical ordering/index checks and allocation.

use crate::claim::{ClaimRecord, LaneKey, LaneOrder, ScopedRecordId};
use crate::policy::definition::StateMode;
use crate::workflow::admission::{AdmissionLimits, add, record_bytes};
use crate::workflow::batch::{
    ApplyError, BatchDecision, BatchItem, BatchRejection, GroupView, admitted_bytes,
    admitted_claim_bytes, build_plan,
};
use crate::workflow::{Operation, WorkflowRecord};

use super::{
    EntryOrder, IndexRejection, IndexRows, IndexedBatchItem, IndexedBatchPlan, IndexedDecision,
    IndexedGroupView, OrderingChange, OrderingMetadata, rows,
};

pub(super) fn decide<'view>(
    view: IndexedGroupView<'view>,
    items: &[IndexedBatchItem<'_>],
    order: EntryOrder,
    encoded_entry: &[u8],
    limits: AdmissionLimits,
) -> Result<IndexedDecision<'view>, ApplyError> {
    decide_with(view, items, order, encoded_entry, limits, admitted_bytes)
}

fn decide_with<'view, 'input>(
    view: IndexedGroupView<'view>,
    items: &[IndexedBatchItem<'input>],
    order: EntryOrder,
    encoded_entry: &[u8],
    limits: AdmissionLimits,
    admit: impl Fn(
        GroupView<'view>,
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
    let mut bytes = match admit(view.batch, &commands, encoded_entry, limits)? {
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
    let batch = match build_plan(view.batch, &commands)? {
        BatchDecision::Accepted(plan) => plan,
        BatchDecision::Rejected(error) => return Ok(rejected(IndexRejection::Batch(error))),
    };
    let mut consumed = 0_u32;
    for (item, plan) in items.iter().zip(batch.items()) {
        let before = current(view, item.item)?;
        let before_mode =
            before.map_or(Ok(StateMode::Parallel), |entry| mode(view, entry.record))?;
        let priority = before.map_or(item.create_priority, |entry| entry.priority);
        let (after, after_mode) = if let Some(write) = plan.plan().write() {
            let record = &write.record;
            let after_mode = mode(view, record)?;
            if after_mode == StateMode::Fifo && priority != 0 && active(record) {
                return Ok(rejected(IndexRejection::FifoPriority));
            }
            let allocate = before.is_none_or(|entry| {
                entry.logical_state() != logical_state(record) && active(record)
            });
            let sequence = if allocate {
                let ordinal = order
                    .first_ordinal
                    .checked_add(consumed)
                    .ok_or(ApplyError::InvalidAuthority)?;
                consumed = consumed
                    .checked_add(1)
                    .ok_or(ApplyError::InvalidAuthority)?;
                (u128::from(order.identity.index().get()) << 32) + u128::from(ordinal)
            } else {
                before.ok_or(ApplyError::InvalidAuthority)?.state_enter_seq
            };
            (
                OrderingMetadata {
                    priority,
                    state_enter_seq: sequence,
                },
                after_mode,
            )
        } else {
            let entry = before.ok_or(ApplyError::InvalidAuthority)?;
            (
                OrderingMetadata {
                    priority,
                    state_enter_seq: entry.state_enter_seq,
                },
                before_mode,
            )
        };
        ordering.push(OrderingChange {
            before,
            before_mode,
            after,
            after_mode,
        });
    }
    Ok(IndexedDecision::Accepted(IndexedBatchPlan {
        batch,
        ordering,
        order,
        consumed,
    }))
}

pub(crate) fn decide_selected_claims<'view>(
    view: IndexedGroupView<'view>,
    items: &[IndexedBatchItem<'_>],
    order: EntryOrder,
    encoded_entry: &[u8],
    limits: AdmissionLimits,
) -> Result<IndexedDecision<'view>, ApplyError> {
    decide_with(
        view,
        items,
        order,
        encoded_entry,
        limits,
        admitted_claim_bytes,
    )
}

fn rejected(error: IndexRejection) -> IndexedDecision<'static> {
    IndexedDecision::Rejected(error)
}

fn current<'a>(
    view: IndexedGroupView<'a>,
    item: BatchItem<'_>,
) -> Result<Option<ClaimRecord<'a>>, ApplyError> {
    let id = ScopedRecordId {
        partition_key: item.partition.as_bytes(),
        id: item.command.id.as_bytes(),
    };
    match (view.batch.records.get(&id), view.records.get(&id)) {
        (None, None) => Ok(None),
        (Some(record), Some(entry))
            if entry.record == *record && ScopedRecordId::for_record(entry) == id =>
        {
            Ok(Some(*entry))
        }
        _ => Err(ApplyError::InvalidAuthority),
    }
}

fn validate(
    view: IndexedGroupView<'_>,
    item: IndexedBatchItem<'_>,
    limits: AdmissionLimits,
    bytes: &mut usize,
) -> Result<Option<IndexRejection>, ApplyError> {
    let Some(entry) = current(view, item.item)? else {
        return Ok((matches!(item.item.command.operation, Operation::Create(_))
            && item.create_priority > 2)
            .then_some(IndexRejection::InvalidPriority));
    };
    if entry.priority > 2 {
        return Err(ApplyError::InvalidAuthority);
    }
    match rows(entry, mode(view, entry.record)?) {
        None => {}
        Some(IndexRows::Parallel { due }) => {
            if view.parallel.get(&due) != Some(&entry)
                || LaneKey::for_record(&entry).is_some_and(|lane| view.fifo.contains_key(&lane))
            {
                return Err(ApplyError::InvalidAuthority);
            }
        }
        Some(IndexRows::Fifo {
            lane,
            member,
            head_candidate,
        }) => {
            if entry.priority != 0 {
                return Err(ApplyError::InvalidAuthority);
            }
            let members = view.fifo.get(&lane).ok_or(ApplyError::InvalidAuthority)?;
            if let Some(head_candidate) = head_candidate {
                if members.get(&member) != Some(&entry)
                    || view.parallel.contains_key(&head_candidate)
                {
                    return Err(ApplyError::InvalidAuthority);
                }
            } else {
                let row = members.get(&member).ok_or(ApplyError::InvalidAuthority)?;
                if row.partition_key.is_empty()
                    || row.partition_key.len() > wf_types::bounds::MAX_RECORD_ID_BYTES
                {
                    return Err(ApplyError::InvalidAuthority);
                }
                let row_bytes = validate_head(view, lane, member, *row, limits)?;
                if ScopedRecordId::for_record(row) != ScopedRecordId::for_record(&entry) {
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
                let (head_order, head) = members
                    .first_key_value()
                    .ok_or(ApplyError::InvalidAuthority)?;
                let head_bytes = validate_head(view, lane, *head_order, *head, limits)?;
                if let Err(error) = add(bytes, head_bytes).and_then(|()| limits.check_total(*bytes))
                {
                    return Ok(Some(IndexRejection::Batch(BatchRejection::Admission(
                        error,
                    ))));
                }
                if ScopedRecordId::for_record(head) != ScopedRecordId::for_record(&entry) {
                    return Ok(Some(IndexRejection::NotFifoHead));
                }
            }
        }
    }
    Ok(None)
}

fn validate_head(
    view: IndexedGroupView<'_>,
    lane: LaneKey<'_>,
    order: LaneOrder<'_>,
    head: ClaimRecord<'_>,
    limits: AdmissionLimits,
) -> Result<usize, ApplyError> {
    let mut head_bytes =
        record_bytes(head.record, limits).map_err(|_| ApplyError::InvalidAuthority)?;
    let id = ScopedRecordId::for_record(&head);
    let canonical = view
        .batch
        .records
        .get(&id)
        .ok_or(ApplyError::InvalidAuthority)?;
    let canonical_bytes =
        record_bytes(canonical, limits).map_err(|_| ApplyError::InvalidAuthority)?;
    add(&mut head_bytes, canonical_bytes).map_err(|_| ApplyError::InvalidAuthority)?;
    if head.priority != 0
        || LaneKey::for_record(&head) != Some(lane)
        || LaneOrder::for_record(&head) != order
        || view.records.get(&id) != Some(&head)
        || head.record != *canonical
        || crate::claim::DueOrder::for_record(&head)
            .is_some_and(|due| view.parallel.contains_key(&due))
    {
        return Err(ApplyError::InvalidAuthority);
    }
    Ok(head_bytes)
}

fn mode(view: IndexedGroupView<'_>, record: &WorkflowRecord) -> Result<StateMode, ApplyError> {
    let Some(policy) = view.policies.get(record.workflow_type()) else {
        return Ok(StateMode::Parallel);
    };
    if policy.workflow_type() != record.workflow_type() {
        return Err(ApplyError::InvalidAuthority);
    }
    Ok(policy
        .states()
        .get(logical_state(record))
        .and_then(|state| state.mode)
        .unwrap_or(StateMode::Parallel))
}
fn logical_state(record: &WorkflowRecord) -> &[u8] {
    if record.state() == b"running" {
        record.run_state().unwrap_or(b"queued")
    } else {
        record.state()
    }
}
fn active(record: &WorkflowRecord) -> bool {
    !matches!(
        logical_state(record),
        b"completed" | b"failed" | b"cancelled"
    )
}
