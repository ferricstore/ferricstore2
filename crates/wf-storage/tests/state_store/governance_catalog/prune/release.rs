//! Actual release capability custody, inherited pruning and old-profile refusal.
use super::{P, PruneFixture, S, TestResult, common, f, fixtures::defaults, reply};
use redb::ReadableDatabase;
use wf_ports::state::GroupApplyStore;
use wf_storage::state::{RedbStateStore, StateStoreGenesis, StateStoreLimits};
fn release_fixture(count: u64) -> TestResult<PruneFixture> {
    let dir = common::directory("gc4-release")?;
    let path = dir.join("state");
    let scope = common::scope()?;
    let store = RedbStateStore::create_workflow_release_database(
        redb::Builder::new().create(&path)?,
        scope.group().clone(),
        scope.recovery().clone(),
        StateStoreGenesis {
            initial_time: common::clock(0)?,
            placements: &[],
            policies: &[],
        },
        defaults(),
        count,
        StateStoreLimits::default(),
    )?;
    Ok(PruneFixture { dir, path, store })
}
fn marker(path: &std::path::Path) -> TestResult<[u8;4]> {
    let cohort = f::cohort(path)?;
    let manifest = cohort.get("wf_state_meta_v1")
        .and_then(|rows| rows.get(b"manifest".as_slice())).ok_or("missing manifest")?;
    Ok(manifest[manifest.len()-36..manifest.len()-32].try_into()?)
}
#[test]
fn workflow_release_gc4_is_explicit_and_preserves_actual_credit_capability_on_reopen() -> TestResult
{
    let mut fixture = release_fixture(4)?;
    assert_eq!(fixture.store.governance_catalog_profile(), 4);
    let scope = common::scope()?;
    let tx = fixture.store.begin_apply(
        scope.group(),
        scope.recovery(),
        common::driver_limits().codec.admission(),
        10000,
        64 * common::MIB,
        32 * common::MIB,
    )?;
    tx.require_workflow_release()?;
    tx.require_catalog_progress()?;
    tx.require_catalog_prune()?;
    assert_eq!(tx.require_identified_credit_profile()?, (2, 4));
    drop(tx);
    let path = fixture.path.clone();
    let dir = fixture.dir.clone();
    drop(fixture);
    assert_eq!(&marker(&path)?, b"GC\0\x04");
    let store = common::open(&path)?;
    assert_eq!(store.governance_catalog_profile(), 4);
    drop(store);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}
#[test]
fn workflow_release_gc4_keeps_actual_revision_checked_pruning() -> TestResult {
    let mut fixture = release_fixture(4)?;
    let identity = fixture.register(b"register", b"member", 0)?;
    let frame = PruneFixture::frame(b"prune", b"member", identity)?;
    let outcome = fixture.apply(&frame, 1)?;
    assert_eq!(reply(&outcome)?, P::Removed);
    assert_eq!(fixture.page()?.page.members.len(), 0);
    let path = fixture.path.clone();
    let dir = fixture.dir.clone();
    drop(fixture);
    let store = common::open(&path)?;
    assert_eq!(store.governance_catalog_profile(), 4);
    drop(store);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}
#[test]
fn workflow_release_gc3_stays_closed_and_invalid_gc4_counts_write_no_tables() -> TestResult {
    let mut old = PruneFixture::new(Some(4), true)?;
    let scope = common::scope()?;
    let tx = old.store.begin_apply(
        scope.group(),
        scope.recovery(),
        common::driver_limits().codec.admission(),
        10000,
        64 * common::MIB,
        32 * common::MIB,
    )?;
    assert_eq!(
        tx.require_workflow_release(),
        Err(S::UnsupportedPolicyProfile)
    );
    drop(tx);
    let path = old.path.clone();
    let dir = old.dir.clone();
    drop(old);
    assert_eq!(&marker(&path)?, b"GC\0\x03");
    std::fs::remove_dir_all(dir)?;
    for count in [0, wf_domain::workflow::MAX_EXACT_MILLIS + 1] {
        let dir = common::directory("gc4-invalid")?;
        let path = dir.join("state");
        let result = RedbStateStore::create_workflow_release_database(
            redb::Builder::new().create(&path)?,
            scope.group().clone(),
            scope.recovery().clone(),
            StateStoreGenesis {
                initial_time: common::clock(0)?,
                placements: &[],
                policies: &[],
            },
            defaults(),
            count,
            StateStoreLimits::default(),
        );
        assert_eq!(result.err(), Some(S::InvalidChanges));
        let db = redb::Database::open(&path)?;
        assert_eq!(db.begin_read()?.list_tables()?.count(), 0);
        drop(db);
        std::fs::remove_dir_all(dir)?;
    }
    Ok(())
}
