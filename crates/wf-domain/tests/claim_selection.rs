//! Source-derived authoritative selection scenarios; no storage success is implied.

#![forbid(unsafe_code)]

use std::collections::BTreeMap;

use wf_domain::claim::{
    AuthoritativeClaimView, ClaimRecord, DueOrder, FifoIndex, LaneKey, LaneOrder, ParallelIndex,
    PartitionFilter, RecordIndex, ScopedRecordId, SelectionRequest, SelectionStop, StateFilter,
    select,
};
use wf_domain::workflow::{CommittedCommand, Create, Operation, WorkflowRecord, reduce};
use wf_types::identity::RecordId;
use wf_types::version::TimestampMillis;

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn record(id: &[u8], state: &[u8], due: u64) -> Result<WorkflowRecord, Box<dyn std::error::Error>> {
    let command = CommittedCommand {
        id: RecordId::new(id.to_vec())?,
        now: TimestampMillis::new(500),
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

fn entry<'a>(
    record: &'a WorkflowRecord,
    partition: &'a [u8],
    seq: u128,
    priority: u8,
) -> ClaimRecord<'a> {
    ClaimRecord {
        record,
        partition_key: partition,
        state_enter_seq: seq,
        priority,
    }
}

fn add_parallel<'a>(index: &mut ParallelIndex<'a>, record: ClaimRecord<'a>) -> TestResult {
    let order = DueOrder::for_record(&record).ok_or("record has no active due order")?;
    assert_eq!(index.insert(order, record), None);
    Ok(())
}

fn add_fifo<'a>(index: &mut FifoIndex<'a>, record: ClaimRecord<'a>) -> TestResult {
    let lane = LaneKey::for_record(&record).ok_or("record has no active lane")?;
    assert_eq!(
        index
            .entry(lane)
            .or_default()
            .insert(LaneOrder::for_record(&record), record),
        None
    );
    Ok(())
}

fn request(now: u64) -> SelectionRequest<'static> {
    let mut request = SelectionRequest::new(b"group", b"orders", TimestampMillis::new(now));
    request.limit = 10;
    request
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

fn view<'a>(
    parallel: &'a ParallelIndex<'a>,
    fifo: &'a FifoIndex<'a>,
    records: &'a RecordIndex<'a>,
) -> AuthoritativeClaimView<'a> {
    AuthoritativeClaimView {
        group: b"group",
        revision: 1,
        records,
        parallel,
        fifo,
    }
}

#[test]
fn higher_priorities_drain_before_lower_priorities_by_default() -> TestResult {
    // Source 9bdbecf61fe86d5f: priority section:8-45, assertions34,44.
    // Select high then low via same-snapshot continuation; persistence omitted.
    let low = record(b"low", b"queued", 1_000)?;
    let high = record(b"high", b"queued", 1_000)?;
    let mut parallel = ParallelIndex::new();
    add_parallel(&mut parallel, entry(&low, b"tenant", 1, 0))?;
    add_parallel(&mut parallel, entry(&high, b"tenant", 2, 2))?;
    let fifo = FifoIndex::new();
    let records = authority(&parallel, &fifo);
    let authoritative = view(&parallel, &fifo, &records);
    let mut request = request(1_000);
    request.limit = 1;
    let first = select(&authoritative, request, None)?;
    assert_eq!(first.records()[0].record.id().as_bytes(), b"high");
    assert_eq!(first.stop(), SelectionStop::ClaimLimit);
    let second = select(&authoritative, request, first.continuation())?;
    assert_eq!(second.records()[0].record.id().as_bytes(), b"low");
    assert_eq!(second.stop(), SelectionStop::Complete);
    Ok(())
}

#[test]
fn priority_option_targets_one_priority_band() -> TestResult {
    // Source 7d7eaf003a23555b: priority section:47-86, assertions74,85.
    let low = record(b"low", b"queued", 1_000)?;
    let high = record(b"high", b"queued", 1_000)?;
    let mut parallel = ParallelIndex::new();
    add_parallel(&mut parallel, entry(&low, b"tenant", 1, 0))?;
    add_parallel(&mut parallel, entry(&high, b"tenant", 2, 2))?;
    let fifo = FifoIndex::new();
    let records = authority(&parallel, &fifo);
    let authoritative = view(&parallel, &fifo, &records);
    let mut request = request(1_000);
    request.priority = Some(0);
    let low_selection = select(&authoritative, request, None)?;
    assert_eq!(low_selection.records().len(), 1);
    assert_eq!(low_selection.records()[0].record.id().as_bytes(), b"low");
    request.priority = Some(2);
    assert_eq!(
        select(&authoritative, request, None)?.records()[0]
            .record
            .id()
            .as_bytes(),
        b"high"
    );
    Ok(())
}

#[test]
fn multi_state_claims_earliest_due_instead_of_first_listed_state() -> TestResult {
    const STATES: &[&[u8]] = &[b"queued", b"retry"];
    // Source ed190d79e3813b5b: multi-state section:8-44, assertion32-41.
    // This helper preserves selected ID/partition/state, not compact lease fields.
    let queued = record(b"queued", b"queued", 100)?;
    let retry = record(b"retry", b"retry", 10)?;
    let mut parallel = ParallelIndex::new();
    add_parallel(&mut parallel, entry(&queued, b"tenant", 1, 0))?;
    add_parallel(&mut parallel, entry(&retry, b"tenant", 2, 0))?;
    let fifo = FifoIndex::new();
    let records = authority(&parallel, &fifo);
    let mut request = request(101);
    request.states = StateFilter::Only(STATES);
    request.limit = 1;
    let selection = select(&view(&parallel, &fifo, &records), request, None)?;
    assert_eq!(selection.records()[0].record.id().as_bytes(), b"retry");
    assert_eq!(selection.records()[0].partition_key, b"tenant");
    assert_eq!(selection.records()[0].record.state(), b"retry");
    Ok(())
}

#[test]
fn fifo_selects_one_head_per_lane_in_entry_order() -> TestResult {
    // Source 3c9f0622ef767c44: invariant_test.exs:38-106, assertions68,77.
    // Frozen initial selection assertion only; completion/reselection is separate.
    let first = record(b"z-first", b"queued", 2_000)?;
    let second = record(b"a-second", b"queued", 2_000)?;
    let mut fifo = FifoIndex::new();
    add_fifo(&mut fifo, entry(&first, b"tenant", 1, 0))?;
    add_fifo(&mut fifo, entry(&second, b"tenant", 2, 0))?;
    let parallel = ParallelIndex::new();
    let records = authority(&parallel, &fifo);
    let selection = select(&view(&parallel, &fifo, &records), request(2_000), None)?;
    assert_eq!(selection.records().len(), 1);
    assert_eq!(selection.records()[0].record.id().as_bytes(), b"z-first");
    Ok(())
}

#[test]
fn fifo_partition_list_selects_only_one_head_per_listed_lane() -> TestResult {
    // Source 2c1a0a952f9bef71: invariant_test.exs:312-366, assertion345.
    let records = [
        record(b"first-a", b"queued", 2_000)?,
        record(b"second-a", b"queued", 2_000)?,
        record(b"first-b", b"queued", 2_000)?,
        record(b"second-b", b"queued", 2_000)?,
        record(b"ignored-c", b"queued", 2_000)?,
    ];
    let mut fifo = FifoIndex::new();
    for (index, record) in records.iter().enumerate() {
        let partition = match index {
            0 | 1 => b"a".as_slice(),
            2 | 3 => b"b".as_slice(),
            _ => b"c".as_slice(),
        };
        add_fifo(&mut fifo, entry(record, partition, index as u128, 0))?;
    }
    let parallel = ParallelIndex::new();
    let records = authority(&parallel, &fifo);
    let mut request = request(2_000);
    request.partitions = PartitionFilter::Only(&[b"a", b"b"]);
    let selection = select(&view(&parallel, &fifo, &records), request, None)?;
    let ids: Vec<_> = selection
        .records()
        .iter()
        .map(|selected| selected.record.id().as_bytes())
        .collect();
    assert_eq!(ids, [b"first-a".as_slice(), b"first-b".as_slice()]);
    Ok(())
}

#[test]
fn fifo_deep_head_is_selected_with_eighty_earlier_due_followers() -> TestResult {
    // Source 74f5fcdcf4328003: invariant_test.exs:546-582, assertions568-580.
    let head = record(b"deep-head", b"queued", 2_000)?;
    let followers: Vec<_> = (1..=80)
        .map(|index| record(format!("follower-{index}").as_bytes(), b"queued", 1_000))
        .collect::<Result<_, _>>()?;
    let mut fifo = FifoIndex::new();
    add_fifo(&mut fifo, entry(&head, b"tenant", 0, 0))?;
    for (index, follower) in followers.iter().enumerate() {
        add_fifo(&mut fifo, entry(follower, b"tenant", index as u128 + 1, 0))?;
    }
    let parallel = ParallelIndex::new();
    let records = authority(&parallel, &fifo);
    let mut request = request(2_000);
    request.limit = 1;
    request.scan_budget = 1;
    let selection = select(&view(&parallel, &fifo, &records), request, None)?;
    assert_eq!(head.next_run_at(), Some(TimestampMillis::new(2_000)));
    assert_eq!(
        followers[0].next_run_at(),
        Some(TimestampMillis::new(1_000))
    );
    assert_eq!(selection.records().len(), 1);
    assert_eq!(selection.records()[0].record.id().as_bytes(), b"deep-head");
    assert_eq!(selection.scanned(), 1);
    assert_eq!(selection.head_reads(), 1);
    Ok(())
}

#[test]
fn fifo_global_due_order_survives_deep_head_lookup() -> TestResult {
    // Source 9f5d395534240b02: invariant_test.exs:584-624, assertion616-622.
    let later = record(b"later-head", b"first", 2_000)?;
    let earlier = record(b"earlier-head", b"second", 1_500)?;
    let followers: Vec<_> = (1..=80)
        .map(|index| record(format!("follower-{index}").as_bytes(), b"first", 1_000))
        .collect::<Result<_, _>>()?;
    let mut fifo = FifoIndex::new();
    add_fifo(&mut fifo, entry(&later, b"tenant", 0, 0))?;
    add_fifo(&mut fifo, entry(&earlier, b"tenant", 81, 0))?;
    for (index, follower) in followers.iter().enumerate() {
        add_fifo(&mut fifo, entry(follower, b"tenant", index as u128 + 1, 0))?;
    }
    let parallel = ParallelIndex::new();
    let records = authority(&parallel, &fifo);
    let mut request = request(2_000);
    request.limit = 1;
    request.scan_budget = 1;
    let selection = select(&view(&parallel, &fifo, &records), request, None)?;
    assert_eq!(
        selection.records()[0].record.id().as_bytes(),
        b"earlier-head"
    );
    assert_eq!(selection.head_reads(), 2);
    Ok(())
}

#[test]
fn mixed_fifo_policy_preserves_due_order_for_parallel_states() -> TestResult {
    // Source bea9ae0f6f1e970c: invariant_test.exs:1102-1147, assertions1137,1146.
    // An empty authoritative FIFO gate and parallel retry rows preserve the source
    // selection scenario; policy installation and payload return remain omitted.
    const STATES: &[&[u8]] = &[b"fifo_gate", b"retry"];
    let later = record(b"parallel-later-due", b"retry", 10_000)?;
    let earlier = record(b"parallel-earlier-due", b"retry", 2_000)?;
    let mut parallel = ParallelIndex::new();
    add_parallel(&mut parallel, entry(&later, b"tenant", 1, 0))?;
    add_parallel(&mut parallel, entry(&earlier, b"tenant", 2, 0))?;
    let mut fifo = FifoIndex::new();
    fifo.insert(
        LaneKey {
            workflow_type: b"orders",
            state: b"fifo_gate",
            partition_key: b"tenant",
        },
        BTreeMap::new(),
    );
    let records = authority(&parallel, &fifo);
    let mut request = request(10_000);
    request.limit = 1;
    request.states = StateFilter::Only(STATES);
    let selection = select(&view(&parallel, &fifo, &records), request, None)?;
    assert_eq!(
        selection.records()[0].record.id().as_bytes(),
        b"parallel-earlier-due"
    );
    Ok(())
}
