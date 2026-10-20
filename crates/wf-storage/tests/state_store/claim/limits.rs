//! Separate semantic frontiers and cumulative real-adapter resource accounts.

use super::{TestResult, fixtures as f};

mod filtered;
use std::collections::BTreeMap;
use std::path::Path;
use wf_domain::claim::PartitionFilter;
use wf_domain::claim::apply::{ClaimQuery, ClaimStates};
use wf_domain::history::EventKind;
use wf_domain::lease::{Lease, LeaseToken};
use wf_domain::workflow::admission::AdmissionLimits;
use wf_domain::workflow::indexed::OrderingMetadata;
use wf_domain::workflow::request::RequestScope;
use wf_domain::workflow::request::RetainedStatus;
use wf_domain::workflow::restoration::WorkflowRecordData;
use wf_ports::log::{CodecLimits, RecordBatchApplyError};
use wf_ports::state::StoredWorkflowRecord;
use wf_ports::state::{
    ClaimLaneStep, GroupApplyStore, GroupApplyTransaction, GroupClaimLaneCursor,
    GroupClaimTransaction, StateTransactionError,
};
use wf_storage::transaction::{
    ClaimTransactionError, ClaimTransactionLimits, ClaimTransactionOutcome, apply_claim_transaction,
};
use wf_types::identity::RequestId;
use wf_types::version::{Fence, TimestampMillis};

fn refusal(
    path: &std::path::Path,
    frame: &[u8],
    caps: ClaimTransactionLimits,
) -> TestResult<ClaimTransactionError> {
    let before = f::cohort(path)?;
    let s = f::scope()?;
    let mut store = f::open(path, &s)?;
    let Err(error) = apply_claim_transaction(&mut store, &s, frame, f::log(0), caps) else {
        return Err("resource refusal was accepted".into());
    };
    // Resource refusal must leave this same healthy handle usable.
    drop(store.begin_apply(
        s.group(),
        s.recovery(),
        AdmissionLimits::default(),
        100,
        1_000_000,
        1_000_000,
    )?);
    drop(store);
    assert_eq!(f::cohort(path)?, before);
    drop(f::open(path, &s)?);
    Ok(error)
}

#[test]
fn exact_tight_decode_read_stage_adapter_workspace_and_steps() -> TestResult {
    let dir = f::directory("claim-resource")?;
    let path = dir.join("state");
    let s = f::scope()?;
    drop(f::create(&path, &s, &[b"p"], None)?);
    let mut query = f::query(b"job", b"w", 1);
    query.lease_ms = 0;
    let frame = f::phase(&s, b"q", 25, &query, (1, 1), [41; 32])?;
    let mut caps = f::limits();
    caps.base.codec = CodecLimits::new(AdmissionLimits::new(64, 4096, 1)?);
    // No Fresh load is permitted after the sealed ZeroLease result. Seven
    // actual operations: control, retained open/end, outcome, control, two puts.
    caps.base.maximum_read_steps = 7;
    caps.base.maximum_read_bytes = 1024;
    caps.base.maximum_staging_bytes = 1;
    // Independent escaped keys + scope/bound copies + exact FWSO/control wires.
    caps.base.maximum_adapter_bytes =
        13 + (515 + 16 + 16) + (515 + 19) + 13 + (19 + 114) + (13 + 110);
    assert_eq!(caps.base.maximum_adapter_bytes, 1363);
    let live = caps.base.maximum_read_bytes
        + caps.base.maximum_staging_bytes
        + caps.base.maximum_adapter_bytes;
    let late = frame.len() + 4096 + 8 + 4 + 1024 + live;
    caps.base.workspace_bytes = late;
    for field in 0..4 {
        let mut tight = caps;
        match field {
            0 => tight.base.maximum_read_steps -= 1,
            1 => tight.base.maximum_read_bytes -= 1,
            2 => tight.base.maximum_adapter_bytes -= 1,
            _ => tight.base.workspace_bytes -= 1,
        }
        let error = refusal(&path, &frame, tight)?;
        if field < 3 {
            assert_eq!(
                error,
                ClaimTransactionError::Transaction(StateTransactionError::Limit)
            );
        } else {
            assert!(matches!(
                error,
                ClaimTransactionError::Apply(RecordBatchApplyError::Workspace { .. })
            ));
        }
    }
    let mut expected = f::cohort(&path)?;
    let response = b"FWCR\0\x01\x01\x07";
    let metadata = f::metadata(
        &s,
        b"q",
        0,
        25,
        [41; 32],
        RetainedStatus::ClaimRejected(wf_domain::claim::apply::ClaimRejection::ZeroLease),
    )?;
    f::finish(
        &mut expected,
        &s,
        &frame,
        f::log(0),
        (f::clock(25)?, f::clock(25)?),
        Some((&metadata, response)),
    )?;
    f::apply_check(&path, &s, &frame, f::log(0), caps, &expected, response)?;
    reduced_response_profile(&dir)?;
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

fn reduced_response_profile(dir: &std::path::Path) -> TestResult {
    let path = dir.join("profile");
    let s = f::scope()?;
    let mut store = f::create(&path, &s, &[b"p"], None)?;
    let rows = [b"a".as_slice(), b"b".as_slice()]
        .into_iter()
        .enumerate()
        .map(|(i, id)| {
            f::owner(
                &s,
                b"p",
                f::data(id, b"job", b"q", 0, Some(0))?,
                OrderingMetadata {
                    priority: 0,
                    state_enter_seq: i as u128,
                },
            )
        })
        .collect::<TestResult<Vec<_>>>()?;
    f::seed(&mut store, &s, &rows, 0, 0)?;
    let frame = f::phase(
        &s,
        b"two",
        25,
        &f::query(b"job", b"worker", 2),
        (2, 1),
        [42; 32],
    )?;
    assert!(matches!(
        apply_claim_transaction(&mut store, &s, &frame, f::log(1), f::limits())?,
        ClaimTransactionOutcome::Retained { .. }
    ));
    let request = RequestId::new(b"two".to_vec())?;
    for admission in [
        AdmissionLimits::new(64, 4096, 1)?,
        AdmissionLimits::new(6, 4096, 2)?,
    ] {
        let tx = store.begin_apply(
            s.group(),
            s.recovery(),
            admission,
            100,
            1_000_000,
            1_000_000,
        )?;
        assert_eq!(
            tx.outcome(s.tenant(), &request),
            Err(StateTransactionError::Limit)
        );
        drop(tx);
        let tx = store.begin_apply(
            s.group(),
            s.recovery(),
            AdmissionLimits::default(),
            100,
            1_000_000,
            1_000_000,
        )?;
        assert_eq!(
            tx.outcome(s.tenant(), &request)?
                .ok_or("healthy receipt absent")?
                .response_version(),
            3
        );
    }
    drop(store);
    let saved = f::cohort(&path)?;
    drop(f::open(&path, &s)?);
    let point = f::key(
        2,
        &[
            s.group().as_bytes(),
            s.recovery().as_bytes(),
            s.tenant().as_bytes(),
            b"two",
        ],
    );
    let metadata = f::metadata(&s, b"two", 1, 25, [42; 32], RetainedStatus::Accepted)?;
    for response in [
        b"FWCR\0\x01\0\0\0\0\0\0\0\0\0\0\0\0\0\x03\xe9".to_vec(),
        b"FWCR\0\x01\0\0\0\0\0\0\0\0\0\0\0\0\0\0\x01".to_vec(),
    ] {
        let mut broken = saved.clone();
        broken[1].insert(point.clone(), f::outcome_wire(&metadata, &response)?);
        f::replace_rows(&path, &broken[1])?;
        assert!(
            matches!(f::open(&path, &s), Err(error) if error.downcast_ref::<StateTransactionError>() == Some(&StateTransactionError::Corrupt))
        );
        assert_eq!(f::cohort(&path)?, broken);
    }
    f::replace_rows(&path, &saved[1])?;
    drop(f::open(&path, &s)?);
    Ok(())
}

fn borrowed_query<'a>(kind: &'a [u8], states: ClaimStates<'a>) -> ClaimQuery<'a> {
    ClaimQuery {
        workflow_type: kind,
        partitions: PartitionFilter::Any,
        states,
        exclude_states: &[],
        priority: None,
        worker: b"worker",
        lease_ms: 30_000,
        limit: 1,
    }
}

#[test]
fn actual_fifo_lane_jump_and_head_budget_include_empty_probes() -> TestResult {
    let dir = f::directory("claim-lane-jump")?;
    let path = dir.join("state");
    let s = f::scope()?;
    let mut store = f::create(
        &path,
        &s,
        &[b"p"],
        Some(f::policy(b"job", &[b"empty", b"q"], false)?),
    )?;
    let owners = (0_u32..81)
        .map(|i| {
            f::owner(
                &s,
                b"p",
                f::data(
                    format!("id-{i:03}").as_bytes(),
                    b"job",
                    b"q",
                    0,
                    Some(if i == 0 { 1000 } else { 0 }),
                )?,
                OrderingMetadata {
                    priority: 0,
                    state_enter_seq: u128::from(i),
                },
            )
        })
        .collect::<TestResult<Vec<_>>>()?;
    f::seed(&mut store, &s, &owners, 0, 0)?;
    check_lane_jump(&mut store, &s, &owners[0])?;
    drop(store);
    let mut expected = f::cohort(&path)?;
    let mut query = f::query(b"job", b"worker", 1);
    query.states = Some(vec![b"q".to_vec(), b"empty".to_vec()]);
    query.partitions = Some(vec![b"p".to_vec()]);
    let frame = f::phase(&s, b"heads", 25, &query, (1, 1), [44; 32])?;
    let response = b"FWCR\0\x01\x01\x03";
    let metadata = f::metadata(
        &s,
        b"heads",
        1,
        25,
        [44; 32],
        RetainedStatus::ClaimRejected(wf_domain::claim::apply::ClaimRejection::HeadBudget),
    )?;
    f::finish(
        &mut expected,
        &s,
        &frame,
        f::log(1),
        (f::clock(25)?, f::clock(25)?),
        Some((&metadata, response)),
    )?;
    f::apply_check(
        &path,
        &s,
        &frame,
        f::log(1),
        f::limits(),
        &expected,
        response,
    )?;
    query.states = Some(vec![b"empty".to_vec()]);
    let empty = f::phase(&s, b"empty", 25, &query, (1, 1), [45; 32])?;
    let response = f::receipt(&[], 0, 0, 1)?;
    let metadata = f::metadata(&s, b"empty", 2, 25, [45; 32], RetainedStatus::Accepted)?;
    f::finish(
        &mut expected,
        &s,
        &empty,
        f::log(2),
        (f::clock(25)?, f::clock(25)?),
        Some((&metadata, &response)),
    )?;
    f::apply_check(
        &path,
        &s,
        &empty,
        f::log(2),
        f::limits(),
        &expected,
        &response,
    )?;
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn selected_plan_output_and_complete_policy_overlaps() -> TestResult {
    let dir = f::directory("claim-output")?;
    let s = f::scope()?;
    for (label, governance) in [("selected", false), ("governance", true)] {
        let path = dir.join(label);
        let policy = f::policy(b"job", &[b"q"], governance)?;
        let policy_read = 2048 + 216 * policy.encoded().len();
        let mut store = f::create(&path, &s, &[b"p"], Some(policy))?;
        let before = f::owner(
            &s,
            b"p",
            f::data(b"id", b"job", b"q", 0, Some(0))?,
            OrderingMetadata {
                priority: 0,
                state_enter_seq: 0,
            },
        )?;
        f::seed(&mut store, &s, std::slice::from_ref(&before), 0, 0)?;
        drop(store);
        let frame = f::phase(
            &s,
            b"lease",
            25,
            &f::query(b"job", b"worker", 1),
            (1, 1),
            [46; 32],
        )?;
        let original = f::cohort(&path)?;
        let mut caps = f::limits();
        caps.base.maximum_read_bytes = 512 + policy_read - 1;
        let mut store = f::open(&path, &s)?;
        assert_eq!(
            apply_claim_transaction(&mut store, &s, &frame, f::log(1), caps),
            Err(ClaimTransactionError::Transaction(
                StateTransactionError::Limit
            ))
        );
        drop(store);
        assert_eq!(f::cohort(&path)?, original);
        if governance {
            let mut store = f::open(&path, &s)?;
            assert_eq!(
                apply_claim_transaction(&mut store, &s, &frame, f::log(1), f::limits()),
                Err(ClaimTransactionError::UnsupportedGovernance)
            );
            drop(store);
            assert_eq!(f::cohort(&path)?, original);
            continue;
        }
        for field in 0..2 {
            let mut caps = f::limits();
            match field {
                0 => caps.base.maximum_staging_bytes = 1023,
                _ => caps.base.maximum_adapter_bytes = 1,
            }
            let mut store = f::open(&path, &s)?;
            assert!(
                apply_claim_transaction(&mut store, &s, &frame, f::log(1), caps).is_err(),
                "field {field} escaped admission"
            );
            drop(store);
            assert_eq!(f::cohort(&path)?, original);
        }
        selected_output_cohort(&path, &s, &before, original, &frame)?;
    }
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

fn check_lane_jump(
    store: &mut wf_storage::state::RedbStateStore,
    s: &RequestScope,
    first: &StoredWorkflowRecord,
) -> TestResult {
    for steps in [4, 5] {
        let tx = store.begin_apply(
            s.group(),
            s.recovery(),
            AdmissionLimits::default(),
            steps,
            1_000_000,
            1_000_000,
        )?;
        let mut cursor =
            tx.claim_lane_cursor(s.tenant(), borrowed_query(b"job", ClaimStates::Any))?;
        assert!(
            matches!(cursor.next()?, ClaimLaneStep::Lane(lane) if lane.state == b"q" && lane.partition_key == b"p")
        );
        assert!(
            matches!(
                (steps, cursor.next()?),
                (4, ClaimLaneStep::BudgetEnd) | (5, ClaimLaneStep::RangeEnd)
            ),
            "whole escaped lane successor must skip all eighty follower keys"
        );
    }
    // Independent wire52, prefix21, scope515 and decoded139 tariff:
    // open557 + first seek557 + candidate inspection52 + decoded buffer139.
    assert_eq!(f::member(s, first).len(), 52);
    for adapter_bytes in [1_304, 1_305] {
        let tx = store.begin_apply(
            s.group(),
            s.recovery(),
            AdmissionLimits::default(),
            3,
            139,
            adapter_bytes,
        )?;
        let mut cursor =
            tx.claim_lane_cursor(s.tenant(), borrowed_query(b"job", ClaimStates::Any))?;
        assert!(matches!(
            (adapter_bytes, cursor.next()?),
            (1_304, ClaimLaneStep::BudgetEnd) | (1_305, ClaimLaneStep::Lane(_))
        ));
        assert_eq!(cursor.position().is_some(), adapter_bytes == 1_305);
        if adapter_bytes == 1_304 {
            assert!(matches!(cursor.next()?, ClaimLaneStep::BudgetEnd));
            assert!(cursor.position().is_none());
        }
    }
    Ok(())
}

fn selected_output_cohort(
    path: &Path,
    s: &RequestScope,
    before: &StoredWorkflowRecord,
    original: [BTreeMap<Vec<u8>, Vec<u8>>; 4],
    frame: &[u8],
) -> TestResult {
    let mut expected = original;
    let mut output_cap = f::limits();
    output_cap.base.codec = CodecLimits::new(AdmissionLimits::new(64, 1024, 1)?);
    let limited = f::phase(
        s,
        b"output",
        25,
        &f::query(b"job", b"worker", 1),
        (1, 1),
        [47; 32],
    )?;
    let rejected = b"FWCR\0\x01\x01\x0d";
    let metadata = f::metadata(
        s,
        b"output",
        1,
        25,
        [47; 32],
        RetainedStatus::ClaimRejected(wf_domain::claim::apply::ClaimRejection::DecisionBytes),
    )?;
    f::finish(
        &mut expected,
        s,
        &limited,
        f::log(1),
        (f::clock(25)?, f::clock(25)?),
        Some((&metadata, rejected)),
    )?;
    f::apply_check(
        path,
        s,
        &limited,
        f::log(1),
        output_cap,
        &expected,
        rejected,
    )?;
    let after = f::owner(
        s,
        b"p",
        WorkflowRecordData {
            state: b"running".to_vec(),
            version: 2,
            fence: Fence::new(1),
            updated_at: TimestampMillis::new(25),
            next_run_at: Some(TimestampMillis::new(30025)),
            run_state: Some(b"q".to_vec()),
            lease: Some(Lease {
                worker: b"worker".to_vec(),
                token: LeaseToken::new(b"worker:25:1".to_vec())?,
                deadline: TimestampMillis::new(30025),
            }),
            latest_kind: EventKind::Claimed,
            latest_version: 2,
            latest_fence: Fence::new(1),
            ..f::data(b"id", b"job", b"q", 0, Some(0))?
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
    let metadata = f::metadata(s, b"lease", 2, 25, [46; 32], RetainedStatus::Accepted)?;
    f::finish(
        &mut expected,
        s,
        frame,
        f::log(2),
        (f::clock(25)?, f::clock(25)?),
        Some((&metadata, &response)),
    )?;
    f::apply_check(path, s, frame, f::log(2), f::limits(), &expected, &response)?;
    Ok(())
}
