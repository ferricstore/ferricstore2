//! Actual GC3 profile, member revisions and conditional retained deletion.
use super::{A, Outcome, S, TestResult, common, f};
use wf_domain::{
    governance::catalog::{CatalogPruneReply as P, CatalogPruneRequest as C},
    workflow::control::EntryIdentity,
};
use wf_ports::{
    log::catalog_prune_retained_summary,
    state::{
        GovernanceCatalogPruneReadPort, GovernanceCatalogRead, GovernanceCatalogReadLimits as L,
        GroupApplyStore,
    },
};
mod faults;
mod fixtures;
use fixtures::PruneFixture;
fn reply(outcome: &wf_domain::workflow::request::RetainedOutcome) -> TestResult<P> {
    Ok(catalog_prune_retained_summary(
        outcome.intent(),
        outcome.response_version(),
        outcome.response(),
        common::driver_limits().codec.admission(),
    )?)
}

#[test]
fn catalog_prune_gc3_genesis_reopen_and_empty_capability_are_explicit() -> TestResult {
    for count in [None, Some(4)] {
        let mut fixture = PruneFixture::new(count, true)?;
        assert!(fixture.page()?.page.members.is_empty());
        assert_eq!(fixture.page()?.revision, None);
        let scope = common::scope()?;
        let tx = fixture.store.begin_apply(
            scope.group(),
            scope.recovery(),
            common::driver_limits().codec.admission(),
            10000,
            64 * common::MIB,
            32 * common::MIB,
        )?;
        tx.require_catalog_prune()?;
        assert_eq!(tx.governance_catalog_profile()?, 3);
        if count.is_some() {
            assert_eq!(tx.require_identified_credit_profile()?, (2, 4));
        }
        drop(tx);
        let path = fixture.path.clone();
        let dir = fixture.dir.clone();
        drop(fixture);
        let store = common::open(&path)?;
        assert_eq!(store.governance_catalog_profile(), 3);
        drop(store);
        std::fs::remove_dir_all(dir)?;
    }
    Ok(())
}
#[test]
fn catalog_prune_gc2_refuses_candidate_observation_without_any_table_change() -> TestResult {
    let fixture = PruneFixture::new(None, false)?;
    let path = fixture.path.clone();
    let dir = fixture.dir.clone();
    drop(fixture);
    let before = f::cohort(&path)?;
    let store = common::open(&path)?;
    assert_eq!(
        store
            .governance_catalog_prune_page(
                GovernanceCatalogRead {
                    scope: &common::scope()?,
                    catalog: b"limit",
                    cursor: None,
                    limit: 1
                },
                L::default()
            )
            .err(),
        Some(S::UnsupportedPolicyProfile)
    );
    drop(store);
    assert_eq!(f::cohort(&path)?, before);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}
#[test]
fn catalog_prune_matches_member_revision_and_replay_never_removes_reregistered_member() -> TestResult
{
    let mut fixture = PruneFixture::new(None, true)?;
    let first = fixture.register(b"add-a", b"a", 0)?;
    assert_eq!(fixture.page()?.revision, Some(first));
    let frame = fixture.frame(b"prune-a", b"a", first)?;
    let original = fixture.apply(&frame, 1)?;
    assert_eq!(reply(&original)?, P::Removed);
    assert!(fixture.page()?.page.members.is_empty());
    let next = fixture.register(b"new-a", b"a", 2)?;
    assert_ne!(next, first);
    assert_eq!(fixture.apply(&frame, 3)?, original);
    assert_eq!(fixture.page()?.revision, Some(next));
    let stale = fixture.frame(b"stale-a", b"a", first)?;
    assert_eq!(reply(&*fixture.apply(&stale, 4)?)?, P::Changed);
    assert_eq!(fixture.page()?.revision, Some(next));
    fixture.register(b"add-b", b"b", 5)?;
    let exact = fixture.frame(b"exact-a", b"a", next)?;
    assert_eq!(reply(&*fixture.apply(&exact, 6)?)?, P::Removed);
    let missing = fixture.frame(b"missing-a", b"a", next)?;
    assert_eq!(reply(&*fixture.apply(&missing, 7)?)?, P::Missing);
    assert_eq!(fixture.page()?.page.members, vec![b"b".to_vec()]);
    fixture.cleanup()
}

#[test]
fn catalog_prune_duplicate_registration_rotates_revision_and_unrelated_member_does_not()
-> TestResult {
    let mut fixture = PruneFixture::new(None, true)?;
    let original = fixture.register(b"a-first", b"a", 0)?;
    fixture.register(b"b-first", b"b", 1)?;
    assert_eq!(fixture.page()?.revision, Some(original));
    let repeated = fixture.register(b"a-again", b"a", 2)?;
    assert_ne!(repeated, original);
    assert_eq!(fixture.page()?.revision, Some(repeated));
    let old = fixture.frame(b"old-a", b"a", original)?;
    assert_eq!(reply(&*fixture.apply(&old, 3)?)?, P::Changed);
    assert_eq!(fixture.page()?.revision, Some(repeated));
    fixture.cleanup()
}
