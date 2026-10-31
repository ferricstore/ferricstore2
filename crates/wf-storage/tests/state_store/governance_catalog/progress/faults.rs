//! Independent complete cohorts, retained priority and real disk uncertainty.
use super::*;
use std::sync::{
    Arc,
    atomic::{AtomicU8, Ordering},
};
use wf_storage::transaction::CatalogProgressTransactionError as E;

#[test]
fn governance_progress_every_step_cut_preserves_the_entire_previous_cohort() -> TestResult {
    let dir = common::directory("progress-cut")?;
    let path = dir.join("state");
    drop(p::create(&path, None)?);
    let before = f::cohort(&path)?;
    let frame = p::frame(b"cut", Some(b"a\0z"))?;
    let mut reached = false;
    for steps in 1..=160 {
        let mut limits = common::driver_limits();
        limits.maximum_read_steps = steps;
        let mut store = common::open(&path)?;
        let result = apply_catalog_progress_transaction(
            &mut store,
            &common::scope()?,
            frame.as_bytes(),
            common::log(0),
            limits,
        );
        drop(store);
        match result {
            Err(E::State(S::Limit)) => assert_eq!(f::cohort(&path)?, before, "cut{steps}"),
            Ok(Outcome::Retained { outcome, .. }) => {
                assert_eq!(outcome.response(), b"FWPR\0\x01");
                reached = true;
                break;
            }
            other => return Err(format!("unexpected progress cut {other:?}").into()),
        }
    }
    assert!(reached);
    let store = common::open(&path)?;
    assert_eq!(p::read(&store)?, P::Cursor(b"a\0z".to_vec()));
    drop(store);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn governance_progress_actual_commit_uncertainty_fences_then_recovers_complete_old_or_new()
-> TestResult {
    let dir = common::directory("progress-uncertain")?;
    let path = dir.join("state");
    let expected = dir.join("expected");
    drop(p::create(&path, None)?);
    let mut target = p::create(&expected, None)?;
    let before = f::cohort(&path)?;
    let frame = p::frame(b"unknown", Some(b"cursor"))?;
    p::apply(&mut target, &frame, 0)?;
    drop(target);
    let after = f::cohort(&expected)?;
    assert_ne!(before, after);
    let fault = Arc::new(AtomicU8::new(0));
    let mut store = crate::faults::wrapped(&path, Arc::clone(&fault))?;
    fault.store(4, Ordering::SeqCst);
    assert_eq!(
        apply_catalog_progress_transaction(
            &mut store,
            &common::scope()?,
            frame.as_bytes(),
            common::log(0),
            common::driver_limits()
        )
        .err(),
        Some(E::State(S::Uncertain))
    );
    assert_eq!(fault.load(Ordering::SeqCst), 0);
    assert_eq!(
        store
            .governance_catalog_progress(&common::scope()?, b"limit", L::default())
            .err(),
        Some(S::Unavailable)
    );
    drop(store);
    drop(common::open(&path)?);
    let recovered = f::cohort(&path)?;
    assert!(recovered == before || recovered == after);
    let mut store = common::open(&path)?;
    p::apply(&mut store, &frame, u64::from(recovered != before))?;
    assert_eq!(p::read(&store)?, P::Cursor(b"cursor".to_vec()));
    drop(store);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn governance_progress_invalid_hint_stays_unchanged_until_a_committed_clear() -> TestResult {
    let dir = common::directory("progress-invalid")?;
    let path = dir.join("state");
    let mut store = p::create(&path, None)?;
    p::apply(&mut store, &p::frame(b"set", Some(b"cursor"))?, 0)?;
    drop(store);
    p::damage_hint(&path)?;
    let before = f::cohort(&path)?;
    let mut store = common::open(&path)?;
    assert_eq!(p::read(&store)?, P::Invalid);
    drop(store);
    assert_eq!(f::cohort(&path)?, before);
    store = common::open(&path)?;
    p::apply(&mut store, &p::frame(b"reset", None)?, 1)?;
    assert_eq!(p::read(&store)?, P::Missing);
    drop(store);
    assert_ne!(f::cohort(&path)?, before);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn governance_progress_receipt_only_clear_still_refuses_a_healthy_gc1_downgrade() -> TestResult {
    let dir = common::directory("progress-downgrade")?;
    let path = dir.join("state");
    let mut store = p::create(&path, None)?;
    p::apply(&mut store, &p::frame(b"clear", None)?, 0)?;
    assert_eq!(p::read(&store)?, P::Missing);
    drop(store);
    p::downgrade(&path)?;
    let error = common::open(&path)
        .err()
        .ok_or("downgraded progress receipt was accepted")?;
    assert_eq!(error.downcast_ref::<S>(), Some(&S::Corrupt));
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn governance_progress_conflict_expiry_and_exact_log_replay_do_not_reapply_the_cursor() -> TestResult
{
    let dir = common::directory("progress-gate")?;
    let path = dir.join("state");
    let mut store = p::create(&path, None)?;
    let original = p::frame(b"id", Some(b"a"))?;
    let Outcome::Retained { outcome, .. } = p::apply(&mut store, &original, 0)? else {
        return Err("missing progress result".into());
    };
    let conflict = p::frame(b"id", Some(b"b"))?;
    assert!(matches!(
        p::apply(&mut store, &conflict, 1)?,
        Outcome::IntentConflict(_)
    ));
    assert_eq!(p::read(&store)?, P::Cursor(b"a".to_vec()));
    let expired = wf_ports::log::encode_catalog_progress_request(
        &common::scope()?,
        outcome.request(),
        common::clock(outcome.expires_at().get())?,
        C {
            catalog: b"limit",
            cursor: Some(b"a"),
        },
        common::driver_limits().codec,
        16 * common::MIB,
    )?;
    assert!(matches!(
        p::apply(&mut store, &expired, 2)?,
        Outcome::Expired(_)
    ));
    assert_eq!(p::read(&store)?, P::Cursor(b"a".to_vec()));
    drop(store);
    let before = f::cohort(&path)?;
    let mut store = common::open(&path)?;
    assert!(matches!(
        p::apply(&mut store, &expired, 2)?,
        Outcome::LogReplay(_)
    ));
    drop(store);
    assert_eq!(f::cohort(&path)?, before);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}
