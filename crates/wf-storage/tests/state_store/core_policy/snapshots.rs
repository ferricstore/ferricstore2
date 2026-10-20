//! Birth/current manifests and mixed immutable generations survive real custody.

mod received;

use super::{TestResult, fixtures as f};
use redb::ReadableDatabase;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    io::Cursor,
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicU8, Ordering},
    },
};
use wf_domain::policy::complete::PolicySchemaLimits;
use wf_domain::policy::definition::StateMode;
use wf_domain::policy::installation::PolicyLimits;
use wf_domain::workflow::request::RetainedStatus;
use wf_domain::workflow::{CommittedCommand, Create, Operation};
use wf_ports::consensus::PreparedBatchItem;
use wf_ports::log::{RaftLogEntry, RaftLogLimits, RaftLogStore};
use wf_ports::snapshot::{
    SnapshotError, SnapshotLimits, SnapshotTransition, StateSnapshotStore,
    StateSnapshotTransitionStore,
};
use wf_ports::state::{GroupApplyStore, StateTransactionError};
use wf_storage::transaction::{
    CorePolicyTransactionError, apply_complete_core_policy_patch_transaction,
};
use wf_storage::{
    log::RedbLogStore,
    snapshot::RedbSnapshotStage,
    state::{RedbStateStore, StateStoreLimits},
};
use wf_types::identity::PartitionId;
use wf_types::version::TimestampMillis;

fn published(path: &Path) -> TestResult<f::Cohort> {
    let s = f::scope()?;
    drop(f::create(path, &s, &[b"p"], &[])?);
    let mut expected = f::cohort(path)?;
    let frame = f::frame(
        &s,
        b"publish",
        100,
        b"job",
        Some(0),
        f::mode_patch(&[(b"q", Some(StateMode::Fifo))]),
    )?;
    let response = f::policy_wire(b"job", 1, &[(b"q", StateMode::Fifo)], false)?;
    f::rows_mut(&mut expected)?.insert(f::key(6, &[b"g", b"z", b"t", b"job"]), response.clone());
    let metadata = f::metadata(
        &s,
        b"publish",
        0,
        100,
        f::frame_intent(frame.as_bytes(), 0)?,
        RetainedStatus::Accepted,
    )?;
    f::finish(
        &mut expected,
        &s,
        frame.as_bytes(),
        f::log(0),
        (f::clock(100)?, f::clock(100)?),
        Some((&metadata, 1, &response)),
    )?;
    f::upgrade5(
        &mut expected,
        &s,
        f::catalog(&s, [None, None], None, None, None)?,
    )?;
    f::apply(path, &s, frame.as_bytes(), 0, &expected, &response)?;
    Ok(expected)
}

fn checksum(bytes: &mut Vec<u8>) -> TestResult {
    let end = bytes
        .len()
        .checked_sub(32)
        .ok_or("short checksum fixture")?;
    let hash = Sha256::digest(&bytes[..end]);
    bytes[end..].copy_from_slice(&hash);
    Ok(())
}

fn frozen(expected: &mut f::Cohort, slot: usize) -> TestResult {
    let mut meta = expected[0].as_ref().ok_or("no metadata")?.clone();
    meta.remove(b"snapshot".as_slice()).ok_or("no descriptor")?;
    expected[2 + 2 * slot] = Some(meta);
    expected[3 + 2 * slot] = Some(f::rows(expected)?.clone());
    Ok(())
}

#[test]
fn policy_layout5_origin_and_current_census_match_independent_wires() -> TestResult {
    let dir = f::directory("policy-layout5")?;
    let path = dir.join("state");
    let s = f::scope()?;
    let expected = published(&path)?;
    let meta = expected[0].as_ref().ok_or("no live metadata")?;
    assert_eq!(meta.len(), 4);
    assert_eq!(meta[b"manifest".as_slice()].len(), 142);
    assert_eq!(&meta[b"manifest".as_slice()][..6], b"FWSS\0\x05");
    assert_eq!(&meta[b"origin".as_slice()][..6], b"FWSS\0\x03");
    let wanted = f::snapshot_wire(&s, &expected, 2)?;
    let mut store = f::open(&path, &s)?;
    let mut wire = Vec::new();
    let observed = store.capture_snapshot(&mut wire, SnapshotLimits::default())?;
    assert_eq!(wire, wanted);
    assert_eq!(observed.id().as_slice(), Sha256::digest(&wanted).as_slice());
    assert_eq!(observed.row_count(), 3 + f::rows(&expected)?.len());
    assert_eq!(observed.control().position().get(), 0);
    drop(store);
    let mut captured = expected.clone();
    frozen(&mut captured, 0)?;
    let facts = (
        Sha256::digest(&wire).into(),
        3 + f::rows(&expected)?.len(),
        wire.len(),
    );
    captured[0].as_mut().ok_or("no metadata")?.insert(
        b"snapshot".to_vec(),
        f::catalog(&s, [Some(facts), None], Some(0), None, None)?,
    );
    assert_eq!(f::cohort(&path)?, captured);
    drop(f::open(&path, &s)?);
    malformed_layouts(&dir, &expected)?;
    malformed_streams(&dir, &wire)?;
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

fn malformed_layouts(dir: &Path, original: &f::Cohort) -> TestResult {
    let s = f::scope()?;
    for case in 0..8 {
        let path = dir.join(format!("bad-layout-{case}"));
        drop(f::create(&path, &s, &[b"p"], &[])?);
        let initial = f::cohort(&path)?;
        let mut bad = original.clone();
        let meta = bad[0].as_mut().ok_or("no metadata")?;
        match case {
            0 => {
                meta.get_mut(b"origin".as_slice()).ok_or("no origin")?[19] ^= 1;
            }
            1 => {
                let manifest = meta.get_mut(b"manifest".as_slice()).ok_or("no manifest")?;
                manifest[46] ^= 1;
                checksum(manifest)?;
            }
            2 => {
                meta.get_mut(b"manifest".as_slice()).ok_or("no manifest")?[45] = 2;
                checksum(meta.get_mut(b"manifest".as_slice()).ok_or("no manifest")?)?;
            }
            3 => {
                meta.insert(b"unexpected".to_vec(), Vec::new());
            }
            4 => {
                meta.remove(b"origin".as_slice()).ok_or("no origin")?;
            }
            5 => {
                let manifest = meta.get_mut(b"manifest".as_slice()).ok_or("no manifest")?;
                manifest[4..6].copy_from_slice(&3_u16.to_be_bytes());
                checksum(manifest)?;
            }
            6 => {
                let control = f::key(0, &[b"g", b"z"]);
                let initial_control = f::rows(&initial)?
                    .get(&control)
                    .ok_or("initial control absent")?
                    .clone();
                f::rows_mut(&mut bad)?.retain(|key, _| key.get(6) != Some(&2));
                f::rows_mut(&mut bad)?.insert(control, initial_control);
            }
            _ => {
                let origin = meta.get_mut(b"origin".as_slice()).ok_or("no origin")?;
                origin[19] ^= 1;
                checksum(origin)?;
                let origin_hash = Sha256::digest(origin);
                let manifest = meta.get_mut(b"manifest".as_slice()).ok_or("no manifest")?;
                manifest[78..110].copy_from_slice(&origin_hash);
                checksum(manifest)?;
            }
        }
        f::replace(&path, &bad)?;
        assert!(
            matches!(
                RedbStateStore::open(
                    &path,
                    s.group().clone(),
                    s.recovery().clone(),
                    f::store_limits()?
                ),
                Err(StateTransactionError::Corrupt)
            ),
            "layout case {case}"
        );
        assert_eq!(f::cohort(&path)?, bad, "strict open repaired case {case}");
    }
    Ok(())
}

fn malformed_streams(dir: &Path, original: &[u8]) -> TestResult {
    let s = f::scope()?;
    for case in 0..4 {
        let mut wire = original.to_vec();
        match case {
            0 => wire[19] = 2,
            1 => wire[5] = 1,
            2 => wire[48] = b'x',
            _ => wire.push(0),
        }
        if case < 3 {
            checksum(&mut wire)?;
        }
        let path = dir.join(format!("bad-stream-{case}"));
        assert!(matches!(
            RedbSnapshotStage::receive(
                &path,
                s.group().clone(),
                s.recovery().clone(),
                &mut Cursor::new(&wire),
                f::store_limits()?,
                SnapshotLimits::default()
            ),
            Err(SnapshotError::Malformed)
        ));
        assert_empty_stage(&path)?;
    }
    Ok(())
}

fn legacy_descriptor(wire: &[u8], count: usize, pinned: bool) -> TestResult<Vec<u8>> {
    let mut out = if pinned {
        b"FWSD\0\x02\x01".to_vec()
    } else {
        b"FWSD\0\x01\x01".to_vec()
    };
    out.extend_from_slice(&u64::try_from(count)?.to_be_bytes());
    out.extend_from_slice(&u64::try_from(wire.len())?.to_be_bytes());
    out.extend_from_slice(&Sha256::digest(wire));
    let mut hash = Sha256::new();
    hash.update(b"wf-state-snapshot-descriptor-v1\0\x01g\0\x01z");
    hash.update(&out);
    out.extend_from_slice(&hash.finalize());
    Ok(out)
}

#[test]
fn policy_activation_preserves_frozen3_ids_and_transitions_to_snapshot2() -> TestResult {
    let dir = f::directory("policy-mixed-snapshots")?;
    let path = dir.join("state");
    let s = f::scope()?;
    let mut state = f::create(&path, &s, &[b"p"], &[])?;
    let row = f::owner(&s, b"p", f::data(b"seed", b"job", b"q", 0, Some(0))?, 0, 0)?;
    f::seed(&mut state, &s, std::slice::from_ref(&row), 0, 0)?;
    let seed = f::batch(
        &s,
        b"seed-0",
        0,
        vec![PreparedBatchItem {
            partition: PartitionId::new(b"p".to_vec())?,
            create_priority: 0,
            command: CommittedCommand {
                id: row.record.id().clone(),
                now: TimestampMillis::new(0),
                operation: Operation::Create(Create {
                    workflow_type: b"job".to_vec(),
                    state: b"q".to_vec(),
                    run_at: Some(TimestampMillis::new(0)),
                    payload: None,
                }),
            },
        }],
    )?;
    let mut log = RedbLogStore::create(
        &dir.join("log"),
        s.group().clone(),
        s.recovery().clone(),
        RaftLogLimits::default(),
    )?;
    log.append(vec![RaftLogEntry::new(f::log(0), seed.as_bytes().to_vec())])?;
    log.save_committed(f::log(0))?;
    drop(state);
    let before_capture = f::cohort(&path)?;
    let wanted_old = f::snapshot_wire(&s, &before_capture, 1)?;
    let mut state = f::open(&path, &s)?;
    let mut old_wire = Vec::new();
    let old = state.capture_snapshot(&mut old_wire, SnapshotLimits::default())?;
    assert_eq!(old_wire, wanted_old);
    let old_id = *old.id();
    let old_count = 2 + f::rows(&before_capture)?.len();
    assert_eq!(old.row_count(), old_count);
    assert_eq!(old_id.as_slice(), Sha256::digest(&old_wire).as_slice());
    state.pin_snapshot(old_id, SnapshotLimits::default())?;
    log.purge_through(f::log(0), old_id)?;
    drop(state);
    let mut expected = f::cohort(&path)?;
    let mut old_authority = expected.clone();
    old_authority[0].as_mut().ok_or("no metadata")?.insert(
        b"snapshot".to_vec(),
        legacy_descriptor(&old_wire, old_count, true)?,
    );
    assert_eq!(f::snapshot_wire(&s, &old_authority, 1)?, old_wire);
    assert_eq!(expected[0], old_authority[0]);
    let frozen_meta = expected[2].clone();
    let frozen_rows = expected[3].clone();
    let frame = f::frame(
        &s,
        b"activate",
        100,
        b"job",
        Some(0),
        f::mode_patch(&[(b"q", Some(StateMode::Fifo))]),
    )?;
    let response = f::policy_wire(b"job", 1, &[(b"q", StateMode::Fifo)], false)?;
    f::rows_mut(&mut expected)?.insert(f::due(&s, &row)?, vec![1]);
    f::rows_mut(&mut expected)?.insert(f::member(&s, &row), Vec::new());
    f::rows_mut(&mut expected)?.insert(f::key(6, &[b"g", b"z", b"t", b"job"]), response.clone());
    let metadata = f::metadata(
        &s,
        b"activate",
        1,
        100,
        f::frame_intent(frame.as_bytes(), 1)?,
        RetainedStatus::Accepted,
    )?;
    f::finish(
        &mut expected,
        &s,
        frame.as_bytes(),
        f::log(1),
        (f::clock(100)?, f::clock(100)?),
        Some((&metadata, 1, &response)),
    )?;
    f::upgrade5(
        &mut expected,
        &s,
        f::catalog(
            &s,
            [Some((old_id, old_count, old_wire.len())), None],
            Some(0),
            Some(0),
            None,
        )?,
    )?;
    log.append(vec![RaftLogEntry::new(
        f::log(1),
        frame.as_bytes().to_vec(),
    )])?;
    log.save_committed(f::log(1))?;
    f::apply(&path, &s, frame.as_bytes(), 1, &expected, &response)?;
    assert_eq!(
        (expected[2].clone(), expected[3].clone()),
        (frozen_meta, frozen_rows)
    );
    transition(
        &path,
        &s,
        &mut expected,
        &mut log,
        (&old_wire, old_count, old_id),
    )?;
    drop(log);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

fn transition(
    path: &Path,
    s: &wf_domain::workflow::request::RequestScope,
    expected: &mut f::Cohort,
    log: &mut RedbLogStore,
    old: (&[u8], usize, [u8; 32]),
) -> TestResult {
    let wanted = f::snapshot_wire(s, expected, 2)?;
    let new_id = Sha256::digest(&wanted).into();
    let new_count = 3 + f::rows(expected)?.len();
    let old_facts = Some((old.2, old.1, old.0.len()));
    let new_facts = Some((new_id, new_count, wanted.len()));
    let mut state = f::open(path, s)?;
    let mut retained = Vec::new();
    assert!(
        state
            .snapshot_by_id(old.2, &mut retained, SnapshotLimits::default())?
            .is_some()
    );
    assert_eq!(retained, old.0);
    let mut wire = Vec::new();
    assert_eq!(
        state
            .capture_snapshot(&mut wire, SnapshotLimits::default())?
            .id(),
        &new_id
    );
    assert_eq!(wire, wanted);
    frozen(expected, 1)?;
    expected[0].as_mut().ok_or("no metadata")?.insert(
        b"snapshot".to_vec(),
        f::catalog(s, [old_facts, new_facts], Some(1), Some(0), None)?,
    );
    let transition = SnapshotTransition::local(log.state()?, f::log(1), new_id)?;
    state.prepare_snapshot_transition(transition, SnapshotLimits::default())?;
    expected[0].as_mut().ok_or("no metadata")?.insert(
        b"snapshot".to_vec(),
        f::catalog(
            s,
            [old_facts, new_facts],
            Some(1),
            Some(0),
            Some(transition),
        )?,
    );
    drop(state);
    assert_eq!(f::cohort(path)?, *expected);
    let patch = f::frame(
        s,
        b"pending",
        101,
        b"job",
        Some(1),
        f::mode_patch(&[(b"q", Some(StateMode::Parallel))]),
    )?;
    let mut state = f::open(path, s)?;
    assert_eq!(
        apply_complete_core_policy_patch_transaction(
            &mut state,
            s,
            patch.as_bytes(),
            f::log(2),
            f::limits()
        ),
        Err(CorePolicyTransactionError::Transaction(
            StateTransactionError::InvalidChanges
        ))
    );
    assert_eq!(
        state.snapshot_transition(SnapshotLimits::default())?,
        Some(transition)
    );
    log.purge_through(f::log(1), new_id)?;
    state.finish_snapshot_transition(log.state()?, SnapshotLimits::default())?;
    expected[2] = Some(BTreeMap::new());
    expected[3] = Some(BTreeMap::new());
    expected[0].as_mut().ok_or("no metadata")?.insert(
        b"snapshot".to_vec(),
        f::catalog(s, [None, new_facts], Some(1), Some(1), None)?,
    );
    assert_eq!(state.snapshot_transition(SnapshotLimits::default())?, None);
    assert_eq!(state.snapshot_pin(SnapshotLimits::default())?, Some(new_id));
    assert!(
        state
            .snapshot_by_id(old.2, &mut Vec::new(), SnapshotLimits::default())?
            .is_none()
    );
    drop(state);
    assert_eq!(f::cohort(path)?, *expected);
    drop(f::open(path, s)?.upgrade_snapshot_layout(SnapshotLimits::default())?);
    assert_eq!(
        f::cohort(path)?,
        *expected,
        "layout5 upgrade no-op changed bytes"
    );
    Ok(())
}

fn assert_empty_stage(path: &Path) -> TestResult {
    let db = redb::Builder::new().open(path)?;
    let read = db.begin_read()?;
    assert_eq!(read.list_tables()?.count(), 0);
    assert_eq!(read.list_multimap_tables()?.count(), 0);
    Ok(())
}
