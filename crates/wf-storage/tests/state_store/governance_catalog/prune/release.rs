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
fn marker(path: &std::path::Path) -> TestResult<[u8; 4]> {
    let cohort = f::cohort(path)?;
    let manifest = cohort
        .get("wf_state_meta_v1")
        .and_then(|rows| rows.get(b"manifest".as_slice()))
        .ok_or("missing manifest")?;
    Ok(manifest[manifest.len() - 36..manifest.len() - 32].try_into()?)
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

#[test]
fn workflow_release_gc4_snapshot_preserves_capability_and_refuses_gc3_install() -> TestResult {
    use std::io::Cursor;
    use wf_ports::snapshot::{SnapshotLimits, StateSnapshotStore};
    use wf_storage::snapshot::RedbSnapshotStage;
    let mut source = release_fixture(4)?;
    source.register(b"register", b"member", 0)?;
    let mut wire = Vec::new();
    let metadata = source
        .store
        .capture_snapshot(&mut wire, SnapshotLimits::default())?;
    let scope = common::scope()?;
    let restored_path = source.dir.join("restored");
    let stage = RedbSnapshotStage::receive(
        &restored_path,
        scope.group().clone(),
        scope.recovery().clone(),
        &mut Cursor::new(&wire),
        StateStoreLimits::default(),
        SnapshotLimits::default(),
    )?;
    let mut restored = stage.into_store()?;
    assert_eq!(restored.governance_catalog_profile(), 4);
    let mut original = Vec::new();
    let observed = restored
        .current_snapshot(&mut original, SnapshotLimits::default())?
        .ok_or("snapshot absent")?;
    assert_eq!(observed.id(), metadata.id());
    assert_eq!(original, wire);
    drop(restored);
    let old = PruneFixture::new(Some(4), true)?;
    let old_path = old.path.clone();
    let old_dir = old.dir.clone();
    drop(old);
    let before = f::cohort(&old_path)?;
    let mut old_store = common::open(&old_path)?;
    let stage = RedbSnapshotStage::receive(
        &source.dir.join("stage"),
        scope.group().clone(),
        scope.recovery().clone(),
        &mut Cursor::new(&wire),
        StateStoreLimits::default(),
        SnapshotLimits::default(),
    )?;
    assert!(
        old_store
            .install_snapshot(stage, SnapshotLimits::default())
            .is_err()
    );
    drop(old_store);
    assert_eq!(f::cohort(&old_path)?, before);
    std::fs::remove_dir_all(old_dir)?;
    source.cleanup()
}

#[test]
fn workflow_release_rehashed_gc4_without_credit_catalog_dependencies_is_corrupt() -> TestResult {
    use redb::ReadableTable;
    use sha2::{Digest, Sha256};
    for count in [None, Some(4)] {
        let fixture = PruneFixture::new(count, true)?;
        let path = fixture.path.clone();
        let dir = fixture.dir.clone();
        drop(fixture);
        let db = redb::Database::open(&path)?;
        let tx = db.begin_write()?;
        {
            let mut table = tx.open_table(common::META)?;
            let mut bytes = table
                .get(b"manifest".as_slice())?
                .ok_or("manifest absent")?
                .value()
                .to_vec();
            let at = bytes.len() - 36;
            assert_eq!(&bytes[at..at + 4], b"GC\0\x03");
            bytes[at + 3] = 4;
            if count.is_some() {
                // Keep actual credit1 while falsely advertising GC4's required credit2.
                assert_eq!(&bytes[at - 10..at - 8], &2_u16.to_be_bytes());
                bytes[at - 10..at - 8].copy_from_slice(&1_u16.to_be_bytes());
            }
            let end = bytes.len() - 32;
            let hash = Sha256::digest(&bytes[..end]);
            bytes[end..].copy_from_slice(&hash);
            table.insert(b"manifest".as_slice(), bytes.as_slice())?;
        }
        tx.commit()?;
        drop(db);
        let before = f::cohort(&path)?;
        let scope = common::scope()?;
        assert_eq!(
            RedbStateStore::open(
                &path,
                scope.group().clone(),
                scope.recovery().clone(),
                StateStoreLimits::default()
            )
            .err(),
            Some(S::Corrupt)
        );
        assert_eq!(f::cohort(&path)?, before);
        std::fs::remove_dir_all(dir)?;
    }
    Ok(())
}
