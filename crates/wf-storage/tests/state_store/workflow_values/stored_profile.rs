//! Explicit stored capability, real binary publication and complete snapshots.

use std::{collections::BTreeMap, io::Cursor, path::Path};

use redb::{Durability, ReadableDatabase, ReadableTable};
use sha2::{Digest, Sha256};
use wf_domain::workflow::request::RequestScope;
use wf_domain::workflow::{Create, Operation, Transition};
use wf_ports::blob::{OriginalValuePartition, OriginalValueRefKind, render_original_value_ref};
use wf_ports::consensus::PreparedOperation;
use wf_ports::snapshot::{SnapshotError, SnapshotLimits, StateSnapshotStore};
use wf_ports::state::StateTransactionError;
use wf_storage::snapshot::RedbSnapshotStage;
use wf_storage::state::{RedbStateStore, StateStoreGenesis, StateStoreLimits};
use wf_types::version::{Fence, HlcTimestamp, TimestampMillis};

use super::{TestResult, fixtures as f};

fn fresh(path: &Path) -> TestResult<Result<RedbStateStore, StateTransactionError>> {
    let scope = crate::fixtures::scope()?;
    let placements = crate::fixtures::placements()?;
    let database = redb::Builder::new().create(path)?;
    assert_eq!(database.begin_read()?.list_tables()?.count(), 0);
    Ok(RedbStateStore::create_stored_value_database(
        database,
        scope.group().clone(),
        scope.recovery().clone(),
        StateStoreGenesis {
            initial_time: crate::fixtures::clock(0)?,
            placements: &placements,
            policies: &[],
        },
        f::defaults(),
        StateStoreLimits::default(),
    ))
}

fn metadata(path: &Path) -> TestResult<BTreeMap<Vec<u8>, Vec<u8>>> {
    let database = redb::Builder::new().open(path)?;
    let read = database.begin_read()?;
    let table = read.open_table(crate::fixtures::META)?;
    table
        .iter()?
        .map(|row| {
            let (key, value) = row?;
            Ok((key.value().to_vec(), value.value().to_vec()))
        })
        .collect()
}

fn assert_profile(
    meta: &BTreeMap<Vec<u8>, Vec<u8>>,
    scope: &RequestScope,
    expected: u32,
) -> TestResult {
    assert_eq!(
        meta.keys().map(Vec::as_slice).collect::<Vec<_>>(),
        [
            b"manifest".as_slice(),
            b"membership",
            b"origin",
            b"snapshot"
        ]
    );
    let manifest = meta.get(b"manifest".as_slice()).ok_or("manifest")?;
    assert!(manifest.starts_with(b"FWSS\0\x07"));
    assert_eq!(
        manifest.len(),
        208 + scope.group().as_bytes().len() + scope.recovery().as_bytes().len()
    );
    let at = 108 + scope.group().as_bytes().len() + scope.recovery().as_bytes().len();
    assert_eq!(&manifest[at..at + 4], expected.to_be_bytes());
    assert_eq!(&manifest[at + 4..at + 8], 1_u32.to_be_bytes());
    let end = manifest.len() - 32;
    assert_eq!(
        &manifest[end..],
        Sha256::digest(&manifest[..end]).as_slice()
    );
    Ok(())
}

fn reference() -> TestResult<Vec<u8>> {
    render_original_value_ref(
        b"i",
        OriginalValueRefKind::Payload,
        1,
        OriginalValuePartition::Explicit(b"p"),
        65_535,
    )
    .map_err(|error| format!("reference: {error:?}").into())
}

fn publish(store: &mut RedbStateStore, scope: &RequestScope, bytes: &[u8]) -> TestResult {
    let batch = f::batch(
        5,
        b"profile-publish",
        100,
        PreparedOperation::Batch(vec![f::item(
            b"i",
            100,
            Operation::Create(Create {
                workflow_type: b"r".to_vec(),
                state: b"q".to_vec(),
                run_at: None,
                payload: Some(bytes.to_vec()),
            }),
        )?]),
        vec![f::input(0, 0)],
        vec![f::witness(b"r", None)?],
    )?;
    f::apply_value(store, scope, &batch, 0)?;
    Ok(())
}

fn assert_read(store: &RedbStateStore, scope: &RequestScope, expected: &[u8]) -> TestResult {
    let reference = reference()?;
    let mut clock = f::ValueReadClock {
        hlc: HlcTimestamp::new(201, 0)?,
        wall: TimestampMillis::new(201),
        calls: 0,
        failure: None,
    };
    let read =
        store.read_workflow_value_batch(&mut clock, scope, &[&reference], f::read_limits())?;
    f::assert_read(&read, &[expected], &[Some(0)])?;
    assert_eq!(clock.calls, 1);
    Ok(())
}

fn open(path: &Path, scope: &RequestScope) -> Result<RedbStateStore, StateTransactionError> {
    RedbStateStore::open(
        path,
        scope.group().clone(),
        scope.recovery().clone(),
        StateStoreLimits::default(),
    )
}

#[test]
fn stored_value_profile_genesis_is_explicit_and_survives_reopen() -> TestResult {
    let directory = crate::fixtures::directory("stored-profile-genesis")?;
    let scope = crate::fixtures::scope()?;
    let old = f::fresh(&directory.join("binary"))?;
    assert!(old.is_ok(), "{:?}", old.as_ref().err());
    drop(old?);
    let created = fresh(&directory.join("stored"))?;
    assert!(created.is_ok(), "{:?}", created.as_ref().err());
    drop(created?);
    let old = metadata(&directory.join("binary"))?;
    let stored = metadata(&directory.join("stored"))?;
    assert_profile(&old, &scope, 1)?;
    assert_profile(&stored, &scope, 2)?;
    assert_eq!(
        old.get(b"origin".as_slice()),
        stored.get(b"origin".as_slice())
    );
    assert_eq!(
        crate::fixtures::raw_rows(&directory.join("binary"))?,
        crate::fixtures::raw_rows(&directory.join("stored"))?
    );
    drop(open(&directory.join("stored"), &scope)?);
    assert_eq!(metadata(&directory.join("stored"))?, stored);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

#[test]
fn stored_value_profile_preserves_binary_publication_and_policy_updates() -> TestResult {
    let directory = crate::fixtures::directory("stored-profile-policy")?;
    let scope = crate::fixtures::scope()?;
    let path = directory.join("stored");
    let created = fresh(&path)?;
    assert!(created.is_ok(), "{:?}", created.as_ref().err());
    let mut store = created?;
    publish(&mut store, &scope, b"FSV2\x02opaque")?;
    let policy = f::original_policy(1)?;
    f::install_original(&mut store, &scope, policy.encoded(), None, 1, 150)?;
    let transition = f::batch(
        6,
        b"carry",
        200,
        PreparedOperation::Batch(vec![f::item(
            b"i",
            200,
            Operation::Transition(Transition {
                from: b"q".to_vec(),
                to: b"waiting".to_vec(),
                fence: Fence::new(0),
                lease_token: None,
                run_at: None,
                payload: None,
            }),
        )?]),
        vec![f::input(0, 0)],
        vec![f::witness(b"r", Some(&policy))?],
    )?;
    f::apply_value(&mut store, &scope, &transition, 2)?;
    assert_read(&store, &scope, b"FSV2\x01FSV2\x02opaque")?;
    drop(store);
    assert_profile(&metadata(&path)?, &scope, 2)?;
    let rows = crate::fixtures::raw_rows(&path)?;
    let aliases = rows
        .iter()
        .filter(|(key, _)| key.get(6) == Some(&9))
        .collect::<Vec<_>>();
    assert_eq!(aliases.len(), 1);
    assert!(aliases[0].1.starts_with(b"FWVA\0\x01"));
    let reopened = open(&path, &scope)?;
    assert_read(&reopened, &scope, b"FSV2\x01FSV2\x02opaque")?;
    drop(reopened);
    assert_eq!(crate::fixtures::raw_rows(&path)?, rows);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

#[test]
fn stored_value_profile_snapshot_roundtrip_preserves_capability() -> TestResult {
    let directory = crate::fixtures::directory("stored-profile-snapshot")?;
    let scope = crate::fixtures::scope()?;
    let path = directory.join("source");
    let created = fresh(&path)?;
    assert!(created.is_ok(), "{:?}", created.as_ref().err());
    let mut source = created?;
    publish(&mut source, &scope, b"FSV2\x02opaque")?;
    let mut wire = Vec::new();
    let captured = source.capture_snapshot(&mut wire, SnapshotLimits::default())?;
    assert!(wire.starts_with(b"FWSP\0\x04"));
    source.pin_snapshot(*captured.id(), SnapshotLimits::default())?;
    drop(source);
    let rows = crate::fixtures::raw_rows(&path)?;
    let source_meta = metadata(&path)?;
    assert_profile(&source_meta, &scope, 2)?;
    assert_eq!(captured.row_count(), rows.len() + 3);
    let stage = RedbSnapshotStage::receive(
        &directory.join("stage"),
        scope.group().clone(),
        scope.recovery().clone(),
        &mut Cursor::new(&wire),
        StateStoreLimits::default(),
        SnapshotLimits::default(),
    )?;
    let target_path = directory.join("target");
    let mut target = fresh(&target_path)??;
    assert_eq!(
        target
            .install_snapshot(stage, SnapshotLimits::default())?
            .id(),
        captured.id()
    );
    assert_read(&target, &scope, b"FSV2\x01FSV2\x02opaque")?;
    let mut installed = Vec::new();
    assert_eq!(
        target
            .current_snapshot(&mut installed, SnapshotLimits::default())?
            .ok_or("artifact")?
            .id(),
        captured.id()
    );
    assert_eq!(installed, wire);
    drop(target);
    assert_eq!(crate::fixtures::raw_rows(&target_path)?, rows);
    let target_meta = metadata(&target_path)?;
    for name in [b"manifest".as_slice(), b"membership", b"origin"] {
        assert_eq!(target_meta.get(name), source_meta.get(name));
    }
    drop(open(&target_path, &scope)?);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

fn rewrite(path: &Path, key: &[u8], bytes: &[u8], metadata: bool) -> TestResult {
    let database = redb::Builder::new().open(path)?;
    let mut write = database.begin_write()?;
    write.set_durability(Durability::Immediate)?;
    let definition = if metadata {
        crate::fixtures::META
    } else {
        crate::fixtures::ROWS
    };
    let mut table = write.open_table(definition)?;
    assert!(table.insert(key, bytes)?.is_some());
    drop(table);
    write.commit()?;
    Ok(())
}

fn reseal(bytes: &mut [u8]) -> TestResult {
    let end = bytes.len().checked_sub(32).ok_or("checksum")?;
    let checksum = Sha256::digest(&bytes[..end]);
    bytes[end..].copy_from_slice(&checksum);
    Ok(())
}

#[test]
fn stored_value_profile_rejects_cross_profile_install_and_unknown_versions() -> TestResult {
    let directory = crate::fixtures::directory("stored-profile-refusals")?;
    let scope = crate::fixtures::scope()?;
    let path = directory.join("source");
    let created = fresh(&path)?;
    assert!(created.is_ok(), "{:?}", created.as_ref().err());
    let mut source = created?;
    publish(&mut source, &scope, b"abc")?;
    let mut wire = Vec::new();
    source.capture_snapshot(&mut wire, SnapshotLimits::default())?;
    let target_path = directory.join("binary");
    let mut target = f::fresh(&target_path)??;
    let stage = RedbSnapshotStage::receive(
        &directory.join("cross-stage"),
        scope.group().clone(),
        scope.recovery().clone(),
        &mut Cursor::new(&wire),
        StateStoreLimits::default(),
        SnapshotLimits::default(),
    )?;
    assert!(matches!(
        target.install_snapshot(stage, SnapshotLimits::default()),
        Err(SnapshotError::GenesisMismatch)
    ));
    publish(&mut target, &scope, b"abc")?;
    drop((target, source));
    assert_profile(&metadata(&target_path)?, &scope, 1)?;
    let before_meta = metadata(&target_path)?;
    let mut rows = crate::fixtures::raw_rows(&target_path)?;
    let key = rows
        .keys()
        .find(|key| key.get(6) == Some(&9))
        .ok_or("alias")?
        .clone();
    let alias = rows.get_mut(&key).ok_or("alias value")?;
    assert!(alias.starts_with(b"FWVA\0\x01"));
    alias[4..6].copy_from_slice(&2_u16.to_be_bytes());
    reseal(alias)?;
    rewrite(&target_path, &key, alias, false)?;
    assert!(matches!(
        open(&target_path, &scope),
        Err(StateTransactionError::Corrupt)
    ));
    assert_eq!(crate::fixtures::raw_rows(&target_path)?, rows);
    assert_eq!(metadata(&target_path)?, before_meta);
    let mut manifest = metadata(&path)?
        .remove(b"manifest".as_slice())
        .ok_or("manifest")?;
    let at = 108 + scope.group().as_bytes().len() + scope.recovery().as_bytes().len();
    manifest[at..at + 4].copy_from_slice(&3_u32.to_be_bytes());
    reseal(&mut manifest)?;
    rewrite(&path, b"manifest", &manifest, true)?;
    assert!(matches!(
        open(&path, &scope),
        Err(StateTransactionError::UnsupportedPolicyProfile)
    ));
    std::fs::remove_dir_all(directory)?;
    Ok(())
}
