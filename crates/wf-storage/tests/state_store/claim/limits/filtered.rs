//! Paid unrelated traversal and a real priority/time frontier under scan one.

use super::{TestResult, borrowed_query, f};
use std::path::Path;
use wf_domain::claim::apply::ClaimStates;
use wf_domain::history::EventKind;
use wf_domain::lease::{Lease, LeaseToken};
use wf_domain::workflow::admission::AdmissionLimits;
use wf_domain::workflow::indexed::OrderingMetadata;
use wf_domain::workflow::request::RetainedStatus;
use wf_domain::workflow::restoration::WorkflowRecordData;
use wf_ports::state::{
    ClaimDueStep, GroupApplyStore, GroupClaimDueCursor, GroupClaimTransaction,
    StateTransactionError,
};
use wf_storage::transaction::{ClaimTransactionError, apply_claim_transaction};
use wf_types::version::{Fence, TimestampMillis};

#[test]
fn filtered_physical_prefix_and_end_are_not_semantic_budget() -> TestResult {
    let dir = f::directory("claim-filtered")?;
    let path = dir.join("state");
    let s = f::scope()?;
    let mut store = f::create(&path, &s, &[b"p"], None)?;
    let rows = (0_u32..81)
        .map(|i| {
            f::owner(
                &s,
                b"p",
                f::data(
                    format!("foreign-{i:03}").as_bytes(),
                    b"other",
                    b"q",
                    0,
                    Some(0),
                )?,
                OrderingMetadata {
                    priority: 2,
                    state_enter_seq: u128::from(i),
                },
            )
        })
        .collect::<TestResult<Vec<_>>>()?;
    f::seed(&mut store, &s, &rows, 0, 0)?;
    {
        let tx = store.begin_apply(
            s.group(),
            s.recovery(),
            AdmissionLimits::default(),
            4,
            1_000_000,
            1_000_000,
        )?;
        let mut cursor = tx.claim_due_cursor(
            s.tenant(),
            borrowed_query(b"wanted", ClaimStates::Any),
            TimestampMillis::new(25),
        )?;
        loop {
            match cursor.next()? {
                ClaimDueStep::Row { .. } => {}
                ClaimDueStep::BudgetEnd => break,
                ClaimDueStep::RangeEnd => {
                    return Err("unpaid foreign traversal claimed actual end".into());
                }
            }
        }
        assert!(matches!(cursor.next()?, ClaimDueStep::BudgetEnd));
    }
    drop(store);
    let before = f::cohort(&path)?;
    let frame = f::phase(
        &s,
        b"filtered",
        25,
        &f::query(b"wanted", b"worker", 1),
        (1, 1),
        [43; 32],
    )?;
    let mut small = f::limits();
    small.base.maximum_read_steps = 12;
    let mut store = f::open(&path, &s)?;
    assert_eq!(
        apply_claim_transaction(&mut store, &s, &frame, f::log(1), small),
        Err(ClaimTransactionError::Transaction(
            StateTransactionError::Limit
        ))
    );
    drop(store);
    assert_eq!(f::cohort(&path)?, before);
    let mut expected = before;
    let response = f::receipt(&[], 0, 0, 0)?;
    let metadata = f::metadata(&s, b"filtered", 1, 25, [43; 32], RetainedStatus::Accepted)?;
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
    future_priority_frontier(&dir)?;
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

fn future_priority_frontier(dir: &Path) -> TestResult {
    let path = dir.join("future-priority");
    let s = f::scope()?;
    let mut store = f::create(&path, &s, &[b"p"], None)?;
    let mut owners = Vec::new();
    for (i, (id, due, priority)) in [
        (b"future-high".as_slice(), 1000, 2),
        (b"due-low".as_slice(), 0, 0),
    ]
    .into_iter()
    .enumerate()
    {
        owners.push(f::owner(
            &s,
            b"p",
            f::data(id, b"wanted", b"q", 0, Some(due))?,
            OrderingMetadata {
                priority,
                state_enter_seq: i as u128,
            },
        )?);
    }
    f::seed(&mut store, &s, &owners, 0, 0)?;
    drop(store);
    let before = f::cohort(&path)?;
    let mut expected = before.clone();
    let after = f::owner(
        &s,
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
            ..f::data(b"due-low", b"wanted", b"q", 0, Some(0))?
        },
        OrderingMetadata {
            priority: 0,
            state_enter_seq: 1,
        },
    )?;
    let frame = f::phase(
        &s,
        b"frontier",
        25,
        &f::query(b"wanted", b"worker", 1),
        (1, 1),
        [48; 32],
    )?;
    let response = f::receipt(std::slice::from_ref(&after), 0, 1, 0)?;
    f::replace_expected(
        &mut expected,
        &s,
        &after,
        &f::history(&after),
        &[f::due(&s, &owners[1])?],
        &[(f::due(&s, &after)?, vec![0])],
    )?;
    let metadata = f::metadata(&s, b"frontier", 1, 25, [48; 32], RetainedStatus::Accepted)?;
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
    for key in [f::point(&s, &owners[0]), f::due(&s, &owners[0])?] {
        assert_eq!(expected[1].get(&key), before[1].get(&key));
    }
    Ok(())
}
