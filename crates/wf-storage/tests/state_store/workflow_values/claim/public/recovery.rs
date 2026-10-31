use super::super::fixtures as leaf;
use super::{fixtures as p, *};
use std::sync::{
    Arc,
    atomic::{AtomicU8, Ordering},
};
use wf_ports::state::{GroupApplyStore, StateTransactionError as S};
use wf_storage::transaction::{ClaimTransactionError, apply_public_claim_transaction};

#[test]
fn unknown_public_commit_fences_and_keeps_the_entire_original_funded_cohort_or_none() -> TestResult
{
    let dir = f::directory("public-claim-uncertain")?;
    let path = dir.join("state");
    let reference = dir.join("reference");
    drop(p::filled(&path, true)?);
    let before = leaf::cohort(&path)?;
    let frame = p::frame(false, p::BATCH)?;
    let mut expected = p::filled(&reference, true)?;
    apply_public_claim_transaction(
        &mut expected,
        &f::scope()?,
        frame.as_bytes(),
        f::log(6),
        p::limits()?,
    )?;
    drop(expected);
    let complete = leaf::cohort(&reference)?;
    let fault = Arc::new(AtomicU8::new(0));
    let mut store = crate::faults::wrapped(&path, Arc::clone(&fault))?;
    fault.store(4, Ordering::SeqCst);
    assert_eq!(
        apply_public_claim_transaction(
            &mut store,
            &f::scope()?,
            frame.as_bytes(),
            f::log(6),
            p::limits()?
        )
        .err(),
        Some(ClaimTransactionError::Transaction(S::Uncertain))
    );
    assert_eq!(fault.load(Ordering::SeqCst), 0);
    let scope = f::scope()?;
    assert!(matches!(
        store.begin_apply(
            scope.group(),
            scope.recovery(),
            v::limits().record_batch.codec.admission(),
            10000,
            16 * 1_048_576,
            16 * 1_048_576
        ),
        Err(S::Unavailable)
    ));
    drop(store);
    let actual = leaf::cohort(&path)?;
    assert!(
        actual == before || actual == complete,
        "no partial funding, refund, workflow, history, value, outcome or control publication"
    );
    let mut restored = f::open(&path)?;
    match apply_public_claim_transaction(
        &mut restored,
        &scope,
        frame.as_bytes(),
        f::log(6),
        p::limits()?,
    )? {
        wf_storage::transaction::ClaimTransactionOutcome::Retained { control, outcome } => {
            assert_eq!(actual, before);
            assert_eq!(outcome.status(), RetainedStatus::Accepted);
            assert_eq!(control.position(), f::log(6).index());
        }
        wf_storage::transaction::ClaimTransactionOutcome::LogReplay(control) => {
            assert_eq!(actual, complete);
            assert_eq!(control.position(), f::log(6).index());
        }
        _ => return Err("same immutable original frame must complete or exact-log replay".into()),
    }
    drop(restored);
    assert_eq!(leaf::cohort(&path)?, complete);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}
