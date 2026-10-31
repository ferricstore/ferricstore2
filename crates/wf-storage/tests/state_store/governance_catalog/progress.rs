//! Actual GC2 genesis, same-guard observations and consumed retained progress.
use super::{Outcome, S, TestResult, common, f};
use wf_domain::governance::catalog::CatalogProgressChange as C;
use wf_ports::state::{
    GovernanceCatalogProgress as P, GovernanceCatalogProgressPort,
    GovernanceCatalogReadLimits as L, GroupApplyStore,
};
use wf_storage::transaction::apply_catalog_progress_transaction;
mod fixtures;
use fixtures as p;

#[test]
fn governance_progress_explicit_genesis_and_reopen_preserve_empty_target_and_combined_profiles()
-> TestResult {
    for count in [None, Some(4)] {
        let dir = common::directory("progress-genesis")?;
        let path = dir.join("state");
        let mut store = p::create(&path, count)?;
        assert_eq!(p::read(&store)?, P::Missing);
        let scope = common::scope()?;
        let tx = store.begin_apply(
            scope.group(),
            scope.recovery(),
            common::driver_limits().codec.admission(),
            10000,
            64 * common::MIB,
            32 * common::MIB,
        )?;
        tx.require_catalog_progress()?;
        assert_eq!(tx.governance_catalog_profile()?, 2);
        if count.is_some() {
            assert_eq!(tx.require_identified_credit_profile()?, (2, 4));
        }
        drop(tx);
        drop(store);
        let store = common::open(&path)?;
        assert_eq!(p::read(&store)?, P::Missing);
        drop(store);
        std::fs::remove_dir_all(dir)?;
    }
    Ok(())
}

#[test]
fn governance_progress_old_capability_refuses_without_any_table_or_control_change() -> TestResult {
    let dir = common::directory("progress-old")?;
    let path = dir.join("state");
    drop(f::create(&path)?);
    let before = f::cohort(&path)?;
    let store = common::open(&path)?;
    assert_eq!(
        store.governance_catalog_progress(&common::scope()?, b"limit", L::default()),
        Err(S::UnsupportedPolicyProfile)
    );
    drop(store);
    assert_eq!(f::cohort(&path)?, before);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn governance_progress_set_clear_and_original_retry_preserve_newer_progress() -> TestResult {
    let dir = common::directory("progress-apply")?;
    let path = dir.join("state");
    let mut store = p::create(&path, None)?;
    assert_eq!(p::read(&store)?, P::Missing);
    let first = p::frame(b"first", Some(b"a\0z"))?;
    let Outcome::Retained {
        outcome: original, ..
    } = p::apply(&mut store, &first, 0)?
    else {
        return Err("missing original progress reply".into());
    };
    assert_eq!(original.response_version(), 13);
    assert_eq!(original.response(), b"FWPR\0\x01");
    assert_eq!(p::read(&store)?, P::Cursor(b"a\0z".to_vec()));
    let next = p::frame(b"next", Some(b"later"))?;
    p::apply(&mut store, &next, 1)?;
    let Outcome::Retained {
        outcome: replayed, ..
    } = p::apply(&mut store, &first, 2)?
    else {
        return Err("missing replayed progress reply".into());
    };
    assert_eq!(replayed, original);
    assert_eq!(p::read(&store)?, P::Cursor(b"later".to_vec()));
    let clear = p::frame(b"clear", None)?;
    p::apply(&mut store, &clear, 3)?;
    assert_eq!(p::read(&store)?, P::Missing);
    drop(store);
    let store = common::open(&path)?;
    assert_eq!(p::read(&store)?, P::Missing);
    drop(store);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}
