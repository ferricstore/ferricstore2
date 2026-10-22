//! Source FIFO head transition publishes a parallel row and releases its follower.

use super::fifo_indexes::{claim_one, publish, queued_policy};
use super::*;
use std::collections::BTreeSet;
use wf_domain::workflow::Transition;

const TYPE: &[u8] = b"flow-invariant-fifo-transition-unblock-type:2";
const PARTITION: &[u8] = b"tenant:fifo-transition-unblock:1";
const IDS: [&[u8]; 2] = [b"z-fifo-transition-first:1", b"a-fifo-transition-second:1"];

#[test]
fn fifo_transition_to_parallel_unblocks_source_lane() -> TestResult {
    // invariant_test.exs917–956, declaration4e48b0f1ca72d413; pure fixture only.
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
    let (first_count, first) = claim_one(
        scope,
        &mut rows,
        &mut fifo,
        &mut parallel,
        policy,
        (2_000, 3, 1),
        IDS[0],
    )?;
    assert_eq!(first_count, 1);
    let first = first.ok_or("source first claim returned no record")?;
    let transition = CommittedCommand {
        id: first.id().clone(),
        now: TimestampMillis::new(2_100),
        operation: Operation::Transition(Transition {
            from: b"running".to_vec(),
            to: b"waiting".to_vec(),
            fence: first.fence(),
            lease_token: Some(
                first
                    .lease()
                    .ok_or("source first lease absent")?
                    .token
                    .clone(),
            ),
            run_at: Some(TimestampMillis::new(5_000)),
            payload: None,
        }),
    };
    let transitioned = publish(
        scope,
        &mut rows,
        &mut fifo,
        &mut parallel,
        policy,
        &transition,
        4,
    );
    assert!(transitioned.is_ok(), "{transitioned:?}");
    transitioned?;
    let waiting = BTreeSet::from([(0, 5_000, IDS[0].to_vec(), b"waiting".to_vec())]);
    assert_eq!(parallel, waiting);
    assert_eq!(rows[IDS[0]].0.state(), b"waiting");
    assert!(rows[IDS[0]].0.lease().is_none());
    assert_eq!(rows[IDS[0]].1.state_enter_seq, 4_u128 << 32);
    let (second_count, second) = claim_one(
        scope,
        &mut rows,
        &mut fifo,
        &mut parallel,
        policy,
        (2_101, 5, 10),
        IDS[1],
    )?;
    assert_eq!(second_count, 1);
    let second = second.ok_or("source second claim returned no record")?;
    assert_eq!(second.id().as_bytes(), IDS[1]);
    assert_eq!(fifo, BTreeSet::from([(-1, 2_u128 << 32, IDS[1].to_vec())]));
    assert_eq!(parallel, waiting);
    Ok(())
}
