//! Source-derived FIFO middle cancellation; saved rows advance only by plan deltas.

use super::*;
use std::collections::BTreeSet;
use wf_domain::claim::{
    AuthoritativeClaimView, SelectionAction, SelectionRequest, StateFilter, select,
};
use wf_domain::history::EventKind;
use wf_domain::workflow::Cancel;
use wf_domain::workflow::batch::BatchItem;
use wf_domain::workflow::indexed::{
    EntryOrder, IndexRows, IndexedBatchItem, IndexedDecision, OrderingMetadata,
    decide_indexed_batch,
};
use wf_types::version::Fence;

const TYPE: &[u8] = b"flow-invariant-fifo-middle-cancel-type:2";
const PARTITION: &[u8] = b"tenant:fifo-middle-cancel:1";
const IDS: [&[u8]; 3] = [
    b"fifo-middle-cancel-first:1",
    b"fifo-middle-cancel-second:1",
    b"fifo-middle-cancel-third:1",
];

type FifoRow = ((i8, u128, Vec<u8>), Option<u64>);

fn source_fifo_policy() -> Result<PolicyCandidate, TestError> {
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
        &InstallCommand::low_level(TYPE.to_vec(), patch),
        PolicyLimits::default(),
    )?)
}

fn with_published_fifo<T>(
    rows: &BTreeMap<Vec<u8>, (WorkflowRecord, OrderingMetadata)>,
    members: &BTreeSet<(i8, u128, Vec<u8>)>,
    policy: &PolicySnapshot,
    revision: u64,
    run: impl for<'view> FnOnce(
        IndexedGroupView<'view>,
        AuthoritativeClaimView<'view>,
    ) -> Result<T, TestError>,
) -> Result<T, TestError> {
    let entries: Vec<_> = rows
        .iter()
        .map(|(id, (record, metadata))| {
            assert_eq!(id.as_slice(), record.id().as_bytes());
            ClaimRecord {
                record,
                partition_key: PARTITION,
                priority: metadata.priority,
                state_enter_seq: metadata.state_enter_seq,
            }
        })
        .collect();
    let indexed: BTreeMap<_, _> = entries
        .iter()
        .map(|row| (ScopedRecordId::for_record(row), *row))
        .collect();
    let canonical: BTreeMap<_, _> = entries
        .iter()
        .map(|row| (ScopedRecordId::for_record(row), row.record))
        .collect();
    let mut fifo = BTreeMap::new();
    for (score, sequence, id) in members {
        let row = *indexed
            .get(&ScopedRecordId {
                partition_key: PARTITION,
                id: id.as_slice(),
            })
            .ok_or("saved member has no canonical record")?;
        let lane = LaneKey::for_record(&row).ok_or("saved terminal FIFO member")?;
        assert_eq!(
            (lane.workflow_type, lane.state, lane.partition_key),
            (TYPE, b"queued".as_slice(), PARTITION)
        );
        let order = LaneOrder::for_record(&row);
        assert_eq!(
            (*score, *sequence, id.as_slice()),
            (order.score(), order.sequence(), order.id())
        );
        fifo.entry(lane)
            .or_insert_with(BTreeMap::new)
            .insert(order, row);
    }
    let group = GroupId::new(b"g".to_vec())?;
    let partition = PartitionId::new(PARTITION.to_vec())?;
    let placements = BTreeMap::from([(partition, group.clone())]);
    let parallel = BTreeMap::new();
    let policies = BTreeMap::from([(TYPE, policy)]);
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

fn fifo_row_key(row: IndexRows<'_>) -> Result<FifoRow, TestError> {
    let IndexRows::Fifo {
        lane,
        member,
        head_candidate,
    } = row
    else {
        return Err("unexpected parallel row in source FIFO sequence".into());
    };
    assert_eq!(
        (lane.workflow_type, lane.state, lane.partition_key),
        (TYPE, b"queued".as_slice(), PARTITION)
    );
    if let Some(due) = head_candidate {
        assert_eq!(
            (due.workflow_type, due.state, due.partition_key),
            (TYPE, b"queued".as_slice(), PARTITION)
        );
        assert_eq!(due.id, member.id());
        assert_eq!(due.priority, 0);
    }
    Ok((
        (member.score(), member.sequence(), member.id().to_vec()),
        head_candidate.map(|due| due.due.get()),
    ))
}

fn publish_fifo_command(
    rows: &mut BTreeMap<Vec<u8>, (WorkflowRecord, OrderingMetadata)>,
    members: &mut BTreeSet<(i8, u128, Vec<u8>)>,
    policy: &PolicySnapshot,
    command: &CommittedCommand,
    index: u64,
) -> TestResult {
    let previous = rows.get(command.id.as_bytes()).map(|(_, meta)| *meta);
    let (record, metadata, before, after) =
        with_published_fifo(rows, members, policy, index - 1, |view, _| {
            let partition = PartitionId::new(PARTITION.to_vec())?;
            let items = [IndexedBatchItem::new(BatchItem::new(
                &partition,
                command,
                command.id.as_bytes(),
            ))];
            let decision = decide_indexed_batch(
                view,
                &items,
                EntryOrder {
                    identity: LogIdentity::new(1, 7, AppliedPosition::new(index)),
                    first_ordinal: 0,
                },
                b"fifo-middle-entry",
                AdmissionLimits::default(),
            )?;
            let IndexedDecision::Accepted(plan) = decision else {
                return Err("source FIFO command rejected".into());
            };
            assert_eq!(plan.batch().items().len(), 1);
            let changed = plan.batch().items()[0].plan();
            assert_eq!(changed.outcome(), Outcome::Changed);
            let write = changed.write().ok_or("accepted command has no write")?;
            let history = changed.history().ok_or("accepted command has no history")?;
            let metadata = plan.metadata().next().ok_or("missing ordering metadata")?;
            assert_eq!(metadata.priority, 0);
            assert_eq!(
                metadata.state_enter_seq,
                previous.map_or(u128::from(index) << 32, |meta| meta.state_enter_seq)
            );
            let delta = plan.deltas().next().ok_or("missing publication delta")?;
            let before = delta.before.map(fifo_row_key).transpose()?;
            let after = delta.after.map(fifo_row_key).transpose()?;
            let (kind, expected_dues) = match &command.operation {
                Operation::Create(_) => (EventKind::Created, (None, Some(Some(2_000)))),
                Operation::Cancel(_) => (EventKind::Cancelled, (Some(Some(2_000)), None)),
                Operation::Claim(_) => (
                    EventKind::Claimed,
                    (Some(Some(2_000)), Some(Some(command.now.get() + 30_000))),
                ),
                Operation::Complete(_) => (EventKind::Completed, (Some(Some(32_000)), None)),
                _ => return Err("unexpected source FIFO operation".into()),
            };
            assert_eq!(history.kind, kind);
            assert_eq!(
                (
                    before.as_ref().map(|(_, due)| *due),
                    after.as_ref().map(|(_, due)| *due)
                ),
                expected_dues
            );
            assert_eq!(
                plan.consumed_ordinals(),
                u32::from(matches!(&command.operation, Operation::Create(_)))
            );
            assert_eq!(write.record.id(), &command.id);
            assert_eq!(write.record.workflow_type(), TYPE);
            assert_eq!(write.record.payload(), Some(command.id.as_bytes()));
            for (key, _) in before.iter().chain(after.iter()) {
                assert_eq!(key.2.as_slice(), command.id.as_bytes());
            }
            Ok((
                write.record.clone(),
                metadata,
                before.map(|(key, _)| key),
                after.map(|(key, _)| key),
            ))
        })?;
    if let Some(key) = before {
        assert!(
            members.remove(&key),
            "returned predecessor row was not published"
        );
    }
    if let Some(key) = after {
        assert!(
            members.insert(key),
            "returned after row was already published"
        );
    }
    rows.insert(record.id().as_bytes().to_vec(), (record, metadata));
    Ok(())
}

fn claim_fifo_one(
    rows: &mut BTreeMap<Vec<u8>, (WorkflowRecord, OrderingMetadata)>,
    members: &mut BTreeSet<(i8, u128, Vec<u8>)>,
    policy: &PolicySnapshot,
    now: u64,
    index: u64,
    expected_id: &[u8],
) -> Result<(usize, Option<WorkflowRecord>), TestError> {
    let (count, selected) = with_published_fifo(rows, members, policy, index - 1, |_, view| {
        let partitions = [PARTITION];
        let states = [b"queued".as_slice()];
        let selection = select(
            &view,
            SelectionRequest {
                partitions: PartitionFilter::Only(&partitions),
                states: StateFilter::Only(&states),
                limit: 1,
                ..SelectionRequest::new(view.group, TYPE, TimestampMillis::new(now))
            },
            None,
        )?;
        let count = selection.records().len();
        if count != 1 {
            return Ok((count, None));
        }
        let selected = selection.records()[0];
        assert_eq!(selected.record.id().as_bytes(), expected_id);
        assert_eq!(selected.partition_key, PARTITION);
        assert_eq!(selected.action, SelectionAction::Claim);
        Ok((count, Some(selected.record.id().as_bytes().to_vec())))
    })?;
    let Some(selected) = selected else {
        return Ok((count, None));
    };
    let command = CommittedCommand {
        id: RecordId::new(selected.clone())?,
        now: TimestampMillis::new(now),
        operation: Operation::Claim(Claim {
            expected_state: b"queued".to_vec(),
            worker: b"fifo-worker".to_vec(),
            token: LeaseToken::new(format!("fifo-worker:{now}:1").into_bytes())?,
            lease_ms: 30_000,
        }),
    };
    publish_fifo_command(rows, members, policy, &command, index)?;
    let record = &rows
        .get(selected.as_slice())
        .ok_or("claimed record absent")?
        .0;
    assert_eq!(record.id().as_bytes(), selected.as_slice());
    Ok((count, Some(record.clone())))
}

#[test]
fn fifo_cancelled_middle_preserves_source_claim_order() -> TestResult {
    // invariant_test.exs723–770, declaration75afbefe8cf622e2. Source startup was blocked.
    let installed = source_fifo_policy()?;
    let policy = installed.policy();
    let mut rows = BTreeMap::new();
    let mut members = BTreeSet::new();
    for (index, id, now) in [(1, IDS[0], 1_000), (2, IDS[1], 1_001), (3, IDS[2], 1_002)] {
        let command = CommittedCommand {
            id: RecordId::new(id.to_vec())?,
            now: TimestampMillis::new(now),
            operation: Operation::Create(Create {
                workflow_type: TYPE.to_vec(),
                state: b"queued".to_vec(),
                run_at: Some(TimestampMillis::new(2_000)),
                payload: Some(id.to_vec()),
            }),
        };
        publish_fifo_command(&mut rows, &mut members, policy, &command, index)?;
    }
    let mut expected = BTreeSet::from([
        (0, 1_u128 << 32, IDS[0].to_vec()),
        (0, 2_u128 << 32, IDS[1].to_vec()),
        (0, 3_u128 << 32, IDS[2].to_vec()),
    ]);
    assert_eq!(members, expected);
    let cancel = CommittedCommand {
        id: RecordId::new(IDS[1].to_vec())?,
        now: TimestampMillis::new(1_500),
        operation: Operation::Cancel(Cancel {
            fence: Fence::new(0),
            lease_token: None,
            expected_state: None,
            expected_version: None,
            error: None,
        }),
    };
    let cancelled = publish_fifo_command(&mut rows, &mut members, policy, &cancel, 4);
    assert!(cancelled.is_ok(), "{cancelled:?}");
    cancelled?;
    expected.remove(&(0, 2_u128 << 32, IDS[1].to_vec()));
    assert_eq!(members, expected);
    assert_eq!(
        (rows[IDS[1]].0.state(), rows[IDS[1]].0.version()),
        (b"cancelled".as_slice(), 2)
    );
    let (first_count, first) = claim_fifo_one(&mut rows, &mut members, policy, 2_000, 5, IDS[0])?;
    assert_eq!(first_count, 1);
    let first = first.ok_or("source first claim returned no record")?;
    assert_eq!(first.id().as_bytes(), IDS[0]);
    expected.remove(&(0, 1_u128 << 32, IDS[0].to_vec()));
    expected.insert((-1, 1_u128 << 32, IDS[0].to_vec()));
    assert_eq!(members, expected);
    let complete = CommittedCommand {
        id: first.id().clone(),
        now: TimestampMillis::new(2_100),
        operation: Operation::Complete(Terminal {
            guard: LeaseGuard {
                token: first
                    .lease()
                    .ok_or("source first lease absent")?
                    .token
                    .clone(),
                fence: first.fence(),
            },
            payload: None,
            value: None,
        }),
    };
    let completed = publish_fifo_command(&mut rows, &mut members, policy, &complete, 6);
    assert!(completed.is_ok(), "{completed:?}");
    completed?;
    expected.remove(&(-1, 1_u128 << 32, IDS[0].to_vec()));
    assert_eq!(members, expected);
    assert_eq!(
        (rows[IDS[0]].0.state(), rows[IDS[0]].0.version()),
        (b"completed".as_slice(), 3)
    );
    let (third_count, third) = claim_fifo_one(&mut rows, &mut members, policy, 2_101, 7, IDS[2])?;
    assert_eq!(third_count, 1);
    let third = third.ok_or("source third claim returned no record")?;
    assert_eq!(third.id().as_bytes(), IDS[2]);
    expected.remove(&(0, 3_u128 << 32, IDS[2].to_vec()));
    expected.insert((-1, 3_u128 << 32, IDS[2].to_vec()));
    assert_eq!(members, expected);
    assert_eq!(third.version(), 2);
    Ok(())
}
