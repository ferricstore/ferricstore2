use super::*;
use std::sync::{
    Arc,
    atomic::{AtomicU8, Ordering},
};

#[test]
fn catalog_ack_old_capability_refuses_without_changing_any_table() -> TestResult {
    let dir = common::directory("catalog-ack-old")?;
    let path = dir.join("state");
    credit::seed(&path)?;
    let before = credit::cohort(&path)?;
    let frame = a::frame(
        b"ack",
        Request {
            expected_head: 1,
            up_to: 1,
        },
    )?;
    let mut store = common::open(&path)?;
    assert_eq!(
        apply_catalog_ack_transaction(
            &mut store,
            &common::scope()?,
            frame.as_bytes(),
            common::log(2),
            common::driver_limits()
        )
        .err(),
        Some(CreditTransactionError::State(
            StateTransactionError::UnsupportedPolicyProfile
        ))
    );
    drop(store);
    assert_eq!(credit::cohort(&path)?, before);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn catalog_ack_every_step_cut_aborts_deletions_metadata_and_original_outcome_together() -> TestResult
{
    let dir = common::directory("catalog-ack-step-cut")?;
    let path = dir.join("state");
    a::seed(&path)?;
    let before = credit::cohort(&path)?;
    let frame = a::frame(
        b"cut",
        Request {
            expected_head: 1,
            up_to: 3,
        },
    )?;
    let mut reached = false;
    for steps in 1..=120 {
        let mut selected = common::driver_limits();
        selected.maximum_read_steps = steps;
        let mut store = common::open(&path)?;
        let result = apply_catalog_ack_transaction(
            &mut store,
            &common::scope()?,
            frame.as_bytes(),
            common::log(3),
            selected,
        );
        match result {
            Err(error) => {
                assert_eq!(
                    error,
                    CreditTransactionError::State(StateTransactionError::Limit)
                );
                drop(store);
                assert_eq!(credit::cohort(&path)?, before);
            }
            Ok(RecordBatchTransactionOutcome::Retained { outcome, .. }) => {
                assert_eq!(
                    a::reply(&outcome)?,
                    Reply::Accepted {
                        meta: CreditCatalogMeta::restore(4, 3)?,
                        deleted: 3
                    }
                );
                reached = true;
                drop(store);
                break;
            }
            Ok(_) => return Err("unexpected ACK cut outcome".into()),
        }
    }
    assert!(
        reached,
        "must reach actual complete ACK within bounded steps"
    );
    assert_eq!(catalog::page(&path, 256)?.entries.len(), 0);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn catalog_ack_actual_uncertain_commit_fences_and_reopens_a_complete_old_or_new_cohort()
-> TestResult {
    let dir = common::directory("catalog-ack-uncertain")?;
    let path = dir.join("fault");
    let expected = dir.join("expected");
    a::seed(&path)?;
    a::seed(&expected)?;
    let before = credit::cohort(&path)?;
    assert_eq!(credit::cohort(&expected)?, before);
    let frame = a::frame(
        b"unknown",
        Request {
            expected_head: 1,
            up_to: 2,
        },
    )?;
    a::retained(
        &expected,
        b"unknown",
        Request {
            expected_head: 1,
            up_to: 2,
        },
        3,
    )?;
    let after = credit::cohort(&expected)?;
    assert_ne!(before, after);
    let fault = Arc::new(AtomicU8::new(0));
    let mut store = crate::faults::wrapped(&path, Arc::clone(&fault))?;
    fault.store(4, Ordering::SeqCst);
    let result = apply_catalog_ack_transaction(
        &mut store,
        &common::scope()?,
        frame.as_bytes(),
        common::log(3),
        common::driver_limits(),
    );
    assert!(
        matches!(
            result,
            Err(CreditTransactionError::State(
                StateTransactionError::Uncertain
            ))
        ),
        "{result:?}"
    );
    assert_eq!(fault.load(Ordering::SeqCst), 0);
    assert!(matches!(
        store.read_credit_catalog(&common::scope()?, 256, 10_000, 64 * common::MIB),
        Err(StateTransactionError::Unavailable)
    ));
    drop(store);
    drop(common::open(&path)?);
    let recovered = credit::cohort(&path)?;
    assert!(recovered == before || recovered == after);
    let replay = a::retained(
        &path,
        b"unknown",
        Request {
            expected_head: 1,
            up_to: 2,
        },
        if recovered == before { 3 } else { 4 },
    )?;
    assert_eq!(
        a::reply(&replay)?,
        Reply::Accepted {
            meta: CreditCatalogMeta::restore(3, 3)?,
            deleted: 2
        }
    );
    assert_eq!(catalog::page(&path, 256)?.entries.len(), 1);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}
