use super::*;
use std::sync::{
    Arc,
    atomic::{AtomicU8, Ordering},
};

#[test]
fn governance_catalog_old_capability_refuses_without_any_table_change() -> TestResult {
    let dir = common::directory("governance-old")?;
    let path = dir.join("old");
    drop(common::create(&path)?);
    let before = f::cohort(&path)?;
    let frame = f::frame(b"old", A::Register, &[b"a"])?;
    let mut store = common::open(&path)?;
    assert_eq!(
        apply_catalog_membership_transaction(
            &mut store,
            &common::scope()?,
            frame.as_bytes(),
            common::log(0),
            common::driver_limits()
        )
        .err(),
        Some(E::State(S::UnsupportedPolicyProfile))
    );
    drop(store);
    assert_eq!(f::cohort(&path)?, before);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn governance_catalog_every_step_cut_aborts_members_outcome_and_control_together() -> TestResult {
    let dir = common::directory("governance-cut")?;
    let path = dir.join("state");
    drop(f::create(&path)?);
    let before = f::cohort(&path)?;
    let frame = f::frame(b"cut", A::Register, &[b"a", b"b", b"c"])?;
    let mut reached = false;
    for steps in 1..=160 {
        let mut limits = common::driver_limits();
        limits.maximum_read_steps = steps;
        let mut store = common::open(&path)?;
        let result = apply_catalog_membership_transaction(
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
                assert_eq!(outcome.response(), b"FWGR\0\x01");
                reached = true;
                break;
            }
            other => return Err(format!("unexpected step cut: {other:?}").into()),
        }
    }
    assert!(reached);
    assert_eq!(
        f::page(&path, None, 256)?.members,
        vec![b"a".to_vec(), b"b".to_vec(), b"c".to_vec()]
    );
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn governance_catalog_actual_uncertain_commit_fences_and_reopens_complete_old_or_new() -> TestResult
{
    let dir = common::directory("governance-uncertain")?;
    let path = dir.join("fault");
    let expected = dir.join("expected");
    drop(f::create(&path)?);
    drop(f::create(&expected)?);
    let before = f::cohort(&path)?;
    assert_eq!(f::cohort(&expected)?, before);
    let frame = f::frame(b"unknown", A::Register, &[b"a", b"b"])?;
    f::retained(&expected, &frame, 0)?;
    let after = f::cohort(&expected)?;
    assert_ne!(before, after);
    let fault = Arc::new(AtomicU8::new(0));
    let mut store = crate::faults::wrapped(&path, Arc::clone(&fault))?;
    fault.store(4, Ordering::SeqCst);
    assert_eq!(
        apply_catalog_membership_transaction(
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
            .read_governance_catalog(f::read(&common::scope()?, None, 256), 1000, common::MIB)
            .err(),
        Some(S::Unavailable)
    );
    drop(store);
    drop(common::open(&path)?);
    let reopened = f::cohort(&path)?;
    assert!(reopened == before || reopened == after);
    f::retained(&path, &frame, u64::from(reopened != before))?;
    assert_eq!(
        f::page(&path, None, 256)?.members,
        vec![b"a".to_vec(), b"b".to_vec()]
    );
    std::fs::remove_dir_all(dir)?;
    Ok(())
}
