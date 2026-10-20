//! Supplemental boundedness, deterministic continuation and malformed-index decisions.

#![forbid(unsafe_code)]

use wf_domain::claim::{
    AuthoritativeClaimView, ClaimRecord, DueOrder, FifoIndex, LaneKey, LaneOrder, ParallelIndex,
    PartitionFilter, RecordIndex, ScopedRecordId, SelectionError, SelectionRequest, SelectionStop,
    StateFilter, select,
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

fn entry<'a>(record: &'a WorkflowRecord, partition: &'a [u8], seq: u128) -> ClaimRecord<'a> {
    ClaimRecord {
        record,
        partition_key: partition,
        priority: 0,
        state_enter_seq: seq,
    }
}

fn add<'a>(index: &mut ParallelIndex<'a>, entry: ClaimRecord<'a>) -> TestResult {
    index.insert(
        DueOrder::for_record(&entry).ok_or("due order absent")?,
        entry,
    );
    Ok(())
}

fn add_lane<'a>(index: &mut FifoIndex<'a>, entry: ClaimRecord<'a>) -> TestResult {
    index
        .entry(LaneKey::for_record(&entry).ok_or("lane absent")?)
        .or_default()
        .insert(LaneOrder::for_record(&entry), entry);
    Ok(())
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
        revision: 7,
        records,
        parallel,
        fifo,
    }
}

fn request() -> SelectionRequest<'static> {
    SelectionRequest::new(b"group", b"orders", TimestampMillis::new(2_000))
}

#[test]
fn equivalent_reconstructed_snapshot_continuation_uses_values() -> TestResult {
    let a = record(b"a", b"queued", 1_000)?;
    let b = record(b"b", b"queued", 1_000)?;
    let mut parallel = ParallelIndex::new();
    add(&mut parallel, entry(&a, b"tenant", 1))?;
    add(&mut parallel, entry(&b, b"tenant", 2))?;
    let fifo = FifoIndex::new();
    let records = authority(&parallel, &fifo);
    let first = select(&view(&parallel, &fifo, &records), request(), None)?;
    let equivalent_parallel = parallel.clone();
    let equivalent_fifo = fifo.clone();
    let second = select(
        &view(&equivalent_parallel, &equivalent_fifo, &records),
        request(),
        first.continuation(),
    )?;
    assert_eq!(second.records()[0].record.id().as_bytes(), b"b");
    Ok(())
}

#[test]
fn conflicting_scoped_identity_rejects_on_later_continuation_page() -> TestResult {
    let first = record(b"duplicate", b"queued", 1_000)?;
    let conflicting = record(b"duplicate", b"retry", 1_500)?;
    let mut parallel = ParallelIndex::new();
    add(&mut parallel, entry(&first, b"tenant", 1))?;
    add(&mut parallel, entry(&conflicting, b"tenant", 2))?;
    let fifo = FifoIndex::new();
    let records = authority(&parallel, &fifo);
    let authoritative = view(&parallel, &fifo, &records);
    let initial = select(&authoritative, request(), None)?;
    assert_eq!(initial.records().len(), 1);
    let resumed = select(&authoritative, request(), initial.continuation());
    assert!(matches!(resumed, Err(SelectionError::InvalidIndex)));
    Ok(())
}

#[test]
fn exhausted_scan_is_not_reported_as_empty_complete_work() -> TestResult {
    let ignored = record(b"a-ignored", b"ignored", 1_000)?;
    let eligible = record(b"b-eligible", b"queued", 1_000)?;
    let mut parallel = ParallelIndex::new();
    add(&mut parallel, entry(&ignored, b"tenant", 1))?;
    add(&mut parallel, entry(&eligible, b"tenant", 2))?;
    let fifo = FifoIndex::new();
    let records = authority(&parallel, &fifo);
    let authoritative = view(&parallel, &fifo, &records);
    let mut request = request();
    request.states = StateFilter::Only(&[b"queued"]);
    request.scan_budget = 1;
    let initial = select(&authoritative, request, None)?;
    assert_eq!(initial.records(), []);
    assert_eq!(initial.scanned(), 1);
    assert_eq!(initial.stop(), SelectionStop::ScanBudget);
    assert!(initial.continuation().is_some());
    let resumed = select(&authoritative, request, initial.continuation())?;
    assert_eq!(resumed.records()[0].record.id().as_bytes(), b"b-eligible");
    assert_eq!(resumed.stop(), SelectionStop::Complete);
    assert!(resumed.continuation().is_none());
    Ok(())
}

#[test]
fn continuation_rejects_changed_revision_time_or_filters() -> TestResult {
    let a = record(b"a", b"queued", 1_000)?;
    let b = record(b"b", b"queued", 1_000)?;
    let mut parallel = ParallelIndex::new();
    add(&mut parallel, entry(&a, b"tenant", 1))?;
    add(&mut parallel, entry(&b, b"tenant", 2))?;
    let fifo = FifoIndex::new();
    let records = authority(&parallel, &fifo);
    let authoritative = view(&parallel, &fifo, &records);
    let initial = select(&authoritative, request(), None)?;
    let mut changed_view = authoritative;
    changed_view.revision += 1;
    assert!(matches!(
        select(&changed_view, request(), initial.continuation()),
        Err(SelectionError::StaleContinuation)
    ));
    let mut changed_request = request();
    changed_request.now = TimestampMillis::new(2_001);
    assert!(matches!(
        select(&authoritative, changed_request, initial.continuation()),
        Err(SelectionError::StaleContinuation)
    ));
    changed_request = request();
    changed_request.partitions = PartitionFilter::Only(&[b"tenant"]);
    assert!(matches!(
        select(&authoritative, changed_request, initial.continuation()),
        Err(SelectionError::StaleContinuation)
    ));
    Ok(())
}

#[test]
fn error_after_eligible_row_returns_no_partial_success() -> TestResult {
    let a = record(b"a", b"queued", 1_000)?;
    let b = record(b"b", b"queued", 1_000)?;
    let mut parallel = ParallelIndex::new();
    add(&mut parallel, entry(&a, b"tenant", 1))?;
    let second = entry(&b, b"tenant", 2);
    let mut bad_key = DueOrder::for_record(&second).ok_or("due absent")?;
    bad_key.due = TimestampMillis::new(1_001);
    parallel.insert(bad_key, second);
    let fifo = FifoIndex::new();
    let records = authority(&parallel, &fifo);
    let mut request = request();
    request.limit = 10;
    assert!(matches!(
        select(&view(&parallel, &fifo, &records), request, None),
        Err(SelectionError::InvalidIndex)
    ));
    assert_eq!(a.version(), 1);
    assert_eq!(b.version(), 1);
    assert!(a.lease().is_none());
    assert!(b.lease().is_none());
    Ok(())
}

#[test]
fn mismatched_group_and_duplicate_filters_reject_before_selection() -> TestResult {
    let record = record(b"a", b"queued", 1_000)?;
    let mut parallel = ParallelIndex::new();
    add(&mut parallel, entry(&record, b"tenant", 1))?;
    let fifo = FifoIndex::new();
    let records = authority(&parallel, &fifo);
    let authoritative = view(&parallel, &fifo, &records);
    let mut request = request();
    request.group = b"other-group";
    assert!(matches!(
        select(&authoritative, request, None),
        Err(SelectionError::GroupMismatch)
    ));
    request.group = b"group";
    request.states = StateFilter::Only(&[b"queued", b"queued"]);
    assert!(matches!(
        select(&authoritative, request, None),
        Err(SelectionError::InvalidScope)
    ));
    request.states = StateFilter::Any;
    request.partitions = PartitionFilter::Only(&[b"tenant", b"tenant"]);
    assert!(matches!(
        select(&authoritative, request, None),
        Err(SelectionError::InvalidScope)
    ));
    Ok(())
}

#[test]
fn output_scan_priority_and_filter_footprint_bounds_reject() {
    const PARTITIONS: &[&[u8]] = &[b"a", b"b", b"c", b"d", b"e", b"f", b"g", b"h"];
    const STATES: &[&[u8]] = &[b"1", b"2", b"3", b"4", b"5", b"6", b"7", b"8", b"9"];
    let parallel = ParallelIndex::new();
    let fifo = FifoIndex::new();
    let records = authority(&parallel, &fifo);
    let authoritative = view(&parallel, &fifo, &records);
    for limit in [0, 1_001, usize::MAX] {
        let mut request = request();
        request.limit = limit;
        assert!(matches!(
            select(&authoritative, request, None),
            Err(SelectionError::InvalidBudget)
        ));
    }
    for scan_budget in [0, 16_385, usize::MAX] {
        let mut request = request();
        request.scan_budget = scan_budget;
        assert!(matches!(
            select(&authoritative, request, None),
            Err(SelectionError::InvalidBudget)
        ));
    }
    let mut request = request();
    request.priority = Some(3);
    assert!(matches!(
        select(&authoritative, request, None),
        Err(SelectionError::InvalidPriority)
    ));
    request.priority = None;
    request.partitions = PartitionFilter::Only(PARTITIONS);
    request.states = StateFilter::Only(STATES);
    assert!(matches!(
        select(&authoritative, request, None),
        Err(SelectionError::InvalidScope)
    ));
}

#[test]
fn insufficient_head_budget_rejects_without_skipping_an_unread_lane() -> TestResult {
    let a = record(b"a", b"queued", 1_000)?;
    let b = record(b"b", b"queued", 1_000)?;
    let parallel = ParallelIndex::new();
    let mut fifo = FifoIndex::new();
    add_lane(&mut fifo, entry(&a, b"a", 1))?;
    add_lane(&mut fifo, entry(&b, b"b", 2))?;
    let records = authority(&parallel, &fifo);
    let mut request = request();
    request.head_budget = 1;
    assert!(matches!(
        select(&view(&parallel, &fifo, &records), request, None),
        Err(SelectionError::HeadBudget)
    ));
    Ok(())
}

#[test]
fn fifo_priority_and_parallel_lane_overlap_reject_atomically() -> TestResult {
    let record = record(b"a", b"queued", 1_000)?;
    let mut nonzero = entry(&record, b"tenant", 1);
    nonzero.priority = 3;
    let parallel = ParallelIndex::new();
    let mut fifo = FifoIndex::new();
    add_lane(&mut fifo, nonzero)?;
    let records = authority(&parallel, &fifo);
    assert!(matches!(
        select(&view(&parallel, &fifo, &records), request(), None),
        Err(SelectionError::InvalidPriority)
    ));
    let zero = entry(&record, b"tenant", 1);
    let mut fifo = FifoIndex::new();
    add_lane(&mut fifo, zero)?;
    let mut parallel = ParallelIndex::new();
    add(&mut parallel, zero)?;
    let records = authority(&parallel, &fifo);
    assert!(matches!(
        select(&view(&parallel, &fifo, &records), request(), None),
        Err(SelectionError::InvalidFifo)
    ));
    Ok(())
}

#[test]
fn selected_order_is_independent_of_insertion_order_and_inputs_are_borrowed() -> TestResult {
    let a = record(&[255, 0], b"queued", 1_000)?;
    let b = record(b"a", b"queued", 1_000)?;
    let entries = [entry(&a, b"tenant", 1), entry(&b, b"tenant", 2)];
    let mut first = ParallelIndex::new();
    let mut second = ParallelIndex::new();
    for entry in entries {
        add(&mut first, entry)?;
    }
    for entry in entries.into_iter().rev() {
        add(&mut second, entry)?;
    }
    let fifo = FifoIndex::new();
    let records = authority(&first, &fifo);
    let original = first.clone();
    let mut request = request();
    request.limit = 10;
    let left = select(&view(&first, &fifo, &records), request, None)?;
    let right = select(&view(&second, &fifo, &records), request, None)?;
    assert_eq!(left.records(), right.records());
    assert_eq!(left.records()[0].record.id().as_bytes(), b"a");
    assert_eq!(first, original);
    assert_eq!(left.records()[1].record, &a);
    Ok(())
}

#[test]
fn fifo_frontier_merges_with_parallel_priority_due_and_binary_ties() -> TestResult {
    let high = record(b"high", b"retry", 1_900)?;
    let earlier = record(b"earlier", b"retry", 500)?;
    let tied = record(b"a-tie", b"retry", 1_000)?;
    let head = record(b"z-head", b"queued", 1_000)?;
    let follower = record(b"follower", b"queued", 100)?;
    let mut parallel = ParallelIndex::new();
    let mut high_entry = entry(&high, b"tenant", 1);
    high_entry.priority = 2;
    add(&mut parallel, high_entry)?;
    add(&mut parallel, entry(&earlier, b"tenant", 2))?;
    add(&mut parallel, entry(&tied, b"tenant", 3))?;
    let mut fifo = FifoIndex::new();
    add_lane(&mut fifo, entry(&head, b"tenant", 4))?;
    add_lane(&mut fifo, entry(&follower, b"tenant", 5))?;
    let records = authority(&parallel, &fifo);
    let authoritative = view(&parallel, &fifo, &records);
    let mut request = request();
    request.limit = 1;
    request.scan_budget = 1;
    let expected: &[&[u8]] = &[b"high", b"earlier", b"a-tie", b"z-head"];
    let mut continuation = None;
    for (position, expected_id) in expected.iter().enumerate() {
        let selection = select(&authoritative, request, continuation.as_ref())?;
        assert_eq!(selection.records().len(), 1);
        assert_eq!(selection.records()[0].record.id().as_bytes(), *expected_id);
        assert_eq!(selection.scanned(), 1);
        assert_eq!(selection.head_reads(), 1);
        assert_eq!(
            selection.stop(),
            if position == expected.len() - 1 {
                SelectionStop::Complete
            } else {
                SelectionStop::ClaimLimit
            }
        );
        continuation = selection.continuation().copied();
    }
    assert!(continuation.is_none());
    Ok(())
}

#[test]
fn exclusions_and_due_boundary_cannot_release_fifo_follower() -> TestResult {
    let head = record(b"head", b"queued", 2_000)?;
    let follower = record(b"follower", b"queued", 1_000)?;
    let eligible = record(b"other", b"retry", 1_000)?;
    let mut fifo = FifoIndex::new();
    add_lane(&mut fifo, entry(&head, b"tenant", 1))?;
    add_lane(&mut fifo, entry(&follower, b"tenant", 2))?;
    let mut parallel = ParallelIndex::new();
    add(&mut parallel, entry(&eligible, b"tenant", 3))?;
    let records = authority(&parallel, &fifo);
    let authoritative = view(&parallel, &fifo, &records);
    let mut request = request();
    request.now = TimestampMillis::new(1_999);
    request.limit = 10;
    let before = select(&authoritative, request, None)?;
    assert_eq!(before.records().len(), 1);
    assert_eq!(before.records()[0].record.id().as_bytes(), b"other");
    request.now = TimestampMillis::new(2_000);
    let at = select(&authoritative, request, None)?;
    assert_eq!(at.records().len(), 2);
    assert_eq!(at.records()[1].record.id().as_bytes(), b"head");
    request.exclude_states = &[b"queued"];
    let excluded = select(&authoritative, request, None)?;
    assert_eq!(excluded.records(), before.records());
    Ok(())
}

#[test]
fn absent_or_mismatched_canonical_authority_rejects_index_rows() -> TestResult {
    let original = record(b"record", b"queued", 1_000)?;
    let mut parallel = ParallelIndex::new();
    add(&mut parallel, entry(&original, b"tenant", 1))?;
    let fifo = FifoIndex::new();
    let empty = RecordIndex::new();
    assert!(matches!(
        select(&view(&parallel, &fifo, &empty), request(), None),
        Err(SelectionError::InvalidIndex)
    ));
    let different = record(b"record", b"queued", 1_500)?;
    let different_entry = entry(&different, b"tenant", 1);
    let records = RecordIndex::from([(
        ScopedRecordId::for_record(&different_entry),
        different_entry,
    )]);
    assert!(matches!(
        select(&view(&parallel, &fifo, &records), request(), None),
        Err(SelectionError::InvalidIndex)
    ));
    Ok(())
}
