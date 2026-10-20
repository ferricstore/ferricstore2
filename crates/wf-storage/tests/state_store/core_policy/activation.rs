//! Original activation executes a real lease, policy publication and lease drain.

use super::{TestResult, fixtures as f};

mod isolation;
use std::path::Path;
use wf_domain::history::EventKind;
use wf_domain::lease::{Lease, LeaseGuard, LeaseToken};
use wf_domain::policy::definition::StateMode;
use wf_domain::workflow::request::{ClientIntent, RequestScope, RetainedStatus};
use wf_domain::workflow::restoration::WorkflowRecordData;
use wf_domain::workflow::{CommittedCommand, Operation, Terminal};
use wf_ports::consensus::PreparedBatchItem;
use wf_ports::state::StoredWorkflowRecord;
use wf_storage::transaction::{RecordBatchTransactionOutcome, apply_record_batch_transaction};
use wf_types::identity::{PartitionId, TenantId};
use wf_types::version::{Fence, TimestampMillis};

fn first(s: &RequestScope) -> TestResult<StoredWorkflowRecord> {
    let mut data = f::data(b"first", b"job", b"queued", 1000, Some(5000))?;
    data.payload = Some(b"first".to_vec());
    f::owner(s, b"p", data, 0, 0)
}

fn second(s: &RequestScope) -> TestResult<StoredWorkflowRecord> {
    let mut data = f::data(b"second", b"job", b"queued", 1001, Some(2000))?;
    data.payload = Some(b"second".to_vec());
    f::owner(s, b"p", data, 0, 1_u128 << 32)
}

fn second_running(s: &RequestScope) -> TestResult<StoredWorkflowRecord> {
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
        0,
        1_u128 << 32,
    )
}

fn first_running(s: &RequestScope) -> TestResult<StoredWorkflowRecord> {
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
        0,
        0,
    )
}

fn leased_seed(path: &Path, s: &RequestScope) -> TestResult<f::Cohort> {
    let mut store = f::create(path, s, &[b"p"], &[])?;
    f::seed(&mut store, s, &[first(s)?], 0, 1000)?;
    f::seed(&mut store, s, &[second(s)?], 1, 1001)?;
    drop(store);
    let mut expected = f::cohort(path)?;
    for row in [first(s)?, second(s)?] {
        assert_eq!(
            f::rows(&expected)?.get(&f::point(s, &row)),
            Some(&f::record_wire(s, &row)?)
        );
        let (key, bytes) = f::history_wire(s, &row, &f::history(&row))?;
        assert_eq!(f::rows(&expected)?.get(&key), Some(&bytes));
    }
    let after = second_running(s)?;
    let frame = f::claim_frame_scoped(
        s,
        b"parallel",
        2000,
        b"job",
        b"parallel-worker",
        &[b"queued"],
        &[b"p"],
    )?;
    let response = f::receipt(std::slice::from_ref(&after), 0, 1, 0)?;
    f::put_record(&mut expected, s, &after)?;
    assert_eq!(
        f::rows_mut(&mut expected)?.remove(&f::due(s, &second(s)?)?),
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
    f::claim_check(path, s, &frame, 2, &expected, &response)?;
    Ok(expected)
}

#[test]
fn parallel_lease_then_real_fifo_publication_drains_before_first() -> TestResult {
    // Preserved be9fd6690bf99980: all three claims use the actual default budgets.
    let dir = f::directory("policy-real-drain")?;
    let path = dir.join("state");
    let s = f::scope()?;
    let before = leased_seed(&path, &s)?;
    let mut expected = before.clone();
    let frame = f::frame(
        &s,
        b"activate",
        2001,
        b"job",
        Some(0),
        f::mode_patch(&[(b"queued", Some(StateMode::Fifo))]),
    )?;
    let response = f::policy_wire(b"job", 1, &[(b"queued", StateMode::Fifo)], false)?;
    let old_first = first(&s)?;
    let running = second_running(&s)?;
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
        f::frame_intent(frame.as_bytes(), 3)?,
        RetainedStatus::Accepted,
    )?;
    f::finish(
        &mut expected,
        &s,
        frame.as_bytes(),
        f::log(3),
        (f::clock(2001)?, f::clock(2001)?),
        Some((&metadata, 1, &response)),
    )?;
    f::upgrade5(
        &mut expected,
        &s,
        f::catalog(&s, [None, None], None, None, None)?,
    )?;
    let receipt = f::apply(&path, &s, frame.as_bytes(), 3, &expected, &response)?;
    assert_eq!(receipt.status(), RetainedStatus::Accepted);
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
    blocked_claim(&path, &s, &mut expected)?;
    complete_running(&path, &s, &mut expected)?;
    claim_first(&path, &s, &mut expected)?;
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

fn blocked_claim(path: &Path, s: &RequestScope, expected: &mut f::Cohort) -> TestResult {
    let frame = f::claim_frame_scoped(
        s,
        b"blocked",
        5000,
        b"job",
        b"fifo-worker",
        &[b"queued"],
        &[b"p"],
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
    f::claim_check(path, s, &frame, 4, expected, &response)?;
    let running = second_running(s)?;
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

fn complete_running(path: &Path, s: &RequestScope, expected: &mut f::Cohort) -> TestResult {
    let before = second_running(s)?;
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
        0,
        1_u128 << 32,
    )?;
    let item = PreparedBatchItem {
        partition: PartitionId::new(b"p".to_vec())?,
        create_priority: 0,
        command: CommittedCommand {
            id: before.record.id().clone(),
            now: TimestampMillis::new(5100),
            operation: Operation::Complete(Terminal {
                guard: LeaseGuard {
                    token: LeaseToken::new(b"parallel-worker:2000:1".to_vec())?,
                    fence: Fence::new(1),
                },
                payload: None,
                value: None,
            }),
        },
    };
    let frame = f::batch(s, b"complete", 5100, vec![item])?;
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
    f::rows_mut(expected)?.insert(f::due(s, &first(s)?)?, vec![1]);
    let metadata = f::metadata(
        s,
        b"complete",
        5,
        5100,
        f::batch_intent(frame.as_bytes(), 5)?,
        RetainedStatus::Accepted,
    )?;
    f::finish(
        expected,
        s,
        frame.as_bytes(),
        f::log(5),
        (f::clock(5100)?, f::clock(5100)?),
        Some((&metadata, 1, &[0])),
    )?;
    let mut store = f::open(path, s)?;
    assert!(
        matches!(apply_record_batch_transaction(&mut store, s, frame.as_bytes(), f::log(5), f::limits())?,
        RecordBatchTransactionOutcome::Retained { outcome, .. } if outcome.response() == [0])
    );
    drop(store);
    assert_eq!(f::cohort(path)?, *expected);
    drop(f::open(path, s)?);
    Ok(())
}

fn claim_first(path: &Path, s: &RequestScope, expected: &mut f::Cohort) -> TestResult {
    let after = first_running(s)?;
    let frame = f::claim_frame_scoped(
        s,
        b"next",
        5101,
        b"job",
        b"fifo-worker",
        &[b"queued"],
        &[b"p"],
    )?;
    let response = f::receipt(std::slice::from_ref(&after), 0, 1, 1)?;
    f::put_record(expected, s, &after)?;
    assert!(
        f::rows_mut(expected)?
            .remove(&f::due(s, &first(s)?)?)
            .is_some()
    );
    assert!(
        f::rows_mut(expected)?
            .remove(&f::member(s, &first(s)?))
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
    f::claim_check(path, s, &frame, 6, expected, &response)
}

#[test]
fn complete_patch_reindexes_unscheduled_running_and_reverse_modes() -> TestResult {
    let dir = f::directory("policy-canonical")?;
    let path = dir.join("state");
    let s = f::scope()?;
    let policy = f::policy(
        b"job",
        1,
        &[
            (b"queued", StateMode::Parallel),
            (b"removed", StateMode::Fifo),
        ],
        false,
    )?;
    let mut store = f::create(&path, &s, &[b"p"], &[(s.tenant().clone(), policy)])?;
    let mut originals = vec![
        f::owner(
            &s,
            b"p",
            f::data(b"unscheduled", b"job", b"queued", 0, Some(0))?,
            2,
            0,
        )?,
        f::owner(
            &s,
            b"p",
            f::data(b"future", b"job", b"queued", 0, Some(9000))?,
            1,
            1,
        )?,
        f::owner(
            &s,
            b"p",
            f::data(b"running", b"job", b"queued", 0, Some(0))?,
            2,
            2,
        )?,
        f::owner(
            &s,
            b"p",
            f::data(b"removed", b"job", b"removed", 0, Some(0))?,
            0,
            3,
        )?,
    ];
    f::seed(&mut store, &s, &originals, 0, 0)?;
    drop(store);
    let mut expected = f::cohort(&path)?;
    // This component is a valid restored unscheduled record, not a claim that
    // Create(None) creates one: source Create defaults None to committed now.
    assert_eq!(
        f::rows_mut(&mut expected)?.remove(&f::due(&s, &originals[0])?),
        Some(vec![0])
    );
    originals[0] = f::owner(
        &s,
        b"p",
        f::data(b"unscheduled", b"job", b"queued", 0, None)?,
        2,
        0,
    )?;
    f::put_record(&mut expected, &s, &originals[0])?;
    f::replace(&path, &expected)?;
    drop(f::open(&path, &s)?);
    assert_eq!(f::cohort(&path)?, expected);
    let running = canonical_running(&s)?;
    let claim = f::claim_frame(&s, b"lease", 100, b"job", b"w")?;
    let claim_response = f::receipt(std::slice::from_ref(&running), 1, 1, 1)?;
    f::put_record(&mut expected, &s, &running)?;
    assert!(
        f::rows_mut(&mut expected)?
            .remove(&f::due(&s, &originals[2])?)
            .is_some()
    );
    f::rows_mut(&mut expected)?.insert(f::due(&s, &running)?, vec![0]);
    let metadata = f::metadata(
        &s,
        b"lease",
        1,
        100,
        ClientIntent::new(1, [61; 32])?,
        RetainedStatus::Accepted,
    )?;
    f::finish(
        &mut expected,
        &s,
        &claim,
        f::log(1),
        (f::clock(100)?, f::clock(100)?),
        Some((&metadata, 3, &claim_response)),
    )?;
    f::claim_check(&path, &s, &claim, 1, &expected, &claim_response)?;
    let mut rows = originals;
    rows[2] = running;
    publish_reverse(&path, &s, &mut expected, &rows, true)?;
    publish_reverse(&path, &s, &mut expected, &rows, false)?;
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

fn canonical_running(s: &RequestScope) -> TestResult<StoredWorkflowRecord> {
    f::owner(
        s,
        b"p",
        WorkflowRecordData {
            state: b"running".to_vec(),
            version: 2,
            fence: Fence::new(1),
            updated_at: TimestampMillis::new(100),
            next_run_at: Some(TimestampMillis::new(30100)),
            run_state: Some(b"queued".to_vec()),
            lease: Some(Lease {
                worker: b"w".to_vec(),
                token: LeaseToken::new(b"w:100:1".to_vec())?,
                deadline: TimestampMillis::new(30100),
            }),
            latest_kind: EventKind::Claimed,
            latest_version: 2,
            latest_fence: Fence::new(1),
            ..f::data(b"running", b"job", b"queued", 0, Some(0))?
        },
        2,
        2,
    )
}

fn publish_reverse(
    path: &Path,
    s: &RequestScope,
    expected: &mut f::Cohort,
    rows: &[StoredWorkflowRecord],
    forward: bool,
) -> TestResult {
    let before = expected.clone();
    let (at, now, generation, mode) = if forward {
        (2, 101, 2, StateMode::Fifo)
    } else {
        (3, 102, 3, StateMode::Parallel)
    };
    let changes = if forward {
        vec![
            (b"queued".as_slice(), Some(mode)),
            (b"removed".as_slice(), None),
        ]
    } else {
        vec![(b"queued".as_slice(), Some(mode))]
    };
    let request = [b"reverse".as_slice(), &[u8::from(forward)]].concat();
    let frame = f::frame(
        s,
        &request,
        now,
        b"job",
        Some(generation - 1),
        f::mode_patch(&changes),
    )?;
    let response = f::policy_wire(b"job", generation, &[(b"queued", mode)], false)?;
    if forward {
        for row in &rows[..3] {
            if row.record.next_run_at().is_some() {
                assert!(f::rows_mut(expected)?.remove(&f::due(s, row)?).is_some());
            }
            f::rows_mut(expected)?.insert(f::member(s, row), Vec::new());
        }
        f::rows_mut(expected)?.insert(f::due(s, &rows[2])?, vec![1]);
        assert!(
            f::rows_mut(expected)?
                .remove(&f::member(s, &rows[3]))
                .is_some()
        );
        assert_eq!(
            f::rows_mut(expected)?.insert(f::due(s, &rows[3])?, vec![0]),
            Some(vec![1])
        );
    } else {
        assert_eq!(
            f::rows_mut(expected)?.remove(&f::due(s, &rows[2])?),
            Some(vec![1])
        );
        for row in &rows[..3] {
            assert!(f::rows_mut(expected)?.remove(&f::member(s, row)).is_some());
            if row.record.next_run_at().is_some() {
                f::rows_mut(expected)?.insert(f::due(s, row)?, vec![0]);
            }
        }
    }
    f::rows_mut(expected)?.insert(f::key(6, &[b"g", b"z", b"t", b"job"]), response.clone());
    let metadata = f::metadata(
        s,
        &request,
        at,
        now,
        f::frame_intent(frame.as_bytes(), at)?,
        RetainedStatus::Accepted,
    )?;
    f::finish(
        expected,
        s,
        frame.as_bytes(),
        f::log(at),
        (f::clock(now)?, f::clock(now)?),
        Some((&metadata, 1, &response)),
    )?;
    f::upgrade5(expected, s, f::catalog(s, [None, None], None, None, None)?)?;
    f::apply(path, s, frame.as_bytes(), at, expected, &response)?;
    for (key, value) in f::rows(&before)?
        .iter()
        .filter(|(key, _)| matches!(key.get(6), Some(1 | 5)))
    {
        assert_eq!(f::rows(expected)?.get(key), Some(value));
    }
    assert!(rows[0].record.next_run_at().is_none());
    Ok(())
}
