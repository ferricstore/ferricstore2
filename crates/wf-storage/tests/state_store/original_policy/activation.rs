//! Real source parallel lease -> ORIGINAL public FIFO SET -> drain, complete, claim.
use super::{TestResult, fixtures as f};
use std::path::Path;
use wf_domain::history::EventKind;
use wf_domain::lease::{Lease, LeaseToken};
use wf_domain::policy::complete::{PolicySchemaCharge, RawFlowPolicyValue as Raw};
use wf_domain::workflow::request::{ClientIntent, RequestScope, RetainedStatus};
use wf_domain::workflow::restoration::WorkflowRecordData;
use wf_ports::log::prepare_public_mutation;
use wf_ports::state::StoredWorkflowRecord;
use wf_storage::transaction::{RecordBatchTransactionOutcome, apply_public_mutation_transaction};
use wf_types::identity::RequestId;
use wf_types::version::{Fence, TimestampMillis};

fn first(s: &RequestScope, priority: u8) -> TestResult<StoredWorkflowRecord> {
    let mut data = f::data(b"first", b"job", b"queued", 1000, Some(5000))?;
    data.payload = Some(b"first".to_vec());
    f::owner(s, b"p", data, priority, 0)
}

fn second(s: &RequestScope, priority: u8) -> TestResult<StoredWorkflowRecord> {
    let mut data = f::data(b"second", b"job", b"queued", 1001, Some(2000))?;
    data.payload = Some(b"second".to_vec());
    f::owner(s, b"p", data, priority, 1_u128 << 32)
}

fn second_running(s: &RequestScope, priority: u8) -> TestResult<StoredWorkflowRecord> {
    f::owner(
        s,
        b"p",
        WorkflowRecordData {
            state: b"running".to_vec(),
            payload: Some(b"second".to_vec()),
            version: 2,
            fence: Fence::new(1),
            updated_at: TimestampMillis::new(2000),
            next_run_at: Some(TimestampMillis::new(32000)),
            run_state: Some(b"queued".to_vec()),
            lease: Some(Lease {
                worker: b"parallel-worker".to_vec(),
                token: LeaseToken::new(b"parallel-worker:2000:1".to_vec())?,
                deadline: TimestampMillis::new(32000),
            }),
            latest_kind: EventKind::Claimed,
            latest_version: 2,
            latest_fence: Fence::new(1),
            ..f::data(b"second", b"job", b"queued", 1001, Some(2000))?
        },
        priority,
        1_u128 << 32,
    )
}

fn first_running(s: &RequestScope, priority: u8) -> TestResult<StoredWorkflowRecord> {
    f::owner(
        s,
        b"p",
        WorkflowRecordData {
            state: b"running".to_vec(),
            payload: Some(b"first".to_vec()),
            version: 2,
            fence: Fence::new(1),
            updated_at: TimestampMillis::new(5101),
            next_run_at: Some(TimestampMillis::new(35101)),
            run_state: Some(b"queued".to_vec()),
            lease: Some(Lease {
                worker: b"fifo-worker".to_vec(),
                token: LeaseToken::new(b"fifo-worker:5101:1".to_vec())?,
                deadline: TimestampMillis::new(35101),
            }),
            latest_kind: EventKind::Claimed,
            latest_version: 2,
            latest_fence: Fence::new(1),
            ..f::data(b"first", b"job", b"queued", 1000, Some(5000))?
        },
        priority,
        0,
    )
}

fn leased_seed(path: &Path, s: &RequestScope, priority: u8) -> TestResult<f::Cohort> {
    let mut store = f::create(path, &[], 64)?;
    f::seed(&mut store, s, &[first(s, priority)?], 0, 1000)?;
    f::seed(&mut store, s, &[second(s, priority)?], 1, 1001)?;
    drop(store);
    let mut expected = f::cohort(path)?;
    for row in [first(s, priority)?, second(s, priority)?] {
        assert_eq!(
            f::rows(&expected)?.get(&f::point(s, &row)),
            Some(&f::record_wire(s, &row)?)
        );
        let (key, bytes) = f::history_wire(s, &row, &f::history(&row))?;
        assert_eq!(f::rows(&expected)?.get(&key), Some(&bytes));
    }
    let after = second_running(s, priority)?;
    let frame = f::claim_frame(
        s,
        b"parallel",
        2000,
        b"job",
        b"parallel-worker",
        Some(&[b"queued"]),
        Some(&[b"p"]),
    )?;
    let response = f::receipt(std::slice::from_ref(&after), 0, 1, 0)?;
    f::put_record(&mut expected, s, &after)?;
    assert_eq!(
        f::rows_mut(&mut expected)?.remove(&f::due(s, &second(s, priority)?)?),
        Some(vec![0])
    );
    f::rows_mut(&mut expected)?.insert(f::due(s, &after)?, vec![0]);
    let metadata = f::metadata(
        s,
        b"parallel",
        2,
        2000,
        ClientIntent::new(1, [61; 32])?,
        RetainedStatus::Accepted,
    )?;
    f::finish(
        &mut expected,
        s,
        &frame,
        f::log(2),
        (f::clock(2000)?, f::clock(2000)?),
        Some((&metadata, 3, &response)),
    )?;
    claim_check(path, s, &frame, 2, &expected, &response)?;
    Ok(expected)
}

#[test]
fn original_public_fifo_setter_drains_real_parallel_lease() -> TestResult {
    for priority in [0, 1, 2] {
        activation(priority)?;
    }
    Ok(())
}

fn activation(priority: u8) -> TestResult {
    // Preserved be9fd6690bf99980: all three claims use the actual default budgets.
    let dir = f::directory(&format!("original-real-drain-{priority}"))?;
    let path = dir.join("state");
    let s = f::scope()?;
    let before = leased_seed(&path, &s, priority)?;
    let mut expected = before.clone();
    let mode = [(Raw::Atom("mode"), Raw::Atom("fifo"))];
    let states = [(Raw::Binary(b"queued"), Raw::Map(&mode))];
    let attrs = [(Raw::Atom("states"), Raw::Map(&states))];
    let profile = f::limits();
    let frame = f::public_frame(
        &s,
        &RequestId::new(b"activate".to_vec())?,
        2001,
        b"job",
        Raw::Map(&attrs),
        Raw::List(&[]),
        &mut PolicySchemaCharge::new(profile.schema),
    )?;
    let mode = f::etf_map(&[(f::atom(b"mode")?, f::atom(b"fifo")?)])?;
    let states = f::etf_map(&[(f::binary(b"queued")?, mode)])?;
    let response = f::etf_policy(1, b"job", &[(b"states", &states)])?;
    let old_first = first(&s, priority)?;
    let running = second_running(&s, priority)?;
    for row in [&old_first, &running] {
        assert_eq!(
            f::rows_mut(&mut expected)?.remove(&f::due(&s, row)?),
            Some(vec![0])
        );
        f::rows_mut(&mut expected)?.insert(f::member(&s, row), Vec::new());
    }
    f::rows_mut(&mut expected)?.insert(f::due(&s, &running)?, vec![1]);
    f::rows_mut(&mut expected)?.insert(f::key(6, &[b"g", b"z", b"t", b"job"]), response.clone());
    let metadata = f::metadata(
        &s,
        b"activate",
        3,
        2001,
        f::frame_intent(&frame, 3)?,
        RetainedStatus::Accepted,
    )?;
    f::finish(
        &mut expected,
        &s,
        &frame,
        f::log(3),
        (f::clock(2001)?, f::clock(2001)?),
        Some((&metadata, 1, &response)),
    )?;
    f::upgrade6(
        &mut expected,
        &s,
        f::catalog(&s, [None, None], None, None, None)?,
    )?;
    let mut store = f::open(&path, 64)?;
    let RecordBatchTransactionOutcome::Retained { outcome, .. } = f::apply(
        &mut store,
        &frame,
        3,
        profile,
        &mut PolicySchemaCharge::new(profile.schema),
    )?
    else {
        return Err("real public SET did not retain".into());
    };
    assert_eq!(outcome.status(), RetainedStatus::Accepted);
    assert_eq!(outcome.intent().version(), 4);
    assert_eq!(outcome.response(), response);
    assert_eq!(outcome.original_position(), f::log(3));
    drop(store);
    assert_eq!(f::cohort(&path)?, expected);
    drop(f::open(&path, 64)?);
    for (key, bytes) in f::rows(&before)?
        .iter()
        .filter(|(key, _)| matches!(key.get(6), Some(1 | 5)))
    {
        assert_eq!(
            f::rows(&expected)?.get(key),
            Some(bytes),
            "publication changed core/history"
        );
    }
    blocked_claim(&path, &s, &mut expected, priority)?;
    complete_running(&path, &s, &mut expected, priority)?;
    claim_first(&path, &s, &mut expected, priority)?;
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

fn blocked_claim(
    path: &Path,
    s: &RequestScope,
    expected: &mut f::Cohort,
    priority: u8,
) -> TestResult {
    let frame = f::claim_frame(
        s,
        b"blocked",
        5000,
        b"job",
        b"fifo-worker",
        Some(&[b"queued"]),
        Some(&[b"p"]),
    )?;
    let response = f::receipt(&[], 0, 0, 1)?;
    let metadata = f::metadata(
        s,
        b"blocked",
        4,
        5000,
        ClientIntent::new(1, [61; 32])?,
        RetainedStatus::Accepted,
    )?;
    f::finish(
        expected,
        s,
        &frame,
        f::log(4),
        (f::clock(5000)?, f::clock(5000)?),
        Some((&metadata, 3, &response)),
    )?;
    claim_check(path, s, &frame, 4, expected, &response)?;
    let running = second_running(s, priority)?;
    assert_eq!(
        f::rows(expected)?.get(&f::point(s, &running)),
        Some(&f::record_wire(s, &running)?)
    );
    assert_eq!(
        f::rows(expected)?.get(&f::due(s, &running)?),
        Some(&vec![1])
    );
    Ok(())
}

fn complete_running(
    path: &Path,
    s: &RequestScope,
    expected: &mut f::Cohort,
    priority: u8,
) -> TestResult {
    let before = second_running(s, priority)?;
    let after = f::owner(
        s,
        b"p",
        WorkflowRecordData {
            state: b"completed".to_vec(),
            payload: Some(b"second".to_vec()),
            version: 3,
            fence: Fence::new(1),
            updated_at: TimestampMillis::new(5100),
            next_run_at: None,
            run_state: Some(b"queued".to_vec()),
            latest_kind: EventKind::Completed,
            latest_version: 3,
            latest_fence: Fence::new(1),
            ..f::data(b"second", b"job", b"queued", 1001, None)?
        },
        priority,
        1_u128 << 32,
    )?;
    let frame = f::complete_frame(s, &RequestId::new(b"complete".to_vec())?, &before, 5100)?;
    f::put_record(expected, s, &after)?;
    assert!(
        f::rows_mut(expected)?
            .remove(&f::due(s, &before)?)
            .is_some()
    );
    assert!(
        f::rows_mut(expected)?
            .remove(&f::member(s, &before))
            .is_some()
    );
    f::rows_mut(expected)?.insert(f::due(s, &first(s, priority)?)?, vec![1]);
    let metadata = f::metadata(
        s,
        b"complete",
        5,
        5100,
        prepare_public_mutation(&frame, f::limits().codec, f::limits().workspace_bytes)?
            .header(f::log(5))
            .intent,
        RetainedStatus::Accepted,
    )?;
    f::finish(
        expected,
        s,
        &frame,
        f::log(5),
        (f::clock(5100)?, f::clock(5100)?),
        Some((&metadata, 2, &[0])),
    )?;
    let mut store = f::open(path, 64)?;
    assert!(
        matches!(apply_public_mutation_transaction(&mut store, s, &frame, f::log(5), f::limits())?,
        RecordBatchTransactionOutcome::Retained { outcome, .. } if outcome.response() == [0])
    );
    drop(store);
    assert_eq!(f::cohort(path)?, *expected);
    drop(f::open(path, 64)?);
    Ok(())
}

fn claim_first(
    path: &Path,
    s: &RequestScope,
    expected: &mut f::Cohort,
    priority: u8,
) -> TestResult {
    let after = first_running(s, priority)?;
    let frame = f::claim_frame(
        s,
        b"next",
        5101,
        b"job",
        b"fifo-worker",
        Some(&[b"queued"]),
        Some(&[b"p"]),
    )?;
    let response = f::receipt(std::slice::from_ref(&after), 0, 1, 1)?;
    f::put_record(expected, s, &after)?;
    assert!(
        f::rows_mut(expected)?
            .remove(&f::due(s, &first(s, priority)?)?)
            .is_some()
    );
    assert!(
        f::rows_mut(expected)?
            .remove(&f::member(s, &first(s, priority)?))
            .is_some()
    );
    f::rows_mut(expected)?.insert(f::due(s, &after)?, vec![1]);
    f::rows_mut(expected)?.insert(f::member(s, &after), Vec::new());
    let metadata = f::metadata(
        s,
        b"next",
        6,
        5101,
        ClientIntent::new(1, [61; 32])?,
        RetainedStatus::Accepted,
    )?;
    f::finish(
        expected,
        s,
        &frame,
        f::log(6),
        (f::clock(5101)?, f::clock(5101)?),
        Some((&metadata, 3, &response)),
    )?;
    claim_check(path, s, &frame, 6, expected, &response)
}

fn claim_check(
    path: &Path,
    s: &RequestScope,
    frame: &[u8],
    at: u64,
    expected: &f::Cohort,
    response: &[u8],
) -> TestResult {
    let mut store = f::open(path, 64)?;
    f::claim(&mut store, s, frame, at, response)?;
    drop(store);
    assert_eq!(f::cohort(path)?, *expected);
    drop(f::open(path, 64)?);
    Ok(())
}
