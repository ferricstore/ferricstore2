//! Source-derived FIFO middle cancellation; saved rows advance only by plan deltas.

use super::*;
use std::collections::BTreeSet;
use wf_domain::workflow::Cancel;
use wf_domain::workflow::indexed::OrderingMetadata;
use wf_types::version::Fence;

const TYPE: &[u8] = b"flow-invariant-fifo-middle-cancel-type:2";
const PARTITION: &[u8] = b"tenant:fifo-middle-cancel:1";
const IDS: [&[u8]; 3] = [
    b"fifo-middle-cancel-first:1",
    b"fifo-middle-cancel-second:1",
    b"fifo-middle-cancel-third:1",
];

fn source_fifo_policy() -> Result<PolicyCandidate, TestError> {
    super::fifo_indexes::queued_policy(TYPE)
}

fn publish_fifo_command(
    rows: &mut BTreeMap<Vec<u8>, (WorkflowRecord, OrderingMetadata)>,
    members: &mut BTreeSet<(i8, u128, Vec<u8>)>,
    policy: &PolicySnapshot,
    command: &CommittedCommand,
    index: u64,
) -> TestResult {
    let mut parallel = super::fifo_indexes::ParallelMembers::new();
    let result = super::fifo_indexes::publish(
        (TYPE, PARTITION),
        rows,
        members,
        &mut parallel,
        policy,
        command,
        index,
    );
    assert!(
        parallel.is_empty(),
        "middle fixture gained parallel membership"
    );
    result
}

fn claim_fifo_one(
    rows: &mut BTreeMap<Vec<u8>, (WorkflowRecord, OrderingMetadata)>,
    members: &mut BTreeSet<(i8, u128, Vec<u8>)>,
    policy: &PolicySnapshot,
    now: u64,
    index: u64,
    expected_id: &[u8],
) -> Result<(usize, Option<WorkflowRecord>), TestError> {
    let mut parallel = super::fifo_indexes::ParallelMembers::new();
    let result = super::fifo_indexes::claim_one(
        (TYPE, PARTITION),
        rows,
        members,
        &mut parallel,
        policy,
        (now, index, 1),
        expected_id,
    );
    assert!(
        parallel.is_empty(),
        "middle fixture gained parallel membership"
    );
    result
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
