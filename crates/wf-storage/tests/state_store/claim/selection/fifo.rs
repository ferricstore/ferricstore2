//! Original default-budget FIFO cases, followed by isolated tiny-window controls.

use super::{TestResult, f};
use std::path::Path;
use wf_domain::history::EventKind;
use wf_domain::lease::{Lease, LeaseToken};
use wf_domain::workflow::indexed::OrderingMetadata;
use wf_domain::workflow::request::{RequestScope, RetainedStatus};
use wf_domain::workflow::restoration::WorkflowRecordData;
use wf_ports::state::{GroupApplyStore, GroupApplyTransaction, StoredWorkflowRecord};
use wf_types::identity::{PartitionId, RecordId};
use wf_types::version::{Fence, TimestampMillis};

fn deep_seed(path: &Path, s: &RequestScope) -> TestResult<StoredWorkflowRecord> {
    let mut store = f::create(
        path,
        s,
        &[b"p"],
        Some(f::policy(b"deep", &[b"queued"], false)?),
    )?;
    let owners = (0..81)
        .map(|i| {
            let id = if i == 0 {
                b"fifo-deep-head".to_vec()
            } else {
                format!("fifo-deep-follower-{i}").into_bytes()
            };
            f::owner(
                s,
                b"p",
                f::data(
                    &id,
                    b"deep",
                    b"queued",
                    500,
                    Some(if i == 0 { 2000 } else { 1000 }),
                )?,
                OrderingMetadata {
                    priority: 0,
                    state_enter_seq: i as u128,
                },
            )
        })
        .collect::<TestResult<Vec<_>>>()?;
    // ONE original create-many, with both original preclaim due point reads.
    f::seed(&mut store, s, &owners, 0, 500)?;
    {
        let tx = store.begin_apply(
            s.group(),
            s.recovery(),
            f::limits().base.codec.admission(),
            100,
            1_000_000,
            1_000_000,
        )?;
        for (id, due) in [
            (b"fifo-deep-head".as_slice(), 2000),
            (b"fifo-deep-follower-1".as_slice(), 1000),
        ] {
            assert_eq!(
                tx.record(
                    s.tenant(),
                    &PartitionId::new(b"p".to_vec())?,
                    &RecordId::new(id.to_vec())?
                )?
                .ok_or("source preclaim point missing")?
                .record
                .next_run_at(),
                Some(TimestampMillis::new(due))
            );
        }
    }
    drop(store);
    Ok(owners[0].clone())
}

fn deep_claim(
    path: &Path,
    s: &RequestScope,
    before: &StoredWorkflowRecord,
    budgets: (usize, usize),
) -> TestResult {
    let mut expected = f::cohort(path)?;
    let mut query = f::query(b"deep", b"fifo-worker", 1);
    query.states = Some(vec![b"queued".to_vec()]);
    query.partitions = Some(vec![b"p".to_vec()]);
    let frame = f::phase(s, b"deep", 2000, &query, budgets, [21; 32])?;
    let after = f::owner(
        s,
        b"p",
        WorkflowRecordData {
            state: b"running".to_vec(),
            version: 2,
            fence: Fence::new(1),
            updated_at: TimestampMillis::new(2000),
            next_run_at: Some(TimestampMillis::new(32000)),
            run_state: Some(b"queued".to_vec()),
            lease: Some(Lease {
                worker: b"fifo-worker".to_vec(),
                token: LeaseToken::new(b"fifo-worker:2000:1".to_vec())?,
                deadline: TimestampMillis::new(32000),
            }),
            latest_kind: EventKind::Claimed,
            latest_version: 2,
            latest_fence: Fence::new(1),
            ..f::data(b"fifo-deep-head", b"deep", b"queued", 500, Some(2000))?
        },
        before.ordering,
    )?;
    let response = f::receipt(std::slice::from_ref(&after), 0, 1, 1)?;
    f::replace_expected(
        &mut expected,
        s,
        &after,
        &f::history(&after),
        &[f::due(s, before)?, f::member(s, before)],
        &[
            (f::due(s, &after)?, vec![1]),
            (f::member(s, &after), Vec::new()),
        ],
    )?;
    let metadata = f::metadata(s, b"deep", 1, 2000, [21; 32], RetainedStatus::Accepted)?;
    f::finish(
        &mut expected,
        s,
        &frame,
        f::log(1),
        (f::clock(2000)?, f::clock(2000)?),
        Some((&metadata, &response)),
    )?;
    f::apply_check(
        path,
        s,
        &frame,
        f::log(1),
        f::limits(),
        &expected,
        &response,
    )?;
    assert_eq!(
        expected[1]
            .keys()
            .filter(|key| key.get(6) == Some(&4))
            .count(),
        81
    );
    Ok(())
}

#[test]
fn fifo_eighty_earlier_due_followers_do_not_hide_true_head() -> TestResult {
    // 74f5fcdcf4328003: original query first; B1 is a separate physical reset.
    let dir = f::directory("claim-fifo80")?;
    let s = f::scope()?;
    for (label, budgets) in [("original", (16384, 4096)), ("supplement-b1", (1, 1))] {
        let path = dir.join(label);
        let head = deep_seed(&path, &s)?;
        deep_claim(&path, &s, &head, budgets)?;
    }
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

fn any_seed(path: &Path, s: &RequestScope) -> TestResult<StoredWorkflowRecord> {
    let mut store = f::create(
        path,
        s,
        &[b"p"],
        Some(f::policy(b"deep-order", &[b"first", b"second"], false)?),
    )?;
    let mut first = Vec::new();
    for i in 0..81 {
        first.push(f::owner(
            s,
            b"p",
            f::data(
                format!("later-{i}").as_bytes(),
                b"deep-order",
                b"first",
                500,
                Some(if i == 0 { 2000 } else { 1000 }),
            )?,
            OrderingMetadata {
                priority: 0,
                state_enter_seq: i as u128,
            },
        )?);
    }
    f::seed(&mut store, s, &first, 0, 500)?;
    let earlier = f::owner(
        s,
        b"p",
        f::data(b"earlier", b"deep-order", b"second", 600, Some(1500))?,
        OrderingMetadata {
            priority: 0,
            state_enter_seq: 1_u128 << 32,
        },
    )?;
    f::seed(&mut store, s, std::slice::from_ref(&earlier), 1, 600)?;
    drop(store);
    Ok(earlier)
}

fn any_claim(
    path: &Path,
    s: &RequestScope,
    earlier: &StoredWorkflowRecord,
    budgets: (usize, usize),
) -> TestResult {
    let mut expected = f::cohort(path)?;
    let mut query = f::query(b"deep-order", b"fifo-worker", 1);
    query.partitions = Some(vec![b"p".to_vec()]);
    let frame = f::phase(s, b"any", 2000, &query, budgets, [22; 32])?;
    let after = f::owner(
        s,
        b"p",
        WorkflowRecordData {
            state: b"running".to_vec(),
            version: 2,
            fence: Fence::new(1),
            updated_at: TimestampMillis::new(2000),
            next_run_at: Some(TimestampMillis::new(32000)),
            run_state: Some(b"second".to_vec()),
            lease: Some(Lease {
                worker: b"fifo-worker".to_vec(),
                token: LeaseToken::new(b"fifo-worker:2000:1".to_vec())?,
                deadline: TimestampMillis::new(32000),
            }),
            latest_kind: EventKind::Claimed,
            latest_version: 2,
            latest_fence: Fence::new(1),
            ..f::data(b"earlier", b"deep-order", b"second", 600, Some(1500))?
        },
        earlier.ordering,
    )?;
    let response = f::receipt(std::slice::from_ref(&after), 1, 1, 2)?;
    f::replace_expected(
        &mut expected,
        s,
        &after,
        &f::history(&after),
        &[f::due(s, earlier)?, f::member(s, earlier)],
        &[
            (f::due(s, &after)?, vec![1]),
            (f::member(s, &after), Vec::new()),
        ],
    )?;
    let metadata = f::metadata(s, b"any", 2, 2000, [22; 32], RetainedStatus::Accepted)?;
    f::finish(
        &mut expected,
        s,
        &frame,
        f::log(2),
        (f::clock(2000)?, f::clock(2000)?),
        Some((&metadata, &response)),
    )?;
    f::apply_check(
        path,
        s,
        &frame,
        f::log(2),
        f::limits(),
        &expected,
        &response,
    )?;
    Ok(())
}

#[test]
fn fifo_any_states_selects_earlier_other_lane_head() -> TestResult {
    // 9f5d395534240b02: actual500 bulk and600 second-lane create; original ANY.
    let dir = f::directory("claim-fifo-any")?;
    let s = f::scope()?;
    for (label, budgets) in [("original", (16384, 4096)), ("supplement-b1", (1, 2))] {
        let path = dir.join(label);
        let earlier = any_seed(&path, &s)?;
        any_claim(&path, &s, &earlier, budgets)?;
    }
    std::fs::remove_dir_all(dir)?;
    Ok(())
}
