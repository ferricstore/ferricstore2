//! Full and observed index owners preserve source Step ordering and refusals.

use super::*;
use crate::claim::{ClaimRecord, DueOrder, LaneKey, LaneOrder, ScopedRecordId};
use crate::policy::definition::{
    FieldPatch, PolicyPatch, PolicySnapshot, StateChange, StateMode, StatePolicyPatch,
};
use crate::policy::installation::{InstallCommand, PolicyLimits, prepare_install};
use crate::workflow::admission::AdmissionLimits;
use crate::workflow::batch::{BatchItem, BatchRejection, GroupView};
use crate::workflow::indexed::{
    EntryOrder, IndexRejection, IndexRows, IndexedBatchItem, IndexedDecision, IndexedGroupView,
    OrderingMetadata, TouchedIndexedGroupView, decide_indexed_batch, decide_touched_indexed_batch,
};
use std::collections::BTreeMap;
use wf_types::identity::{GroupId, PartitionId};
use wf_types::version::{AppliedPosition, LogIdentity};

fn assert_step_ordering(
    entry: ClaimRecord<'_>,
    command: &CommittedCommand,
    policy: &PolicySnapshot,
    expected: Result<(OrderingMetadata, u32), IndexRejection>,
) -> TestResult {
    let group = GroupId::new(b"g".to_vec())?;
    let partition = PartitionId::new(entry.partition_key.to_vec())?;
    let placements = BTreeMap::from([(partition.clone(), group.clone())]);
    let id = ScopedRecordId::for_record(&entry);
    let canonical = BTreeMap::from([(id, entry.record)]);
    let records = BTreeMap::from([(id, entry)]);
    let due = DueOrder::for_record(&entry).ok_or("step predecessor due absent")?;
    let lane = LaneKey::for_record(&entry).ok_or("step predecessor lane absent")?;
    let member = LaneOrder::for_record(&entry);
    let fifo_mode = policy.state_mode(entry.logical_state()) == StateMode::Fifo;
    let parallel = std::iter::once((due, entry))
        .filter(|_| !fifo_mode)
        .collect();
    let fifo = std::iter::once((lane, BTreeMap::from([(member, entry)])))
        .filter(|_| fifo_mode)
        .collect();
    let policies = BTreeMap::from([(entry.record.workflow_type(), policy)]);
    let item = BatchItem::new(&partition, command, b"counter-boundary");
    let items = [IndexedBatchItem::new(item)];
    let order = EntryOrder {
        identity: LogIdentity::new(1, 7, AppliedPosition::new(9)),
        first_ordinal: 3,
    };
    let limits = AdmissionLimits::default();
    let view = IndexedGroupView {
        batch: GroupView {
            group: &group,
            placements: &placements,
            records: &canonical,
        },
        records: &records,
        parallel: &parallel,
        fifo: &fifo,
        policies: &policies,
    };
    let full = decide_indexed_batch(view, &items, order, b"", limits)?;
    let touched = {
        let policy = policy.clone();
        let current = BTreeMap::from([(id, Some(entry))]);
        let parallel = BTreeMap::from([(due, !fifo_mode)]);
        let members = BTreeMap::from([((lane, member), fifo_mode.then_some(entry))]);
        let heads = BTreeMap::from([(lane, fifo_mode.then_some((member, entry)))]);
        let policies = BTreeMap::from([(entry.record.workflow_type(), Some(&policy))]);
        let retries = BTreeMap::new();
        let view = TouchedIndexedGroupView {
            group: &group,
            placements: &placements,
            current: &current,
            parallel: &parallel,
            fifo_members: &members,
            fifo_heads: &heads,
            policies: &policies,
            retry_policies: &retries,
        };
        decide_touched_indexed_batch(view, &items, order, b"", limits)?
    };
    if full != touched {
        return Err("full and touched indexed step decisions differ".into());
    }
    match (full, expected) {
        (IndexedDecision::Rejected(actual), Err(error)) if actual == error => {}
        (IndexedDecision::Accepted(plan), Ok((metadata, consumed))) => {
            if !plan.metadata().eq([metadata])
                || plan.consumed_ordinals() != consumed
                || plan.next_ordinal() != Some(3 + consumed)
                || plan.batch().items().len() != 1
            {
                return Err("indexed step metadata or item count differs".into());
            }
            let changed = plan.batch().items()[0].plan();
            let history = changed.history().ok_or("indexed step history absent")?;
            let delta = plan.deltas().next().ok_or("step delta absent")?;
            let Operation::StepContinue(step) = &command.operation else {
                return Err("step fixture absent".into());
            };
            let expected_due = DueOrder {
                due: TimestampMillis::new(command.now.get() + step.lease_ms),
                state: &step.to,
                ..due
            };
            if changed.outcome() != Outcome::Changed
                || history.kind != EventKind::StepContinued
                || delta.before != Some(IndexRows::Parallel { due })
                || delta.after != Some(IndexRows::Parallel { due: expected_due })
            {
                return Err("indexed step outcome, history or rows differ".into());
            }
        }
        _ => return Err("unexpected indexed step decision".into()),
    }
    Ok(())
}

#[test]
fn step_continue_preserves_equal_parallel_state_order() -> TestResult {
    let record = step_running()?;
    let before = record.clone();
    let mut command = step_command(&record)?;
    let Operation::StepContinue(step) = &mut command.operation else {
        return Err("step fixture absent".into());
    };
    step.to = b"queued".to_vec();
    let policy = prepare_install(
        None,
        0,
        &InstallCommand::low_level(b"type".to_vec(), PolicyPatch::default()),
        PolicyLimits::default(),
    )?;
    let entry = ClaimRecord {
        record: &record,
        partition_key: b"p",
        priority: 0,
        state_enter_seq: 11,
    };
    assert_step_ordering(
        entry,
        &command,
        policy.policy(),
        Ok((
            OrderingMetadata {
                priority: 0,
                state_enter_seq: 11,
            },
            0,
        )),
    )?;
    let plan = reduce(Some(&record), &command)?;
    let next = &plan.write().ok_or("equal step write absent")?.record;
    assert_eq!((next.version(), next.fence().get()), (3, 2));
    assert_eq!(
        next.lease()
            .ok_or("equal step lease absent")?
            .token
            .as_bytes(),
        b"worker:2:2"
    );
    assert_eq!(record, before);
    Ok(())
}

#[test]
fn step_continue_stamps_parallel_entry_and_rejects_running_fifo_entry() -> TestResult {
    for (from, priority, fifo, stale) in [
        (b"reserve".as_slice(), 0, false, false),
        (b"reserve".as_slice(), 0, true, false),
        (b"reserve".as_slice(), 1, true, false),
        (b"queued".as_slice(), 0, true, false),
        (b"reserve".as_slice(), 0, true, true),
    ] {
        let mut record = step_running()?;
        record.run_state = Some(from.to_vec());
        let before = record.clone();
        let mut command = step_command(&record)?;
        let Operation::StepContinue(step) = &mut command.operation else {
            return Err("step fixture absent".into());
        };
        if from == b"queued" {
            step.to = b"queued".to_vec();
        }
        if stale {
            step.guard.token = LeaseToken::new(b"wrong".to_vec())?;
        }
        let patch = PolicyPatch {
            states: if fifo {
                BTreeMap::from([(
                    step.to.clone(),
                    StateChange::Patch(StatePolicyPatch {
                        mode: FieldPatch::Set(StateMode::Fifo),
                        ..StatePolicyPatch::default()
                    }),
                )])
            } else {
                BTreeMap::new()
            },
            ..PolicyPatch::default()
        };
        let policy = prepare_install(
            None,
            0,
            &InstallCommand::low_level(b"type".to_vec(), patch),
            PolicyLimits::default(),
        )?;
        let entry = ClaimRecord {
            record: &record,
            partition_key: b"p",
            priority,
            state_enter_seq: 11,
        };
        let expected = if stale {
            Err(IndexRejection::Batch(BatchRejection::Domain(
                DomainError::StaleLease,
            )))
        } else if fifo {
            Err(IndexRejection::DirectRunningFifoEntry)
        } else {
            Ok((
                OrderingMetadata {
                    priority,
                    state_enter_seq: (9_u128 << 32) + 3,
                },
                1,
            ))
        };
        assert_step_ordering(entry, &command, policy.policy(), expected)?;
        assert_eq!(record, before);
    }
    Ok(())
}
