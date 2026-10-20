//! Preserved source operands through real creation and durable physical Claim.

use super::{TestResult, fixtures as f};

mod fifo;
use wf_domain::history::EventKind;
use wf_domain::lease::{Lease, LeaseToken};
use wf_domain::workflow::indexed::OrderingMetadata;
use wf_domain::workflow::request::{RequestScope, RetainedStatus};
use wf_domain::workflow::restoration::WorkflowRecordData;
use wf_ports::state::StoredWorkflowRecord;
use wf_types::identity::TenantId;
use wf_types::version::{Fence, TimestampMillis};

#[test]
fn priority_default_uses_two_durable_singleton_calls() -> TestResult {
    // 9bdbecf61fe86d5f: same due1000, worker/lease/limit, TWO actual claims.
    let dir = f::directory("claim-priority")?;
    let path = dir.join("state");
    let s = f::scope()?;
    let mut store = f::create(&path, &s, &[b"p"], None)?;
    let low = f::owner(
        &s,
        b"p",
        f::data(b"low", b"priority", b"queued", 500, Some(1000))?,
        OrderingMetadata {
            priority: 0,
            state_enter_seq: 0,
        },
    )?;
    let high = f::owner(
        &s,
        b"p",
        f::data(b"high", b"priority", b"queued", 500, Some(1000))?,
        OrderingMetadata {
            priority: 2,
            state_enter_seq: 1_u128 << 32,
        },
    )?;
    f::seed(&mut store, &s, std::slice::from_ref(&low), 0, 500)?;
    f::seed(&mut store, &s, std::slice::from_ref(&high), 1, 500)?;
    drop(store);
    let mut expected = f::cohort(&path)?;
    for (index, original, request, stop) in [
        (2, &high, b"high-call".as_slice(), 1),
        (3, &low, b"low-call".as_slice(), 0),
    ] {
        let query = f::query(b"priority", b"worker-priority", 1);
        let frame = f::phase(&s, request, 1000, &query, (16384, 4096), [11; 32])?;
        let after = priority_postimage(
            &s,
            original.record.id().as_bytes(),
            b"priority",
            original.ordering,
        )?;
        let response = f::receipt(std::slice::from_ref(&after), stop, 1, 0)?;
        f::replace_expected(
            &mut expected,
            &s,
            &after,
            &f::history(&after),
            &[f::due(&s, original)?],
            &[(f::due(&s, &after)?, vec![0])],
        )?;
        let metadata = f::metadata(&s, request, index, 1000, [11; 32], RetainedStatus::Accepted)?;
        f::finish(
            &mut expected,
            &s,
            &frame,
            f::log(index),
            (f::clock(1000)?, f::clock(1000)?),
            Some((&metadata, &response)),
        )?;
        let reply = f::apply_check(
            &path,
            &s,
            &frame,
            f::log(index),
            f::limits(),
            &expected,
            &response,
        )?;
        assert_eq!(
            (
                reply.original_position(),
                reply.original_time(),
                reply.expires_at()
            ),
            (
                f::log(index),
                TimestampMillis::new(1000),
                TimestampMillis::new(2000)
            )
        );
    }
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn explicit_priority_targets_actual_band() -> TestResult {
    // 7d7eaf003a23555b: explicit0 first, then2, each an actual singleton.
    let dir = f::directory("claim-band")?;
    let path = dir.join("state");
    let s = f::scope()?;
    let mut store = f::create(&path, &s, &[b"p"], None)?;
    let owners = [(b"low".as_slice(), 0), (b"high".as_slice(), 2)]
        .into_iter()
        .enumerate()
        .map(|(i, (id, priority))| {
            f::owner(
                &s,
                b"p",
                f::data(id, b"priority-target", b"queued", 500, Some(1000))?,
                OrderingMetadata {
                    priority,
                    state_enter_seq: (i as u128) << 32,
                },
            )
        })
        .collect::<TestResult<Vec<_>>>()?;
    for (i, row) in owners.iter().enumerate() {
        f::seed(
            &mut store,
            &s,
            std::slice::from_ref(row),
            u64::try_from(i)?,
            500,
        )?;
    }
    drop(store);
    let mut expected = f::cohort(&path)?;
    for (i, original) in owners.iter().enumerate() {
        let index = u64::try_from(i)? + 2;
        let request = [u8::try_from(index)?];
        let mut query = f::query(b"priority-target", b"worker-priority", 1);
        query.priority = Some(original.ordering.priority);
        let frame = f::phase(&s, &request, 1000, &query, (16384, 4096), [request[0]; 32])?;
        let after = priority_postimage(
            &s,
            original.record.id().as_bytes(),
            b"priority-target",
            original.ordering,
        )?;
        let response = f::receipt(std::slice::from_ref(&after), 0, 1, 0)?;
        f::replace_expected(
            &mut expected,
            &s,
            &after,
            &f::history(&after),
            &[f::due(&s, original)?],
            &[(f::due(&s, &after)?, vec![0])],
        )?;
        let metadata = f::metadata(
            &s,
            &request,
            index,
            1000,
            [request[0]; 32],
            RetainedStatus::Accepted,
        )?;
        f::finish(
            &mut expected,
            &s,
            &frame,
            f::log(index),
            (f::clock(1000)?, f::clock(1000)?),
            Some((&metadata, &response)),
        )?;
        assert_eq!(
            f::apply_check(
                &path,
                &s,
                &frame,
                f::log(index),
                f::limits(),
                &expected,
                &response
            )?
            .status(),
            RetainedStatus::Accepted
        );
    }
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn physical_running_exclusions_and_any_are_scoped() -> TestResult {
    let dir = f::directory("claim-physical")?;
    let path = dir.join("state");
    let s = f::scope()?;
    let mut store = f::create(&path, &s, &[b"p", b"q"], None)?;
    let seed = f::owner(
        &s,
        b"p",
        f::data(b"id", b"physical", b"waiting", 0, Some(10))?,
        OrderingMetadata {
            priority: 0,
            state_enter_seq: 0,
        },
    )?;
    f::seed(&mut store, &s, std::slice::from_ref(&seed), 0, 0)?;
    drop(store);
    let mut expected = f::cohort(&path)?;
    // A coherent restored running point supplies an expired lease and opaque binary worker.
    let running = physical_before(&s, seed.ordering)?;
    expected[1].insert(f::point(&s, &running), f::record_wire(&s, &running)?);
    expected[1].remove(&f::due(&s, &seed)?);
    expected[1].insert(f::due(&s, &running)?, vec![0]);
    let history = f::history_wire(&s, &running, &f::history(&running))?;
    expected[1].insert(history.0, history.1);
    f::replace_rows(&path, &expected[1])?;
    drop(f::open(&path, &s)?);
    let mut query = f::query(b"physical", &[0, 255, b':'], 1);
    query.states = Some(vec![b"running".to_vec()]);
    query.exclude_states = vec![b"waiting".to_vec()];
    query.partitions = Some(vec![b"p".to_vec()]);
    query.lease_ms = 50;
    let frame = f::phase(&s, b"running", 25, &query, (1, 1), [23; 32])?;
    let after = physical_after(&s, seed.ordering)?;
    let response = f::receipt(std::slice::from_ref(&after), 0, 1, 0)?;
    f::replace_expected(
        &mut expected,
        &s,
        &after,
        &f::history(&after),
        &[f::due(&s, &running)?],
        &[(f::due(&s, &after)?, vec![0])],
    )?;
    let metadata = f::metadata(&s, b"running", 1, 25, [23; 32], RetainedStatus::Accepted)?;
    f::finish(
        &mut expected,
        &s,
        &frame,
        f::log(1),
        (f::clock(25)?, f::clock(25)?),
        Some((&metadata, &response)),
    )?;
    f::apply_check(
        &path,
        &s,
        &frame,
        f::log(1),
        f::limits(),
        &expected,
        &response,
    )?;
    query.states = None;
    query.exclude_states = vec![b"running".to_vec()];
    let excluded = f::phase(&s, b"excluded", 75, &query, (1, 1), [24; 32])?;
    let empty = f::receipt(&[], 0, 0, 0)?;
    let metadata = f::metadata(&s, b"excluded", 2, 75, [24; 32], RetainedStatus::Accepted)?;
    f::finish(
        &mut expected,
        &s,
        &excluded,
        f::log(2),
        (f::clock(75)?, f::clock(75)?),
        Some((&metadata, &empty)),
    )?;
    f::apply_check(
        &path,
        &s,
        &excluded,
        f::log(2),
        f::limits(),
        &expected,
        &empty,
    )?;
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn fifo_live_future_unscheduled_and_empty_heads_block_followers() -> TestResult {
    let dir = f::directory("claim-blockers")?;
    let s = f::scope()?;
    for (mode, due) in [
        ("future", Some(1_000_000)),
        ("unscheduled", None),
        ("live", Some(5000)),
        ("empty", None),
    ] {
        let path = dir.join(mode);
        let mut store = f::create(
            &path,
            &s,
            &[b"p"],
            Some(f::policy(b"blocked", &[b"queued"], false)?),
        )?;
        let head = f::owner(
            &s,
            b"p",
            f::data(b"z-head", b"blocked", b"queued", 0, Some(1000))?,
            OrderingMetadata {
                priority: 0,
                state_enter_seq: 0,
            },
        )?;
        let follower = f::owner(
            &s,
            b"p",
            f::data(b"a-follower", b"blocked", b"queued", 0, Some(1))?,
            OrderingMetadata {
                priority: 0,
                state_enter_seq: 1,
            },
        )?;
        let index = if mode == "empty" {
            0
        } else {
            f::seed(&mut store, &s, &[head.clone(), follower], 0, 0)?;
            1
        };
        drop(store);
        let mut expected = f::cohort(&path)?;
        if mode != "empty" {
            let blocker = blocked_head(&s, mode, due, head.ordering)?;
            expected[1].remove(&f::due(&s, &head)?);
            expected[1].remove(&f::member(&s, &head));
            expected[1].insert(f::point(&s, &blocker), f::record_wire(&s, &blocker)?);
            expected[1].insert(f::member(&s, &blocker), Vec::new());
            if due.is_some() {
                expected[1].insert(f::due(&s, &blocker)?, vec![1]);
            }
            let history = f::history_wire(&s, &blocker, &f::history(&blocker))?;
            expected[1].insert(history.0, history.1);
            f::replace_rows(&path, &expected[1])?;
            drop(f::open(&path, &s)?);
        }
        let mut query = f::query(b"blocked", b"worker", 1);
        query.states = Some(vec![b"queued".to_vec()]);
        query.partitions = Some(vec![b"p".to_vec()]);
        let frame = f::phase(&s, b"blocked", 2000, &query, (1, 1), [25; 32])?;
        let response = f::receipt(&[], 0, 0, 1)?;
        let metadata = f::metadata(
            &s,
            b"blocked",
            index,
            2000,
            [25; 32],
            RetainedStatus::Accepted,
        )?;
        f::finish(
            &mut expected,
            &s,
            &frame,
            f::log(index),
            (f::clock(2000)?, f::clock(2000)?),
            Some((&metadata, &response)),
        )?;
        f::apply_check(
            &path,
            &s,
            &frame,
            f::log(index),
            f::limits(),
            &expected,
            &response,
        )?;
    }
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn same_id_binary_partitions_and_tenants_keep_distinct_leases() -> TestResult {
    let dir = f::directory("claim-isolation")?;
    let path = dir.join("state");
    let s = f::scope()?;
    let other = RequestScope::new(
        TenantId::new(vec![0, 255])?,
        s.group().clone(),
        s.recovery().clone(),
    );
    let partitions: [&[u8]; 2] = [b"p\0", b"p\xff"];
    let mut store = f::create(&path, &s, &partitions, None)?;
    let owners = partitions
        .iter()
        .enumerate()
        .map(|(i, partition)| {
            f::owner(
                &s,
                partition,
                f::data(b"same", b"isolated", b"queued", 0, Some(0))?,
                OrderingMetadata {
                    priority: 0,
                    state_enter_seq: (i as u128) << 32,
                },
            )
        })
        .collect::<TestResult<Vec<_>>>()?;
    for (index, owner) in owners.iter().enumerate() {
        f::seed(
            &mut store,
            &s,
            std::slice::from_ref(owner),
            u64::try_from(index)?,
            0,
        )?;
    }
    let foreign = f::owner(
        &other,
        partitions[0],
        f::data(b"same", b"isolated", b"queued", 0, Some(0))?,
        OrderingMetadata {
            priority: 0,
            state_enter_seq: 2_u128 << 32,
        },
    )?;
    f::seed(&mut store, &other, std::slice::from_ref(&foreign), 2, 0)?;
    drop(store);
    let mut expected = f::cohort(&path)?;
    let mut after = Vec::new();
    for (i, partition) in partitions.iter().enumerate() {
        after.push(isolated_postimage(&s, partition, owners[i].ordering)?);
        f::replace_expected(
            &mut expected,
            &s,
            &after[i],
            &f::history(&after[i]),
            &[f::due(&s, &owners[i])?],
            &[(f::due(&s, &after[i])?, vec![0])],
        )?;
    }
    let frame = f::phase(
        &s,
        b"scope",
        25,
        &f::query(b"isolated", b"worker", 2),
        (2, 1),
        [26; 32],
    )?;
    let response = f::receipt(&after, 0, 2, 0)?;
    let metadata = f::metadata(&s, b"scope", 3, 25, [26; 32], RetainedStatus::Accepted)?;
    f::finish(
        &mut expected,
        &s,
        &frame,
        f::log(3),
        (f::clock(25)?, f::clock(25)?),
        Some((&metadata, &response)),
    )?;
    f::apply_check(
        &path,
        &s,
        &frame,
        f::log(3),
        f::limits(),
        &expected,
        &response,
    )?;
    assert_eq!(
        expected[1].get(&f::point(&other, &foreign)),
        Some(&f::record_wire(&other, &foreign)?)
    );
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

fn priority_postimage(
    s: &RequestScope,
    id: &[u8],
    kind: &[u8],
    ordering: OrderingMetadata,
) -> TestResult<StoredWorkflowRecord> {
    f::owner(
        s,
        b"p",
        WorkflowRecordData {
            state: b"running".to_vec(),
            version: 2,
            fence: Fence::new(1),
            updated_at: TimestampMillis::new(1000),
            next_run_at: Some(TimestampMillis::new(31000)),
            run_state: Some(b"queued".to_vec()),
            lease: Some(Lease {
                worker: b"worker-priority".to_vec(),
                token: LeaseToken::new(b"worker-priority:1000:1".to_vec())?,
                deadline: TimestampMillis::new(31000),
            }),
            latest_kind: EventKind::Claimed,
            latest_version: 2,
            latest_fence: Fence::new(1),
            ..f::data(id, kind, b"queued", 500, Some(1000))?
        },
        ordering,
    )
}

fn physical_before(
    s: &RequestScope,
    ordering: OrderingMetadata,
) -> TestResult<StoredWorkflowRecord> {
    f::owner(
        s,
        b"p",
        WorkflowRecordData {
            state: b"running".to_vec(),
            version: 2,
            fence: Fence::new(1),
            run_state: Some(b"waiting".to_vec()),
            lease: Some(Lease {
                worker: b"old".to_vec(),
                token: LeaseToken::new(b"old".to_vec())?,
                deadline: TimestampMillis::new(10),
            }),
            latest_kind: EventKind::Claimed,
            latest_version: 2,
            latest_fence: Fence::new(1),
            ..f::data(b"id", b"physical", b"waiting", 0, Some(10))?
        },
        ordering,
    )
}

fn physical_after(
    s: &RequestScope,
    ordering: OrderingMetadata,
) -> TestResult<StoredWorkflowRecord> {
    f::owner(
        s,
        b"p",
        WorkflowRecordData {
            state: b"running".to_vec(),
            version: 3,
            fence: Fence::new(2),
            updated_at: TimestampMillis::new(25),
            run_state: Some(b"waiting".to_vec()),
            next_run_at: Some(TimestampMillis::new(75)),
            lease: Some(Lease {
                worker: vec![0, 255, b':'],
                token: LeaseToken::new(vec![0, 255, b':', b':', b'2', b'5', b':', b'2'])?,
                deadline: TimestampMillis::new(75),
            }),
            latest_kind: EventKind::Claimed,
            latest_version: 3,
            latest_fence: Fence::new(2),
            ..f::data(b"id", b"physical", b"waiting", 0, Some(10))?
        },
        ordering,
    )
}

fn blocked_head(
    s: &RequestScope,
    mode: &str,
    due: Option<u64>,
    ordering: OrderingMetadata,
) -> TestResult<StoredWorkflowRecord> {
    let mut input = f::data(b"z-head", b"blocked", b"queued", 0, due)?;
    if mode == "live" {
        input.state = b"running".to_vec();
        input.run_state = Some(b"queued".to_vec());
        input.version = 2;
        input.fence = Fence::new(1);
        input.latest_kind = EventKind::Claimed;
        input.latest_version = 2;
        input.latest_fence = Fence::new(1);
        input.lease = Some(Lease {
            worker: b"old".to_vec(),
            token: LeaseToken::new(b"old".to_vec())?,
            deadline: TimestampMillis::new(5000),
        });
    }
    f::owner(s, b"p", input, ordering)
}

fn isolated_postimage(
    s: &RequestScope,
    partition: &[u8],
    ordering: OrderingMetadata,
) -> TestResult<StoredWorkflowRecord> {
    f::owner(
        s,
        partition,
        WorkflowRecordData {
            state: b"running".to_vec(),
            version: 2,
            fence: Fence::new(1),
            updated_at: TimestampMillis::new(25),
            next_run_at: Some(TimestampMillis::new(30025)),
            run_state: Some(b"queued".to_vec()),
            lease: Some(Lease {
                worker: b"worker".to_vec(),
                token: LeaseToken::new(b"worker:25:1".to_vec())?,
                deadline: TimestampMillis::new(30025),
            }),
            latest_kind: EventKind::Claimed,
            latest_version: 2,
            latest_fence: Fence::new(1),
            ..f::data(b"same", b"isolated", b"queued", 0, Some(0))?
        },
        ordering,
    )
}
