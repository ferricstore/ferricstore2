//! Shared source-fixture publication from saved FIFO and parallel index keys.

use super::*;
use std::collections::BTreeSet;
use wf_domain::claim::{
    AuthoritativeClaimView, RecordIndex, SelectionAction, SelectionRequest, StateFilter, select,
};
use wf_domain::history::EventKind;
use wf_domain::workflow::batch::BatchItem;
use wf_domain::workflow::indexed::{
    EntryOrder, IndexDelta, IndexRows, IndexedBatchItem, IndexedDecision, OrderingMetadata,
    decide_indexed_batch,
};

pub(super) type Records = BTreeMap<Vec<u8>, (WorkflowRecord, OrderingMetadata)>;
pub(super) type FifoMembers = BTreeSet<(i8, u128, Vec<u8>)>;
pub(super) type ParallelMembers = BTreeSet<(u8, u64, Vec<u8>, Vec<u8>)>;

pub(super) fn queued_policy(workflow_type: &[u8]) -> Result<PolicyCandidate, TestError> {
    let patch = PolicyPatch {
        states: BTreeMap::from([(
            b"queued".to_vec(),
            StateChange::Patch(StatePolicyPatch {
                mode: FieldPatch::Set(StateMode::Fifo),
                ..StatePolicyPatch::default()
            }),
        )]),
        ..PolicyPatch::default()
    };
    Ok(prepare_install(
        None,
        0,
        &InstallCommand::low_level(workflow_type.to_vec(), patch),
        PolicyLimits::default(),
    )?)
}

fn checked_row<'view>(
    scope: (&'view [u8], &'view [u8]),
    indexed: &RecordIndex<'view>,
    id: &'view [u8],
) -> Result<ClaimRecord<'view>, TestError> {
    let row = *indexed
        .get(&ScopedRecordId {
            partition_key: scope.1,
            id,
        })
        .ok_or("saved index key has no canonical record")?;
    assert_eq!(row.record.id().as_bytes(), id);
    assert_eq!((row.record.workflow_type(), row.partition_key), scope);
    Ok(row)
}

fn with_published<T>(
    scope: (&[u8], &[u8]),
    rows: &Records,
    members: &FifoMembers,
    parallel_members: &ParallelMembers,
    policy: &PolicySnapshot,
    revision: u64,
    run: impl for<'view> FnOnce(
        IndexedGroupView<'view>,
        AuthoritativeClaimView<'view>,
    ) -> Result<T, TestError>,
) -> Result<T, TestError> {
    assert_eq!(policy.workflow_type(), scope.0);
    let entries: Vec<_> = rows
        .iter()
        .map(|(id, (record, metadata))| {
            assert_eq!(id.as_slice(), record.id().as_bytes());
            ClaimRecord {
                record,
                partition_key: scope.1,
                priority: metadata.priority,
                state_enter_seq: metadata.state_enter_seq,
            }
        })
        .collect();
    let indexed: RecordIndex<'_> = entries
        .iter()
        .map(|row| (ScopedRecordId::for_record(row), *row))
        .collect();
    let canonical: BTreeMap<_, _> = entries
        .iter()
        .map(|row| (ScopedRecordId::for_record(row), row.record))
        .collect();
    let mut fifo = BTreeMap::new();
    for (score, sequence, id) in members {
        let row = checked_row(scope, &indexed, id)?;
        let lane = LaneKey::for_record(&row).ok_or("saved terminal FIFO member")?;
        assert_eq!(policy.state_mode(row.logical_state()), StateMode::Fifo);
        let order = LaneOrder::for_record(&row);
        assert_eq!(
            (*score, *sequence, id.as_slice()),
            (order.score(), order.sequence(), order.id())
        );
        assert!(
            fifo.entry(lane)
                .or_insert_with(BTreeMap::new)
                .insert(order, row)
                .is_none()
        );
    }
    let mut parallel = BTreeMap::new();
    for (priority, due, id, state) in parallel_members {
        let row = checked_row(scope, &indexed, id)?;
        let key = DueOrder::for_record(&row).ok_or("saved unscheduled parallel member")?;
        assert_eq!(policy.state_mode(row.logical_state()), StateMode::Parallel);
        assert_eq!(
            (*priority, *due, id.as_slice(), state.as_slice()),
            (key.priority, key.due.get(), key.id, key.state)
        );
        assert_eq!((key.workflow_type, key.partition_key), scope);
        assert!(parallel.insert(key, row).is_none());
    }
    let group = GroupId::new(b"g".to_vec())?;
    let partition = PartitionId::new(scope.1.to_vec())?;
    let placements = BTreeMap::from([(partition, group.clone())]);
    let policies = BTreeMap::from([(scope.0, policy)]);
    run(
        IndexedGroupView {
            batch: GroupView {
                group: &group,
                placements: &placements,
                records: &canonical,
            },
            records: &indexed,
            parallel: &parallel,
            fifo: &fifo,
            policies: &policies,
        },
        AuthoritativeClaimView {
            group: group.as_bytes(),
            revision,
            records: &indexed,
            parallel: &parallel,
            fifo: &fifo,
        },
    )
}

fn update_row(
    scope: (&[u8], &[u8]),
    row: IndexRows<'_>,
    insert: bool,
    fifo: &mut FifoMembers,
    parallel: &mut ParallelMembers,
) -> TestResult {
    match row {
        IndexRows::Fifo {
            lane,
            member,
            head_candidate,
        } => {
            assert_eq!(
                (lane.workflow_type, lane.state, lane.partition_key),
                (scope.0, b"queued".as_slice(), scope.1)
            );
            if let Some(due) = head_candidate {
                assert_eq!(
                    (
                        due.workflow_type,
                        due.state,
                        due.partition_key,
                        due.id,
                        due.priority
                    ),
                    (scope.0, lane.state, scope.1, member.id(), 0)
                );
            }
            let key = (member.score(), member.sequence(), member.id().to_vec());
            let changed = if insert {
                fifo.insert(key)
            } else {
                fifo.remove(&key)
            };
            if !changed {
                return Err("returned FIFO key did not change saved membership".into());
            }
        }
        IndexRows::Parallel { due } => {
            assert_eq!((due.workflow_type, due.partition_key), scope);
            let key = (
                due.priority,
                due.due.get(),
                due.id.to_vec(),
                due.state.to_vec(),
            );
            let changed = if insert {
                parallel.insert(key)
            } else {
                parallel.remove(&key)
            };
            if !changed {
                return Err("returned parallel key did not change saved membership".into());
            }
        }
    }
    Ok(())
}

fn check_delta(
    scope: (&[u8], &[u8]),
    command: &CommittedCommand,
    history_kind: EventKind,
    delta: &IndexDelta<'_>,
    previous: Option<&WorkflowRecord>,
) -> TestResult {
    let (kind, expected_dues) = match &command.operation {
        Operation::Create(_) => (EventKind::Created, (None, Some(Some(2_000)))),
        Operation::Cancel(_) => (EventKind::Cancelled, (Some(Some(2_000)), None)),
        Operation::Claim(claim) => (
            EventKind::Claimed,
            (
                Some(Some(2_000)),
                Some(Some(command.now.get() + claim.lease_ms)),
            ),
        ),
        Operation::Reclaim(claim) => {
            let previous = previous.ok_or("reclaim has no canonical predecessor")?;
            assert_eq!(previous.state(), b"running");
            assert_eq!(
                previous.next_run_at(),
                previous.lease().map(|lease| lease.deadline)
            );
            (
                EventKind::Claimed,
                (
                    Some(previous.next_run_at().map(TimestampMillis::get)),
                    Some(Some(command.now.get() + claim.lease_ms)),
                ),
            )
        }
        Operation::Complete(_) => (EventKind::Completed, (Some(Some(32_000)), None)),
        Operation::Transition(_) => (
            EventKind::Transitioned,
            (Some(Some(32_000)), Some(Some(5_000))),
        ),
        _ => return Err("unexpected source FIFO operation".into()),
    };
    let row_due = |row: IndexRows<'_>| match row {
        IndexRows::Fifo { head_candidate, .. } => head_candidate.map(|due| due.due.get()),
        IndexRows::Parallel { due } => Some(due.due.get()),
    };
    assert_eq!(history_kind, kind);
    assert_eq!(
        (delta.before.map(row_due), delta.after.map(row_due)),
        expected_dues
    );
    assert_eq!(
        delta.before.is_none(),
        matches!(&command.operation, Operation::Create(_))
    );
    assert!(
        delta
            .before
            .is_none_or(|row| matches!(row, IndexRows::Fifo { .. }))
    );
    if let Some(after) = delta.after {
        assert_eq!(
            matches!(after, IndexRows::Parallel { .. }),
            matches!(&command.operation, Operation::Transition(_))
        );
    }
    for row in [delta.before, delta.after].into_iter().flatten() {
        let id = match row {
            IndexRows::Fifo { lane, member, .. } => {
                assert_eq!((lane.workflow_type, lane.partition_key), scope);
                member.id()
            }
            IndexRows::Parallel { due } => {
                assert_eq!((due.workflow_type, due.partition_key), scope);
                due.id
            }
        };
        assert_eq!(id, command.id.as_bytes());
    }
    Ok(())
}

pub(super) fn publish(
    scope: (&[u8], &[u8]),
    rows: &mut Records,
    members: &mut FifoMembers,
    parallel: &mut ParallelMembers,
    policy: &PolicySnapshot,
    command: &CommittedCommand,
    index: u64,
) -> TestResult {
    let previous = rows.get(command.id.as_bytes());
    let predecessor = previous.map(|(record, _)| record);
    let mut next_members = members.clone();
    let mut next_parallel = parallel.clone();
    let (record, metadata) = with_published(
        scope,
        rows,
        members,
        parallel,
        policy,
        index - 1,
        |view, _| {
            let partition = PartitionId::new(scope.1.to_vec())?;
            let items = [IndexedBatchItem::new(BatchItem::new(
                &partition,
                command,
                command.id.as_bytes(),
            ))];
            let IndexedDecision::Accepted(plan) = decide_indexed_batch(
                view,
                &items,
                EntryOrder {
                    identity: LogIdentity::new(1, 7, AppliedPosition::new(index)),
                    first_ordinal: 0,
                },
                b"fifo-middle-entry",
                AdmissionLimits::default(),
            )?
            else {
                return Err("source FIFO command rejected".into());
            };
            assert_eq!(plan.batch().items().len(), 1);
            let changed = plan.batch().items()[0].plan();
            assert_eq!(changed.outcome(), Outcome::Changed);
            let write = changed.write().ok_or("accepted command has no write")?;
            let history = changed.history().ok_or("accepted command has no history")?;
            let metadata = plan.metadata().next().ok_or("missing ordering metadata")?;
            let enters = matches!(
                &command.operation,
                Operation::Create(_) | Operation::Transition(_)
            );
            let expected_sequence = if enters {
                u128::from(index) << 32
            } else {
                previous
                    .ok_or("missing predecessor metadata")?
                    .1
                    .state_enter_seq
            };
            assert_eq!(
                (metadata.priority, metadata.state_enter_seq),
                (0, expected_sequence)
            );
            assert_eq!(plan.consumed_ordinals(), u32::from(enters));
            let delta = plan.deltas().next().ok_or("missing publication delta")?;
            check_delta(scope, command, history.kind, &delta, predecessor)?;
            assert_eq!(write.record.id(), &command.id);
            assert_eq!(write.record.workflow_type(), scope.0);
            assert_eq!(write.record.payload(), Some(command.id.as_bytes()));
            for (row, insert) in [(delta.before, false), (delta.after, true)] {
                if let Some(row) = row {
                    update_row(scope, row, insert, &mut next_members, &mut next_parallel)?;
                }
            }
            Ok((write.record.clone(), metadata))
        },
    )?;
    *members = next_members;
    *parallel = next_parallel;
    rows.insert(record.id().as_bytes().to_vec(), (record, metadata));
    Ok(())
}

pub(super) fn claim_one(
    scope: (&[u8], &[u8]),
    rows: &mut Records,
    members: &mut FifoMembers,
    parallel: &mut ParallelMembers,
    policy: &PolicySnapshot,
    step: (u64, u64, usize),
    expected_id: &[u8],
) -> Result<(usize, Option<WorkflowRecord>), TestError> {
    let (now, index, limit) = step;
    let partitions = [scope.1];
    let states = [b"queued".as_slice()];
    let request = SelectionRequest {
        partitions: PartitionFilter::Only(&partitions),
        states: StateFilter::Only(&states),
        limit,
        ..SelectionRequest::new(b"g", scope.0, TimestampMillis::new(now))
    };
    let claim = Claim {
        expected_state: b"queued".to_vec(),
        worker: b"fifo-worker".to_vec(),
        token: LeaseToken::new(format!("fifo-worker:{now}:1").into_bytes())?,
        lease_ms: 30_000,
    };
    claim_requested(
        scope,
        rows,
        members,
        parallel,
        policy,
        (index, request, claim),
        expected_id,
    )
}

pub(super) fn claim_requested(
    scope: (&[u8], &[u8]),
    rows: &mut Records,
    members: &mut FifoMembers,
    parallel: &mut ParallelMembers,
    policy: &PolicySnapshot,
    step: (u64, SelectionRequest<'_>, Claim),
    expected_id: &[u8],
) -> Result<(usize, Option<WorkflowRecord>), TestError> {
    let (index, request, claim) = step;
    let (count, selected) = with_published(
        scope,
        rows,
        members,
        parallel,
        policy,
        index - 1,
        |_, view| {
            assert_eq!(
                (request.group, request.workflow_type),
                (view.group, scope.0)
            );
            let partitions = [scope.1];
            let states = [b"queued".as_slice()];
            assert_eq!(request.partitions, PartitionFilter::Only(&partitions));
            assert_eq!(request.states, StateFilter::Only(&states));
            let selection = select(&view, request, None)?;
            let count = selection.records().len();
            if count != 1 {
                return Ok((count, None));
            }
            let selected = selection.records()[0];
            assert_eq!(selected.record.id().as_bytes(), expected_id);
            assert_eq!(selected.partition_key, scope.1);
            assert_eq!(selected.record.state(), claim.expected_state);
            assert_eq!(
                selected.action,
                if claim.expected_state == b"running" {
                    SelectionAction::Reclaim
                } else {
                    SelectionAction::Claim
                }
            );
            Ok((
                count,
                Some((selected.record.id().as_bytes().to_vec(), selected.action)),
            ))
        },
    )?;
    let Some((selected, action)) = selected else {
        return Ok((count, None));
    };
    let command = CommittedCommand {
        id: RecordId::new(selected.clone())?,
        now: request.now,
        operation: match action {
            SelectionAction::Claim => Operation::Claim(claim),
            SelectionAction::Reclaim => Operation::Reclaim(claim),
        },
    };
    publish(scope, rows, members, parallel, policy, &command, index)?;
    let record = &rows
        .get(selected.as_slice())
        .ok_or("claimed record absent")?
        .0;
    assert_eq!(record.id().as_bytes(), selected.as_slice());
    Ok((count, Some(record.clone())))
}
