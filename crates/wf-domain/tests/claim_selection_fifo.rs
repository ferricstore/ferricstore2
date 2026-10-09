//! Pinned FIFO lifecycle selection scenarios against borrowed authoritative indexes.

#![forbid(unsafe_code)]

use wf_domain::claim::{
    AuthoritativeClaimView, Claim, ClaimRecord, FifoIndex, LaneKey, LaneOrder, ParallelIndex,
    RecordIndex, ScopedRecordId, SelectionAction, SelectionRequest, StateFilter, select,
};
use wf_domain::lease::{LeaseGuard, LeaseToken};
use wf_domain::retry::{Retry, RetryTime};
use wf_domain::workflow::{CommittedCommand, Create, Operation, Terminal, WorkflowRecord, reduce};
use wf_types::identity::RecordId;
use wf_types::version::TimestampMillis;

type TestResult = Result<(), Box<dyn std::error::Error>>;
type SelectedIds = Vec<(Vec<u8>, SelectionAction)>;

fn record(id: &[u8], state: &[u8], due: u64) -> Result<WorkflowRecord, Box<dyn std::error::Error>> {
    let command = CommittedCommand {
        id: RecordId::new(id.to_vec())?,
        now: TimestampMillis::new(1_000),
        operation: Operation::Create(Create {
            workflow_type: b"orders".to_vec(),
            state: state.to_vec(),
            run_at: Some(TimestampMillis::new(due)),
            payload: None,
        }),
    };
    Ok(reduce(None, &command)?
        .into_parts()
        .1
        .ok_or("create write absent")?
        .record)
}

fn mutate(
    record: &WorkflowRecord,
    now: u64,
    operation: Operation,
) -> Result<WorkflowRecord, Box<dyn std::error::Error>> {
    let command = CommittedCommand {
        id: record.id().clone(),
        now: TimestampMillis::new(now),
        operation,
    };
    Ok(reduce(Some(record), &command)?
        .into_parts()
        .1
        .ok_or("mutation write absent")?
        .record)
}

fn claim(
    record: &WorkflowRecord,
    now: u64,
    lease_ms: u64,
) -> Result<WorkflowRecord, Box<dyn std::error::Error>> {
    mutate(
        record,
        now,
        Operation::Claim(Claim {
            expected_state: record.state().to_vec(),
            worker: b"worker".to_vec(),
            token: LeaseToken::new(b"lease".to_vec())?,
            lease_ms,
        }),
    )
}

fn guard(record: &WorkflowRecord) -> Result<LeaseGuard, Box<dyn std::error::Error>> {
    Ok(LeaseGuard {
        token: record.lease().ok_or("lease absent")?.token.clone(),
        fence: record.fence(),
    })
}

fn add<'a>(
    index: &mut FifoIndex<'a>,
    record: &'a WorkflowRecord,
    partition: &'a [u8],
    sequence: u128,
) {
    let entry = ClaimRecord {
        record,
        partition_key: partition,
        priority: 0,
        state_enter_seq: sequence,
    };
    if let Some(key) = LaneKey::for_record(&entry) {
        assert_eq!(
            index
                .entry(key)
                .or_default()
                .insert(LaneOrder::for_record(&entry), entry),
            None
        );
    }
}

fn authority<'a>(parallel: &ParallelIndex<'a>, fifo: &FifoIndex<'a>) -> RecordIndex<'a> {
    let mut records = RecordIndex::new();
    for entry in parallel
        .values()
        .chain(fifo.values().flat_map(|lane| lane.values()))
    {
        records
            .entry(ScopedRecordId::for_record(entry))
            .or_insert(*entry);
    }
    records
}

fn selected<'a>(
    fifo: &'a FifoIndex<'a>,
    parallel: &'a ParallelIndex<'a>,
    now: u64,
    reclaim: bool,
) -> Result<SelectedIds, Box<dyn std::error::Error>> {
    let records = authority(parallel, fifo);
    let view = AuthoritativeClaimView {
        group: b"group",
        revision: 1,
        records: &records,
        parallel,
        fifo,
    };
    let mut request = SelectionRequest::new(b"group", b"orders", TimestampMillis::new(now));
    request.limit = 10;
    request.reclaim_expired = reclaim;
    Ok(select(&view, request, None)?
        .records()
        .iter()
        .map(|entry| (entry.record.id().as_bytes().to_vec(), entry.action))
        .collect())
}

#[test]
fn live_running_head_blocks_follower_and_completion_unblocks_lane() -> TestResult {
    // Source 3c9f0622ef767c44: invariant_test.exs:38-106, assertions68,77,79,96,105.
    // Reducer+rebuilt test indexes preserve the call sequence; durable indexes omitted.
    let first = record(b"z-first", b"queued", 2_000)?;
    let second = record(b"a-second", b"queued", 2_000)?;
    let parallel = ParallelIndex::new();
    let mut fifo = FifoIndex::new();
    add(&mut fifo, &first, b"tenant", 1);
    add(&mut fifo, &second, b"tenant", 2);
    assert_eq!(
        selected(&fifo, &parallel, 2_000, false)?,
        [(b"z-first".to_vec(), SelectionAction::Claim)]
    );

    let running = claim(&first, 2_000, 30_000)?;
    let mut running_fifo = FifoIndex::new();
    add(&mut running_fifo, &running, b"tenant", 1);
    add(&mut running_fifo, &second, b"tenant", 2);
    assert_eq!(
        selected(&running_fifo, &parallel, 2_001, true)?,
        SelectedIds::new()
    );

    let completed = mutate(
        &running,
        2_100,
        Operation::Complete(Terminal {
            guard: guard(&running)?,
            payload: None,
            value: Some(b"done".to_vec()),
        }),
    )?;
    let mut completed_fifo = FifoIndex::new();
    add(&mut completed_fifo, &completed, b"tenant", 1);
    add(&mut completed_fifo, &second, b"tenant", 2);
    assert_eq!(
        selected(&completed_fifo, &parallel, 2_101, true)?,
        [(b"a-second".to_vec(), SelectionAction::Claim)]
    );
    Ok(())
}

#[test]
fn retry_preserves_lane_blocking_until_due_and_terminal_exit() -> TestResult {
    // Source e339e69ff4c5a0fa: invariant_test.exs:108-194, assertions136,145,156,165,174,184,193.
    // Exact explicit run_at2000/2100 scheduling subset; policy install remains omitted.
    let first = record(b"z-first", b"queued", 2_000)?;
    let second = record(b"a-second", b"queued", 2_000)?;
    let running = claim(&first, 2_000, 30_000)?;
    let retried = mutate(
        &running,
        2_010,
        Operation::Retry(Retry {
            guard: guard(&running)?,
            run_at: RetryTime::Explicit(TimestampMillis::new(2_100)),
            payload: None,
            error: Some(b"try again".to_vec()),
        }),
    )?;
    let parallel = ParallelIndex::new();
    let mut fifo = FifoIndex::new();
    add(&mut fifo, &retried, b"tenant", 1);
    add(&mut fifo, &second, b"tenant", 2);
    assert_eq!(selected(&fifo, &parallel, 2_050, true)?, SelectedIds::new());
    assert_eq!(
        selected(&fifo, &parallel, 2_100, true)?,
        [(b"z-first".to_vec(), SelectionAction::Claim)]
    );
    let rerunning = claim(&retried, 2_100, 30_000)?;
    let completed = mutate(
        &rerunning,
        2_200,
        Operation::Complete(Terminal {
            guard: guard(&rerunning)?,
            payload: None,
            value: Some(b"done".to_vec()),
        }),
    )?;
    let mut completed_fifo = FifoIndex::new();
    add(&mut completed_fifo, &completed, b"tenant", 1);
    add(&mut completed_fifo, &second, b"tenant", 2);
    assert_eq!(
        selected(&completed_fifo, &parallel, 2_201, true)?,
        [(b"a-second".to_vec(), SelectionAction::Claim)]
    );
    Ok(())
}

#[test]
fn expired_running_head_is_reclaimed_before_due_follower_with_limit_one() -> TestResult {
    // Source 485560e32e405cf1: invariant_test.exs:466-508, assertions477,487,497,498,500.
    // Explicit Reclaim selection replaces automatic source reclaim scheduling here.
    let first = record(b"z-first", b"queued", 2_000)?;
    let second = record(b"a-second", b"queued", 2_000)?;
    let running = claim(&first, 2_000, 50)?;
    let parallel = ParallelIndex::new();
    let mut fifo = FifoIndex::new();
    add(&mut fifo, &running, b"tenant", 1);
    add(&mut fifo, &second, b"tenant", 2);
    let records = authority(&parallel, &fifo);
    let view = AuthoritativeClaimView {
        group: b"group",
        revision: 1,
        records: &records,
        parallel: &parallel,
        fifo: &fifo,
    };
    let mut request = SelectionRequest::new(b"group", b"orders", TimestampMillis::new(2_100));
    request.reclaim_expired = true;
    let selection = select(&view, request, None)?;
    assert_eq!(selection.records().len(), 1);
    assert_eq!(selection.records()[0].record.id().as_bytes(), b"z-first");
    assert_eq!(selection.records()[0].action, SelectionAction::Reclaim);
    let reclaimed = mutate(
        &running,
        2_100,
        Operation::Reclaim(Claim {
            expected_state: b"running".to_vec(),
            worker: b"new-worker".to_vec(),
            token: LeaseToken::new(b"new-lease".to_vec())?,
            lease_ms: 50,
        }),
    )?;
    assert_eq!(
        reclaimed.lease().ok_or("reclaim lease absent")?.worker,
        b"new-worker"
    );
    let mut next_fifo = FifoIndex::new();
    add(&mut next_fifo, &reclaimed, b"tenant", 1);
    add(&mut next_fifo, &second, b"tenant", 2);
    assert_eq!(
        selected(&next_fifo, &parallel, 2_101, true)?,
        SelectedIds::new()
    );
    Ok(())
}

#[test]
fn not_due_head_blocks_eighty_due_followers() -> TestResult {
    // Source ba618b494b159854: invariant_test.exs:510-544, assertion536-543.
    // Complete lane truth replaces deletion of a disposable state-index entry.
    let first = record(b"z-cold-head", b"queued", 1_000_000)?;
    let followers: Vec<_> = (0..80)
        .map(|index| record(format!("hot-{index}").as_bytes(), b"queued", 2_000))
        .collect::<Result<_, _>>()?;
    let parallel = ParallelIndex::new();
    let mut fifo = FifoIndex::new();
    add(&mut fifo, &first, b"tenant", 1);
    for (index, follower) in followers.iter().enumerate() {
        add(&mut fifo, follower, b"tenant", index as u128 + 2);
    }
    assert_eq!(selected(&fifo, &parallel, 2_000, true)?, SelectedIds::new());
    Ok(())
}

#[test]
fn fifo_state_and_partition_lanes_are_independent() -> TestResult {
    // Sources d66bd07c7dc9d94e + be6beed30e05ded4: invariant_test.exs:196-310.
    // Preserve both-state selection and per-partition head sets; no persisted claims.
    let validate = record(b"validate", b"validate", 1_000)?;
    let charge = record(b"charge", b"charge", 1_000)?;
    let queued_a = record(b"first-a", b"queued", 1_000)?;
    let queued_b = record(b"first-b", b"queued", 1_000)?;
    let follower_a = record(b"second-a", b"queued", 1_000)?;
    let parallel = ParallelIndex::new();
    let mut fifo = FifoIndex::new();
    add(&mut fifo, &validate, b"tenant", 1);
    add(&mut fifo, &charge, b"tenant", 2);
    add(&mut fifo, &queued_a, b"a", 3);
    add(&mut fifo, &queued_b, b"b", 4);
    add(&mut fifo, &follower_a, b"a", 5);
    let records = authority(&parallel, &fifo);
    let view = AuthoritativeClaimView {
        group: b"group",
        revision: 1,
        records: &records,
        parallel: &parallel,
        fifo: &fifo,
    };
    let mut request = SelectionRequest::new(b"group", b"orders", TimestampMillis::new(1_000));
    request.limit = 10;
    request.states = StateFilter::Only(&[b"validate"]);
    assert_eq!(
        select(&view, request, None)?.records()[0]
            .record
            .id()
            .as_bytes(),
        b"validate"
    );
    request.states = StateFilter::Only(&[b"charge"]);
    assert_eq!(
        select(&view, request, None)?.records()[0]
            .record
            .id()
            .as_bytes(),
        b"charge"
    );
    request.states = StateFilter::Only(&[b"queued"]);
    let queued = select(&view, request, None)?;
    let ids: Vec<_> = queued
        .records()
        .iter()
        .map(|entry| entry.record.id().as_bytes())
        .collect();
    assert_eq!(ids, [b"first-a".as_slice(), b"first-b".as_slice()]);
    Ok(())
}
