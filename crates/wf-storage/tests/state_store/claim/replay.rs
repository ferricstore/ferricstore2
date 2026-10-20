//! Original receipts survive changed leaf controls and strict physical reopen.

use super::{TestResult, fixtures as f};
use std::path::Path;
use wf_domain::claim::apply::ClaimRejection;
use wf_domain::history::EventKind;
use wf_domain::lease::{Lease, LeaseToken};
use wf_domain::workflow::indexed::OrderingMetadata;
use wf_domain::workflow::request::RetainedStatus;
use wf_domain::workflow::request::{RequestScope, RetainedOutcome};
use wf_domain::workflow::restoration::WorkflowRecordData;
use wf_ports::state::StoredWorkflowRecord;
use wf_ports::state::{GroupApplyStore, GroupApplyTransaction, StateTransactionError};
use wf_storage::transaction::{ClaimTransactionOutcome, apply_claim_transaction};
use wf_types::identity::{PartitionId, RecordId, RequestId};
use wf_types::version::{Fence, TimestampMillis};

#[test]
fn retained_replay_conflict_expiry_and_exact_log_replay_precede_fresh_work() -> TestResult {
    let dir = f::directory("claim-retained")?;
    let path = dir.join("state");
    let s = f::scope()?;
    let (mut expected, saved) = retained_seed(&path, &s)?;
    let response = f::receipt(&[], 0, 0, 0)?;
    let mut invalid = f::query(b"job", b"", 0);
    invalid.lease_ms = 0;
    invalid.states = Some(vec![b"q".to_vec(); 65]);
    let changed = f::phase(&s, b"original", 200, &invalid, (0, 0), [31; 32])?;
    f::finish(
        &mut expected,
        &s,
        &changed,
        f::log(1),
        (f::clock(200)?, f::clock(200)?),
        None,
    )?;
    let replay = f::apply_check(
        &path,
        &s,
        &changed,
        f::log(1),
        f::limits(),
        &expected,
        &response,
    )?;
    assert_eq!(*replay, *saved);
    let conflict = f::phase(&s, b"original", 201, &invalid, (0, 0), [32; 32])?;
    f::finish(
        &mut expected,
        &s,
        &conflict,
        f::log(2),
        (f::clock(201)?, f::clock(201)?),
        None,
    )?;
    let mut store = f::open(&path, &s)?;
    assert!(matches!(
        apply_claim_transaction(&mut store, &s, &conflict, f::log(2), f::limits())?,
        ClaimTransactionOutcome::IntentConflict(_)
    ));
    drop(store);
    assert_eq!(f::cohort(&path)?, expected);
    let advance = f::phase(
        &s,
        b"advance",
        1200,
        &f::query(b"other", b"worker", 1),
        (1, 1),
        [33; 32],
    )?;
    let advance_meta = f::metadata(&s, b"advance", 3, 1200, [33; 32], RetainedStatus::Accepted)?;
    f::finish(
        &mut expected,
        &s,
        &advance,
        f::log(3),
        (f::clock(1200)?, f::clock(1200)?),
        Some((&advance_meta, &response)),
    )?;
    f::apply_check(
        &path,
        &s,
        &advance,
        f::log(3),
        f::limits(),
        &expected,
        &response,
    )?;
    let expired = f::phase(&s, b"original", 1, &invalid, (0, 0), [31; 32])?;
    f::finish(
        &mut expected,
        &s,
        &expired,
        f::log(4),
        (f::clock(1200)?, f::clock(1)?),
        None,
    )?;
    let mut store = f::open(&path, &s)?;
    assert!(matches!(
        apply_claim_transaction(&mut store, &s, &expired, f::log(4), f::limits())?,
        ClaimTransactionOutcome::Expired(_)
    ));
    assert!(matches!(
        apply_claim_transaction(&mut store, &s, &expired, f::log(4), f::limits())?,
        ClaimTransactionOutcome::LogReplay(_)
    ));
    drop(store);
    assert_eq!(f::cohort(&path)?, expected);
    drop(f::open(&path, &s)?);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn claim_strict_reopen_checks_response_count_status_and_identity() -> TestResult {
    let dir = f::directory("claim-strict")?;
    let path = dir.join("state");
    let s = f::scope()?;
    let before = high_attempt_seed(&path, &s)?;
    let mut expected = f::cohort(&path)?;
    let after = f::owner(
        &s,
        b"p",
        WorkflowRecordData {
            state: b"running".to_vec(),
            version: 2,
            fence: Fence::new(1),
            attempts: 70_000,
            updated_at: TimestampMillis::new(25),
            next_run_at: Some(TimestampMillis::new(30025)),
            run_state: Some(b"queued".to_vec()),
            lease: Some(Lease {
                worker: b"worker".to_vec(),
                token: LeaseToken::new(b"worker:25:1".to_vec())?,
                deadline: TimestampMillis::new(30025),
            }),
            payload: Some(vec![0, 255]),
            result: Some(Vec::new()),
            error: Some(b"old".to_vec()),
            latest_kind: EventKind::Claimed,
            latest_version: 2,
            latest_fence: Fence::new(1),
            ..f::data(b"id", b"job", b"queued", 0, Some(0))?
        },
        before.ordering,
    )?;
    let frame = f::phase(
        &s,
        b"lease",
        25,
        &f::query(b"job", b"worker", 1),
        (1, 1),
        [34; 32],
    )?;
    let response = f::receipt(std::slice::from_ref(&after), 0, 1, 0)?;
    let metadata = f::metadata(&s, b"lease", 1, 25, [34; 32], RetainedStatus::Accepted)?;
    f::replace_expected(
        &mut expected,
        &s,
        &after,
        &f::history(&after),
        &[f::due(&s, &before)?],
        &[(f::due(&s, &after)?, vec![0])],
    )?;
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
    let mut store = f::open(&path, &s)?;
    let tx = store.begin_apply(
        s.group(),
        s.recovery(),
        f::limits().base.codec.admission(),
        100,
        1_000_000,
        1_000_000,
    )?;
    assert_eq!(
        tx.record(
            s.tenant(),
            &PartitionId::new(b"p".to_vec())?,
            &RecordId::new(b"id".to_vec())?
        )?
        .ok_or("high-attempt point absent")?
        .record
        .attempts(),
        70_000
    );
    assert_eq!(
        tx.outcome(s.tenant(), &RequestId::new(b"lease".to_vec())?)?
            .ok_or("receipt absent")?
            .response(),
        response
    );
    drop(tx);
    drop(store);
    strict_mutations(&path, &s, &expected, &metadata, &response)?;
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

fn strict_mutations(
    path: &std::path::Path,
    s: &wf_domain::workflow::request::RequestScope,
    expected: &[std::collections::BTreeMap<Vec<u8>, Vec<u8>>; 4],
    metadata: &wf_domain::workflow::request::RetainedMetadata,
    response: &[u8],
) -> TestResult {
    let point = f::key(
        2,
        &[
            s.group().as_bytes(),
            s.recovery().as_bytes(),
            s.tenant().as_bytes(),
            b"lease",
        ],
    );
    for case in 0..6 {
        let mut bytes = response.to_vec();
        let mut meta = metadata.clone();
        match case {
            0 => bytes[17..21].copy_from_slice(&1001_u32.to_be_bytes()),
            1 => bytes[6] = 1, // accepted body cannot be decoded as a rejection.
            2 => bytes[8] = 1, // Complete cannot carry restart advice.
            3 => meta.original_time = TimestampMillis::new(26),
            4 => {
                meta.scope = wf_domain::workflow::request::RequestScope::new(
                    wf_types::identity::TenantId::new(b"other".to_vec())?,
                    s.group().clone(),
                    s.recovery().clone(),
                );
            }
            _ => bytes.push(0),
        }
        let mut changed = expected.clone();
        changed[1].insert(point.clone(), f::outcome_wire(&meta, &bytes)?);
        f::replace_rows(path, &changed[1])?;
        assert!(
            matches!(f::open(path, s), Err(error)
            if error.downcast_ref::<StateTransactionError>() == Some(&StateTransactionError::Corrupt)),
            "strict receipt case {case}"
        );
        assert_eq!(f::cohort(path)?, changed);
    }
    f::replace_rows(path, &expected[1])?;
    drop(f::open(path, s)?);
    Ok(())
}

#[test]
fn claim_rejection_and_empty_acceptance_publish_only_outcome_control() -> TestResult {
    let dir = f::directory("claim-no-business")?;
    let path = dir.join("state");
    let s = f::scope()?;
    drop(f::create(&path, &s, &[b"p"], None)?);
    let mut expected = f::cohort(&path)?;
    let before = expected.clone();
    let mut query = f::query(b"job", b"worker", 1);
    query.lease_ms = 0;
    let invalid = f::phase(&s, b"rejected", 10, &query, (1, 1), [35; 32])?;
    let rejected = b"FWCR\0\x01\x01\x07";
    let metadata = f::metadata(
        &s,
        b"rejected",
        0,
        10,
        [35; 32],
        RetainedStatus::ClaimRejected(ClaimRejection::ZeroLease),
    )?;
    f::finish(
        &mut expected,
        &s,
        &invalid,
        f::log(0),
        (f::clock(10)?, f::clock(10)?),
        Some((&metadata, rejected)),
    )?;
    assert_eq!(
        f::apply_check(
            &path,
            &s,
            &invalid,
            f::log(0),
            f::limits(),
            &expected,
            rejected
        )?
        .status(),
        metadata.status
    );
    let frame = f::phase(
        &s,
        b"empty",
        11,
        &f::query(b"job", b"worker", 1),
        (1, 1),
        [36; 32],
    )?;
    let empty = f::receipt(&[], 0, 0, 0)?;
    let metadata = f::metadata(&s, b"empty", 1, 11, [36; 32], RetainedStatus::Accepted)?;
    f::finish(
        &mut expected,
        &s,
        &frame,
        f::log(1),
        (f::clock(11)?, f::clock(11)?),
        Some((&metadata, &empty)),
    )?;
    f::apply_check(&path, &s, &frame, f::log(1), f::limits(), &expected, &empty)?;
    for slot in [0, 2, 3] {
        assert_eq!(expected[slot], before[slot]);
    }
    for family in [1, 3, 4, 5, 6, 7] {
        assert_eq!(
            expected[1]
                .iter()
                .filter(|(key, _)| key.get(6) == Some(&family))
                .collect::<Vec<_>>(),
            before[1]
                .iter()
                .filter(|(key, _)| key.get(6) == Some(&family))
                .collect::<Vec<_>>()
        );
    }
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

fn retained_seed(path: &Path, s: &RequestScope) -> TestResult<(f::Cohort, Box<RetainedOutcome>)> {
    // Current job governance is coherent genesis, not a policy publication.
    drop(f::create(
        path,
        s,
        &[b"p"],
        Some(f::policy(b"job", &[], true)?),
    )?);
    let mut expected = f::cohort(path)?;
    let original = f::phase(
        s,
        b"original",
        100,
        &f::query(b"other", b"worker", 1),
        (1, 1),
        [31; 32],
    )?;
    let response = f::receipt(&[], 0, 0, 0)?;
    let metadata = f::metadata(s, b"original", 0, 100, [31; 32], RetainedStatus::Accepted)?;
    f::finish(
        &mut expected,
        s,
        &original,
        f::log(0),
        (f::clock(100)?, f::clock(100)?),
        Some((&metadata, &response)),
    )?;
    let saved = f::apply_check(
        path,
        s,
        &original,
        f::log(0),
        f::limits(),
        &expected,
        &response,
    )?;
    // The changed original-ID retry targets job and must bypass this actual
    // unsupported Fresh authority as well as its invalid query controls.
    Ok((expected, saved))
}

fn high_attempt_seed(path: &Path, s: &RequestScope) -> TestResult<StoredWorkflowRecord> {
    let mut store = f::create(path, s, &[b"p"], None)?;
    let original = f::owner(
        s,
        b"p",
        f::data(b"id", b"job", b"queued", 0, Some(0))?,
        OrderingMetadata {
            priority: 0,
            state_enter_seq: 0,
        },
    )?;
    f::seed(&mut store, s, std::slice::from_ref(&original), 0, 0)?;
    drop(store);
    // Restore accepts u64 attempts; the durable receipt must preserve 70000.
    let before = f::owner(
        s,
        b"p",
        WorkflowRecordData {
            attempts: 70_000,
            payload: Some(vec![0, 255]),
            result: Some(Vec::new()),
            error: Some(b"old".to_vec()),
            ..f::data(b"id", b"job", b"queued", 0, Some(0))?
        },
        original.ordering,
    )?;
    let mut expected = f::cohort(path)?;
    expected[1].insert(f::point(s, &before), f::record_wire(s, &before)?);
    let history = f::history_wire(s, &before, &f::history(&before))?;
    expected[1].insert(history.0, history.1);
    f::replace_rows(path, &expected[1])?;
    drop(f::open(path, s)?);
    Ok(before)
}
