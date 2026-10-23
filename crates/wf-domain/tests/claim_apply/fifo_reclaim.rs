//! Reclaim publishes the expired source FIFO head without releasing its follower.

use super::fifo_indexes::{claim_requested, publish, queued_policy};
use super::*;
use std::collections::BTreeSet;
use wf_domain::claim::{SelectionRequest, StateFilter};

const TYPE: &[u8] = b"flow-invariant-fifo-reclaim-type:2";
const PARTITION: &[u8] = b"tenant:fifo-reclaim:1";
const IDS: [&[u8]; 2] = [b"z-fifo-reclaim-first:1", b"a-fifo-reclaim-second:1"];
const PARTITIONS: [&[u8]; 1] = [PARTITION];
const STATES: [&[u8]; 1] = [b"queued"];

fn source_reclaim_step(
    index: u64,
    now: u64,
    limit: usize,
    worker: &[u8],
    lease_ms: u64,
    expected_physical_state: &[u8],
) -> Result<(u64, SelectionRequest<'static>, Claim), TestError> {
    source_scoped_reclaim_step(
        (TYPE, &PARTITIONS),
        index,
        now,
        limit,
        worker,
        lease_ms,
        expected_physical_state,
    )
}

fn source_scoped_reclaim_step<'scope>(
    scope: (&'scope [u8], &'scope [&'scope [u8]]),
    index: u64,
    now: u64,
    limit: usize,
    worker: &[u8],
    lease_ms: u64,
    expected_physical_state: &[u8],
) -> Result<(u64, SelectionRequest<'scope>, Claim), TestError> {
    let request = SelectionRequest {
        partitions: PartitionFilter::Only(scope.1),
        states: StateFilter::Only(&STATES),
        limit,
        reclaim_expired: true,
        ..SelectionRequest::new(b"g", scope.0, TimestampMillis::new(now))
    };
    let claim = Claim {
        expected_state: expected_physical_state.to_vec(),
        worker: worker.to_vec(),
        token: LeaseToken::new(format!("source-fifo-reclaim:{index}:{now}").into_bytes())?,
        lease_ms,
    };
    Ok((index, request, claim))
}

#[test]
fn fifo_reclaims_expired_head_before_queued_follower() -> TestResult {
    // invariant_test.exs413–464, declaration3483f6cec1191aa4; pure source-derived fixture.
    let installed = queued_policy(TYPE)?;
    let policy = installed.policy();
    let scope = (TYPE, PARTITION);
    let mut rows = BTreeMap::new();
    let mut fifo = BTreeSet::new();
    let mut parallel = BTreeSet::new();
    for (index, id, now) in [(1, IDS[0], 1_000), (2, IDS[1], 1_001)] {
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
        publish(
            scope,
            &mut rows,
            &mut fifo,
            &mut parallel,
            policy,
            &command,
            index,
        )?;
    }
    let mut expected = BTreeSet::from([
        (0, 1_u128 << 32, IDS[0].to_vec()),
        (0, 2_u128 << 32, IDS[1].to_vec()),
    ]);
    assert_eq!(fifo, expected);
    assert!(parallel.is_empty());
    let queued = b"queued".as_slice();
    let running = b"running".as_slice();
    let old_worker = b"old-worker".as_slice();
    let new_worker = b"new-worker".as_slice();
    let fifo_worker = b"fifo-worker".as_slice();
    let steps = [
        (3, 2_000, 1, old_worker, 50, queued),
        (4, 2_010, 10, fifo_worker, 30_000, queued),
        (4, 2_100, 10, new_worker, 50, running),
        (5, 2_101, 10, fifo_worker, 30_000, queued),
    ];
    let mut results = Vec::new();
    for (position, (index, now, limit, worker, lease_ms, state)) in steps.into_iter().enumerate() {
        let step = source_reclaim_step(index, now, limit, worker, lease_ms, state)?;
        let result = claim_requested(
            scope,
            &mut rows,
            &mut fifo,
            &mut parallel,
            policy,
            step,
            IDS[0],
        )?;
        results.push(result);
        if position == 0 {
            assert!(expected.remove(&(0, 1_u128 << 32, IDS[0].to_vec())));
            assert!(expected.insert((-1, 1_u128 << 32, IDS[0].to_vec())));
        }
        assert_eq!(fifo, expected);
        assert!(parallel.is_empty());
        assert_eq!(
            (rows[IDS[1]].0.state(), rows[IDS[1]].0.version()),
            (queued, 1)
        );
    }
    let first_id = results[0].1.as_ref().map(|record| record.id().as_bytes());
    assert_eq!((results[0].0, first_id), (1, Some(IDS[0])));
    let first = results[0]
        .1
        .as_ref()
        .ok_or("initial source claim has no record")?;
    let first_lease = first.lease().ok_or("initial source lease absent")?;
    assert_eq!((first.version(), first.fence().get()), (2, 1));
    assert_eq!(first_lease.deadline.get(), 2_050);
    assert_eq!(results[1].0, 0);
    assert_eq!(results[2].0, 1);
    let reclaimed = results[2]
        .1
        .as_ref()
        .ok_or("source reclaim has no record")?;
    let lease = reclaimed.lease().ok_or("source reclaim lease absent")?;
    assert_eq!(reclaimed.id().as_bytes(), IDS[0]);
    assert_eq!(lease.worker, b"new-worker");
    assert_eq!(results[3].0, 0);
    assert_eq!((reclaimed.version(), reclaimed.fence().get()), (3, 2));
    assert_eq!(lease.deadline.get(), 2_150);
    assert_eq!(reclaimed.run_state(), Some(b"queued".as_slice()));
    assert_eq!(rows[IDS[0]].1.state_enter_seq, 1_u128 << 32);
    assert_eq!(rows[IDS[1]].1.state_enter_seq, 2_u128 << 32);
    Ok(())
}

#[test]
fn fifo_limit_one_reclaims_head_before_queued_follower() -> TestResult {
    // invariant_test.exs466–508, declaration485560e32e405cf1; pure source-derived fixture.
    let workflow_type = b"flow-invariant-fifo-limit-one-reclaim-type:2".as_slice();
    let partition = b"tenant:fifo-limit-one-reclaim:1".as_slice();
    let ids = [
        b"z-fifo-limit-one-reclaim-first:1".as_slice(),
        b"a-fifo-limit-one-reclaim-second:1".as_slice(),
    ];
    let partitions = [partition];
    let installed = queued_policy(workflow_type)?;
    let policy = installed.policy();
    let scope = (workflow_type, partition);
    let mut rows = BTreeMap::new();
    let mut fifo = BTreeSet::new();
    let mut parallel = BTreeSet::new();
    for (index, id, now) in [(1, ids[0], 1_000), (2, ids[1], 1_001)] {
        let command = CommittedCommand {
            id: RecordId::new(id.to_vec())?,
            now: TimestampMillis::new(now),
            operation: Operation::Create(Create {
                workflow_type: workflow_type.to_vec(),
                state: b"queued".to_vec(),
                run_at: Some(TimestampMillis::new(2_000)),
                payload: Some(id.to_vec()),
            }),
        };
        publish(
            scope,
            &mut rows,
            &mut fifo,
            &mut parallel,
            policy,
            &command,
            index,
        )?;
    }
    let mut expected = BTreeSet::from([
        (0, 1_u128 << 32, ids[0].to_vec()),
        (0, 2_u128 << 32, ids[1].to_vec()),
    ]);
    assert_eq!(fifo, expected);
    assert!(parallel.is_empty());
    let steps = [
        (3, 2_000, "old-worker", 50, "queued"),
        (4, 2_100, "new-worker", 50, "running"),
        (5, 2_101, "follower-worker", 30_000, "queued"),
    ];
    let mut results = Vec::new();
    for (position, (index, now, worker, lease_ms, state)) in steps.into_iter().enumerate() {
        let step = source_scoped_reclaim_step(
            (workflow_type, &partitions),
            index,
            now,
            1,
            worker.as_bytes(),
            lease_ms,
            state.as_bytes(),
        )?;
        results.push(claim_requested(
            scope,
            &mut rows,
            &mut fifo,
            &mut parallel,
            policy,
            step,
            ids[0],
        )?);
        if position == 0 {
            assert!(expected.remove(&(0, 1_u128 << 32, ids[0].to_vec())));
            assert!(expected.insert((-1, 1_u128 << 32, ids[0].to_vec())));
        }
        assert_eq!(fifo, expected);
        assert!(parallel.is_empty());
        assert_eq!(
            (rows[ids[1]].0.state(), rows[ids[1]].0.version()),
            (b"queued".as_slice(), 1)
        );
    }
    let first_id = results[0].1.as_ref().map(|record| record.id().as_bytes());
    assert_eq!((results[0].0, first_id), (1, Some(ids[0])));
    let first = results[0].1.as_ref().ok_or("first absent")?;
    let first_lease = first.lease().ok_or("initial source lease absent")?;
    assert_eq!((first.version(), first.fence().get()), (2, 1));
    assert_eq!(first_lease.deadline.get(), 2_050);
    assert_eq!(results[1].0, 1);
    let reclaimed = results[1].1.as_ref().ok_or("reclaim absent")?;
    let lease = reclaimed.lease().ok_or("source reclaim lease absent")?;
    assert_eq!(reclaimed.id().as_bytes(), ids[0]);
    assert_eq!(lease.worker, b"new-worker");
    assert_eq!(results[2].0, 0);
    assert_eq!((reclaimed.version(), reclaimed.fence().get()), (3, 2));
    assert_eq!(lease.deadline.get(), 2_150);
    assert_eq!(reclaimed.run_state(), Some(b"queued".as_slice()));
    assert_eq!(rows[ids[0]].1.state_enter_seq, 1_u128 << 32);
    assert_eq!(rows[ids[1]].1.state_enter_seq, 2_u128 << 32);
    Ok(())
}
