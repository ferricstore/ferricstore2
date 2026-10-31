//! Actual catalog commands through historical audit, replay and snapshot custody.

use super::{TestResult, caps, f};
use redb::{ReadableDatabase, ReadableTable, TableDefinition, TableHandle};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, path::Path};
use wf_domain::{
    governance::catalog::{CatalogMembershipAction as A, CatalogMembershipChange as C},
    workflow::{active::LifecycleDefaults, control::PreparedDigest},
};
use wf_node::recovery::{
    CommittedRecoveryError as E, CommittedRecoveryLimits, purge_snapshot, recover_committed,
    recover_snapshotted, restore_received_snapshot,
};
use wf_ports::{
    consensus::WorkflowValueDefaults,
    log::{
        CodecError, PreparedBytes, RaftLogLimits, RaftLogStore, encode_catalog_membership_request,
    },
    snapshot::{SnapshotLimits, StateSnapshotStore},
    state::{GovernanceCatalogRead, GroupApplyStore, GroupApplyTransaction},
};
use wf_storage::{
    log::RedbLogStore,
    snapshot::RedbSnapshotStage,
    state::{RedbStateStore, StateStoreGenesis, StateStoreLimits},
    transaction::apply_catalog_membership_transaction,
};
use wf_types::identity::RequestId;

type CatalogCohort = BTreeMap<String, BTreeMap<Vec<u8>, Vec<u8>>>;

fn selected() -> TestResult<CommittedRecoveryLimits> {
    Ok(caps()?.with_governance_catalogs())
}
fn log(dir: &Path) -> TestResult<RedbLogStore> {
    let scope = f::scope()?;
    Ok(RedbLogStore::create(
        &dir.join("log.redb"),
        scope.group().clone(),
        scope.recovery().clone(),
        RaftLogLimits::default(),
    )?)
}
fn pair(dir: &Path) -> TestResult<(RedbLogStore, RedbStateStore)> {
    let scope = f::scope()?;
    let lifecycle = LifecycleDefaults {
        ttl_ms: 7000,
        history_hot_max_events: 2,
        history_max_events: 10,
        max_history_hot_max_events: 100,
        max_history_max_events: 1000,
    };
    let mut hash = Sha256::new();
    hash.update(b"wf-workflow-value-defaults-v1");
    hash.update(lifecycle.ttl_ms.to_be_bytes());
    hash.update(lifecycle.history_hot_max_events.to_be_bytes());
    hash.update(lifecycle.history_max_events.to_be_bytes());
    hash.update(lifecycle.max_history_hot_max_events.to_be_bytes());
    hash.update(lifecycle.max_history_max_events.to_be_bytes());
    let defaults = WorkflowValueDefaults {
        configuration_digest: PreparedDigest::new(hash.finalize().into()),
        lifecycle,
    };
    let state = RedbStateStore::create_governance_catalog_database(
        redb::Builder::new().create(dir.join("state.redb"))?,
        scope.group().clone(),
        scope.recovery().clone(),
        StateStoreGenesis {
            initial_time: f::clock(0, 0, 0)?,
            placements: &[],
            policies: &[],
        },
        defaults,
        StateStoreLimits::default(),
    )?;
    Ok((log(dir)?, state))
}
fn frame(id: &[u8], time: u64, action: A, members: &[&[u8]]) -> TestResult<PreparedBytes> {
    Ok(encode_catalog_membership_request(
        &f::scope()?,
        &RequestId::new(id.to_vec())?,
        f::clock(time, 0, time)?,
        C {
            catalog: b"limit",
            action,
            members,
        },
        f::driver_limits().codec,
        16 * f::MIB,
    )?)
}
fn frames() -> TestResult<Vec<PreparedBytes>> {
    Ok(vec![
        frame(b"add", 1000, A::Register, &[b"a", b"b"])?,
        frame(b"later", 1001, A::Register, &[b"c"])?,
        frame(b"remove", 1002, A::Unregister, &[b"a", b"b"])?,
        frame(b"add", 1000, A::Register, &[b"a", b"b"])?,
        frame(b"later", 1004, A::Register, &[b"d"])?,
        frame(b"final", 1005, A::Register, &[b"e"])?,
    ])
}
fn append(log: &mut RedbLogStore, frames: &[PreparedBytes], bad: bool) -> TestResult {
    for (index, frame) in frames.iter().enumerate() {
        let mut bytes = frame.as_bytes().to_vec();
        if bad && index == 2 {
            bytes.push(0);
        }
        f::append(log, u64::try_from(index)?, &bytes)?;
    }
    f::append(log, 6, b"opaque uncommitted catalog suffix")?;
    log.save_committed(f::at(5))?;
    Ok(())
}
fn apply(state: &mut RedbStateStore, frames: &[PreparedBytes], end: usize) -> TestResult {
    for (index, frame) in frames.iter().take(end).enumerate() {
        apply_catalog_membership_transaction(
            state,
            &f::scope()?,
            frame.as_bytes(),
            f::at(u64::try_from(index)?),
            f::driver_limits(),
        )?;
    }
    Ok(())
}
fn check(state: &mut RedbStateStore) -> TestResult {
    let scope = f::scope()?;
    let page = state.read_governance_catalog(
        GovernanceCatalogRead {
            scope: &scope,
            catalog: b"limit",
            cursor: None,
            limit: 256,
        },
        10000,
        64 * f::MIB,
    )?;
    assert_eq!(page.members, vec![b"c".to_vec(), b"e".to_vec()]);
    let tx = state.begin_apply(
        scope.group(),
        scope.recovery(),
        f::driver_limits().codec.admission(),
        10000,
        64 * f::MIB,
        32 * f::MIB,
    )?;
    let outcome = tx
        .outcome(scope.tenant(), &RequestId::new(b"add".to_vec())?)?
        .ok_or("missing catalog receipt")?;
    assert_eq!(
        (
            outcome.intent().version(),
            outcome.response_version(),
            outcome.original_position(),
            outcome.original_time().get()
        ),
        (14, 12, f::at(0), 1000)
    );
    assert_eq!(outcome.response(), b"FWGR\0\x01");
    Ok(())
}
fn cohort(dir: &Path) -> TestResult<CatalogCohort> {
    let db = redb::Builder::new().open(dir.join("state.redb"))?;
    let tx = db.begin_read()?;
    assert_eq!(tx.list_multimap_tables()?.count(), 0);
    let mut result = BTreeMap::new();
    for handle in tx.list_tables()? {
        let name = handle.name();
        let table = tx.open_table(TableDefinition::<&[u8], &[u8]>::new(name))?;
        let mut rows = BTreeMap::new();
        for row in table.iter()? {
            let (k, v) = row?;
            assert!(
                rows.insert(k.value().to_vec(), v.value().to_vec())
                    .is_none()
            );
        }
        assert!(result.insert(name.to_owned(), rows).is_none());
    }
    assert_eq!(result.len(), 6);
    Ok(result)
}

#[test]
fn governance_catalog_node_replays_once_preserves_retained_originals_and_closed_generic_profiles()
-> TestResult {
    let dir = f::directory("governance-node")?;
    let (mut log, mut state) = pair(&dir)?;
    let frames = frames()?;
    append(&mut log, &frames, false)?;
    apply(&mut state, &frames, 2)?;
    drop((log, state));
    let before = cohort(&dir)?;
    let logs = f::log_rows(&dir)?;
    let scope = f::scope()?;
    let (log, state) = f::open_pair(&dir)?;
    assert_eq!(
        recover_committed(log, state, scope.group(), scope.recovery(), caps()?).err(),
        Some(E::UnsupportedEntry)
    );
    assert_eq!(cohort(&dir)?, before);
    let (log, state) = f::open_pair(&dir)?;
    let (log, mut state) =
        recover_committed(log, state, scope.group(), scope.recovery(), selected()?)?;
    check(&mut state)?;
    drop((log, state));
    let before = cohort(&dir)?;
    let (log, state) = f::open_pair(&dir)?;
    drop(recover_committed(
        log,
        state,
        scope.group(),
        scope.recovery(),
        selected()?,
    )?);
    assert_eq!(cohort(&dir)?, before);
    assert_eq!(f::log_rows(&dir)?, logs);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}
#[test]
fn governance_catalog_node_audits_complete_historical_body_behind_a_valid_applied_last_entry()
-> TestResult {
    let dir = f::directory("governance-node-history")?;
    let (mut log, mut state) = pair(&dir)?;
    let frames = frames()?;
    append(&mut log, &frames, true)?;
    apply(&mut state, &frames, frames.len())?;
    check(&mut state)?;
    drop((log, state));
    let before = cohort(&dir)?;
    let logs = f::log_rows(&dir)?;
    let scope = f::scope()?;
    let (log, state) = f::open_pair(&dir)?;
    assert_eq!(
        recover_committed(log, state, scope.group(), scope.recovery(), selected()?).err(),
        Some(E::Codec(CodecError::TrailingBytes))
    );
    assert_eq!(cohort(&dir)?, before);
    assert_eq!(f::log_rows(&dir)?, logs);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}
fn received(dir: &Path, wire: &[u8], frames: &[PreparedBytes], expected: &f::Rows) -> TestResult {
    std::fs::create_dir(dir)?;
    let mut log = log(dir)?;
    for (index, frame) in frames.iter().enumerate() {
        f::append(&mut log, u64::try_from(index)?, frame.as_bytes())?;
    }
    f::append(&mut log, 6, b"opaque uncommitted catalog suffix")?;
    let scope = f::scope()?;
    let candidate = RedbSnapshotStage::receive(
        &dir.join("state.redb"),
        scope.group().clone(),
        scope.recovery().clone(),
        &mut std::io::Cursor::new(wire),
        StateStoreLimits::default(),
        SnapshotLimits::default(),
    )?;
    let (mut log, state) = restore_received_snapshot(
        log,
        candidate,
        scope.group(),
        scope.recovery(),
        selected()?,
        SnapshotLimits::default(),
    )?;
    log.save_committed(f::at(5))?;
    let (log, mut state) = recover_snapshotted(
        log,
        state,
        scope.group(),
        scope.recovery(),
        selected()?,
        SnapshotLimits::default(),
    )?;
    check(&mut state)?;
    drop((log, state));
    assert_eq!(&f::state_rows(dir)?, expected);
    Ok(())
}
#[test]
fn governance_catalog_node_purged_and_received_snapshots_preserve_suffix_and_receipt_only_history()
-> TestResult {
    let dir = f::directory("governance-node-snapshot")?;
    let source = dir.join("source");
    std::fs::create_dir(&source)?;
    let (mut log, mut state) = pair(&source)?;
    let frames = frames()?;
    append(&mut log, &frames, false)?;
    apply(&mut state, &frames, 2)?;
    let mut prefix = Vec::new();
    state.capture_snapshot(&mut prefix, SnapshotLimits::default())?;
    let scope = f::scope()?;
    let (log, state) = purge_snapshot(
        log,
        state,
        scope.group(),
        scope.recovery(),
        selected()?,
        SnapshotLimits::default(),
    )?;
    let (log, mut state) = recover_snapshotted(
        log,
        state,
        scope.group(),
        scope.recovery(),
        selected()?,
        SnapshotLimits::default(),
    )?;
    check(&mut state)?;
    let mut complete = Vec::new();
    let checkpoint = state.capture_snapshot(&mut complete, SnapshotLimits::default())?;
    assert_eq!(checkpoint.control().position(), f::at(5).index());
    drop((log, state));
    let expected = f::state_rows(&source)?;
    received(&dir.join("suffix"), &prefix, &frames, &expected)?;
    received(&dir.join("checkpoint"), &complete, &frames, &expected)?;
    std::fs::remove_dir_all(dir)?;
    Ok(())
}
