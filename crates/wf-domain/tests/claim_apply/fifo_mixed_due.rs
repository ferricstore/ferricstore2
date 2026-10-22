//! Source mixed-state claims retain parallel due order through indexed publication.

use super::fifo_indexes::{FifoMembers, ParallelMembers, Records, publish_checked, with_published};
use super::*;
use std::collections::BTreeSet;
use wf_domain::claim::{SelectionAction, SelectionRequest, StateFilter, select};
use wf_domain::history::EventKind;
use wf_domain::workflow::indexed::IndexRows;

const TYPE: &[u8] = b"flow-invariant-fifo-mixed-type:2";
const PARTITION: &[u8] = b"tenant:fifo-mixed:1";
const IDS: [&[u8]; 2] = [b"parallel-later-due:1", b"parallel-earlier-due:1"];

#[test]
fn mixed_fifo_policy_publishes_source_parallel_due_order() -> TestResult {
    // invariant_test.exs1102-1147, bea9ae0f6f1e970c; prepared policy is fixture authority.
    let patch = PolicyPatch {
        states: BTreeMap::from([
            (
                b"fifo_gate".to_vec(),
                StateChange::Patch(StatePolicyPatch {
                    mode: FieldPatch::Set(StateMode::Fifo),
                    ..StatePolicyPatch::default()
                }),
            ),
            (
                b"retry".to_vec(),
                StateChange::Patch(StatePolicyPatch {
                    mode: FieldPatch::Set(StateMode::Parallel),
                    ..StatePolicyPatch::default()
                }),
            ),
        ]),
        ..PolicyPatch::default()
    };
    let installed = prepare_install(
        None,
        0,
        &InstallCommand::low_level(TYPE.to_vec(), patch),
        PolicyLimits::default(),
    )?;
    let policy = installed.policy();
    let mut rows = BTreeMap::new();
    let mut fifo = BTreeSet::new();
    let mut parallel = BTreeSet::new();
    let later = CommittedCommand {
        id: RecordId::new(IDS[0].to_vec())?,
        now: TimestampMillis::new(1_000),
        operation: Operation::Create(Create {
            workflow_type: TYPE.to_vec(),
            state: b"retry".to_vec(),
            run_at: Some(TimestampMillis::new(10_000)),
            payload: Some(b"later due".to_vec()),
        }),
    };
    let created_later = publish_mixed_due(&mut rows, &mut fifo, &mut parallel, policy, &later, 1);
    assert!(created_later.is_ok(), "{created_later:?}");
    created_later?;
    let earlier = CommittedCommand {
        id: RecordId::new(IDS[1].to_vec())?,
        now: TimestampMillis::new(1_001),
        operation: Operation::Create(Create {
            workflow_type: TYPE.to_vec(),
            state: b"retry".to_vec(),
            run_at: Some(TimestampMillis::new(2_000)),
            payload: Some(b"earlier due".to_vec()),
        }),
    };
    let created_earlier =
        publish_mixed_due(&mut rows, &mut fifo, &mut parallel, policy, &earlier, 2);
    assert!(created_earlier.is_ok(), "{created_earlier:?}");
    created_earlier?;
    let (count, claimed) = claim_mixed_due(&mut rows, &mut fifo, &mut parallel, policy)?;
    assert_eq!(count, 1);
    let claimed = claimed.ok_or("source singleton claim returned no record")?;
    assert_eq!(claimed.id().as_bytes(), IDS[1]);
    assert!(fifo.is_empty());
    assert_eq!(
        parallel,
        ParallelMembers::from([
            (0, 10_000, IDS[0].to_vec(), b"retry".to_vec()),
            (0, 40_000, IDS[1].to_vec(), b"retry".to_vec()),
        ])
    );
    assert_eq!(rows[IDS[0]].1.state_enter_seq, 1_u128 << 32);
    assert_eq!(rows[IDS[1]].1.state_enter_seq, 2_u128 << 32);
    assert_eq!((claimed.version(), claimed.fence().get()), (2, 1));
    assert_eq!(claimed.payload(), Some(b"earlier due".as_slice()));
    Ok(())
}

fn publish_mixed_due(
    rows: &mut Records,
    fifo: &mut FifoMembers,
    parallel: &mut ParallelMembers,
    policy: &PolicySnapshot,
    command: &CommittedCommand,
    index: u64,
) -> Result<WorkflowRecord, TestError> {
    publish_checked(
        (TYPE, PARTITION),
        rows,
        fifo,
        parallel,
        policy,
        (index, command, b"fifo_gate"),
        |previous, plan| {
            let changed = plan.batch().items()[0].plan();
            let write = changed.write().ok_or("source mixed write absent")?;
            let history = changed.history().ok_or("source mixed history absent")?;
            let (kind, due, sequence, payload, enters) = match &command.operation {
                Operation::Create(_) => {
                    let (id, now, due, payload) = match index {
                        1 => (IDS[0], 1_000, 10_000, b"later due".as_slice()),
                        2 => (IDS[1], 1_001, 2_000, b"earlier due".as_slice()),
                        _ => return Err("unexpected mixed create index".into()),
                    };
                    assert!(previous.is_none());
                    assert_eq!(command.id.as_bytes(), id);
                    assert_eq!(
                        (command.now.get(), write.record.created_at().get()),
                        (now, now)
                    );
                    assert_eq!(write.record.state(), b"retry");
                    assert_eq!((write.record.version(), write.record.fence().get()), (1, 0));
                    (
                        EventKind::Created,
                        due,
                        u128::from(index) << 32,
                        Some(payload),
                        true,
                    )
                }
                Operation::Claim(_) => {
                    let (old, metadata) = previous.ok_or("source mixed predecessor absent")?;
                    assert_eq!(command.now.get(), 10_000);
                    assert_eq!(write.record.state(), b"running");
                    assert_eq!(write.record.run_state(), Some(b"retry".as_slice()));
                    (
                        EventKind::Claimed,
                        40_000,
                        metadata.state_enter_seq,
                        old.payload(),
                        false,
                    )
                }
                _ => return Err("unexpected mixed operation".into()),
            };
            let metadata = plan
                .metadata()
                .next()
                .ok_or("source mixed metadata absent")?;
            assert_eq!((metadata.priority, metadata.state_enter_seq), (0, sequence));
            assert_eq!(plan.consumed_ordinals(), u32::from(enters));
            assert_eq!(history.kind, kind);
            assert_eq!(write.record.payload(), payload);
            assert_eq!(
                write.record.next_run_at().map(TimestampMillis::get),
                Some(due)
            );
            let delta = plan.deltas().next().ok_or("source mixed delta absent")?;
            let old_due = previous
                .and_then(|(old, _)| old.next_run_at())
                .map(TimestampMillis::get);
            for (row, expected_due) in [(delta.before, old_due), (delta.after, Some(due))] {
                match (row, expected_due) {
                    (None, None) => {}
                    (Some(IndexRows::Parallel { due: row }), Some(expected_due)) => {
                        assert_eq!((row.workflow_type, row.partition_key), (TYPE, PARTITION));
                        assert_eq!(
                            (row.id, row.state, row.priority),
                            (command.id.as_bytes(), b"retry".as_slice(), 0)
                        );
                        assert_eq!(row.due.get(), expected_due);
                    }
                    _ => return Err("source mixed physical delta differs".into()),
                }
            }
            Ok(write.record.clone())
        },
    )
}

fn claim_mixed_due(
    rows: &mut Records,
    fifo: &mut FifoMembers,
    parallel: &mut ParallelMembers,
    policy: &PolicySnapshot,
) -> Result<(usize, Option<WorkflowRecord>), TestError> {
    let partitions = [PARTITION];
    let states = [b"fifo_gate".as_slice(), b"retry".as_slice()];
    let request = SelectionRequest {
        partitions: PartitionFilter::Only(&partitions),
        states: StateFilter::Only(&states),
        limit: 1,
        ..SelectionRequest::new(b"g", TYPE, TimestampMillis::new(10_000))
    };
    let (count, selected) = with_published(
        (TYPE, PARTITION),
        rows,
        fifo,
        parallel,
        policy,
        2,
        |_, view| {
            let selection = select(&view, request, None)?;
            let count = selection.records().len();
            if count != 1 {
                return Ok((count, None));
            }
            let selected = selection.records()[0];
            assert_eq!(selected.partition_key, PARTITION);
            assert_eq!(selected.action, SelectionAction::Claim);
            assert_eq!(selected.record.state(), b"retry");
            let fence = selected
                .record
                .fence()
                .checked_next()
                .ok_or("mixed claim fence overflow")?;
            Ok((count, Some((selected.record.id().clone(), fence))))
        },
    )?;
    let Some((id, fence)) = selected else {
        return Ok((count, None));
    };
    let token = LeaseToken::new(format!("fifo-worker:10000:{}", fence.get()).into_bytes())?;
    let command = CommittedCommand {
        id,
        now: TimestampMillis::new(10_000),
        operation: Operation::Claim(Claim {
            expected_state: b"retry".to_vec(),
            worker: b"fifo-worker".to_vec(),
            token,
            lease_ms: 30_000,
        }),
    };
    let record = publish_mixed_due(rows, fifo, parallel, policy, &command, 3)?;
    let lease = record.lease().ok_or("source mixed lease absent")?;
    let Operation::Claim(claim) = &command.operation else {
        return Err("source mixed claim operation absent".into());
    };
    assert_eq!(lease.worker, b"fifo-worker");
    assert_eq!(lease.token, claim.token);
    assert_eq!((record.fence(), lease.deadline.get()), (fence, 40_000));
    Ok((count, Some(record)))
}
