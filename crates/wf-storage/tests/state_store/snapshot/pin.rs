//! A durable local pin retains exact DATA without certifying Raft commitment.

use sha2::{Digest, Sha256};
use std::sync::{
    Arc,
    atomic::{AtomicU8, Ordering},
};
use wf_ports::state::StateTransactionError;
use wf_storage::state::{RedbStateStore, StateStoreLimits};

use super::super::{control, faults::wrapped};
use super::{
    RawCohort, SnapshotError, SnapshotLimits, StateSnapshotStore, TestResult, f, fixtures as s,
    limits,
};

const PINNED: &str = "46575344000201000000000000000300000000000001451e4bc7621eeb5aba5567c8fc9c2470d758faf7cd7663e4eb4c156370dc7a80802ec5f24ac81db435a4e5e4726ca46e1bb9efc579d2875af827595e73cd1115c5";

fn replace_descriptor(mut rows: RawCohort, descriptor: Vec<u8>) -> TestResult<RawCohort> {
    let old = rows
        .get_mut(s::META)
        .ok_or("no live metadata")?
        .insert(b"snapshot".to_vec(), descriptor);
    assert!(old.is_some());
    Ok(rows)
}

fn limited(error: SnapshotError) {
    assert!(
        matches!(
            error,
            SnapshotError::Limit | SnapshotError::State(StateTransactionError::Limit)
        ),
        "{error:?}"
    );
}

#[test]
fn pin_literal_idempotence_and_replacement_refusal_preserve_all_tables() -> TestResult {
    let dir = f::directory("snapshot-pin-literal")?;
    let path = dir.join("state.redb");
    let mut active = s::empty(&path)?;
    assert_eq!(active.snapshot_pin(SnapshotLimits::default())?, None);
    assert!(matches!(
        active.pin_snapshot([0; 32], SnapshotLimits::default()),
        Err(SnapshotError::StaleSnapshot)
    ));
    let mut wire = Vec::new();
    let observation = active.capture_snapshot(&mut wire, SnapshotLimits::default())?;
    let id = *observation.id();
    drop(observation);
    assert_eq!(wire, s::hex(s::INITIAL)?);
    drop(active);
    let unpinned = s::raw(&path)?;
    let expected = replace_descriptor(unpinned.clone(), s::hex(PINNED)?)?;
    let mut active = f::open(&path)?;
    assert!(matches!(
        active.pin_snapshot([0; 32], SnapshotLimits::default()),
        Err(SnapshotError::StaleSnapshot)
    ));
    assert_eq!(active.snapshot_pin(SnapshotLimits::default())?, None);
    active.pin_snapshot(id, SnapshotLimits::default())?;
    active.pin_snapshot(id, SnapshotLimits::default())?;
    assert_eq!(active.snapshot_pin(SnapshotLimits::default())?, Some(id));
    let mut forbidden = Vec::new();
    assert!(matches!(
        active.capture_snapshot(&mut forbidden, SnapshotLimits::default()),
        Err(SnapshotError::SnapshotPinned)
    ));
    assert_eq!(forbidden, [] as [u8; 0]);
    let candidate = s::stage(&dir.join("incoming.redb"), &wire)?;
    assert!(matches!(
        active.install_snapshot(candidate, SnapshotLimits::default()),
        Err(SnapshotError::SnapshotPinned)
    ));
    let mut current = Vec::new();
    let metadata = active
        .current_snapshot(&mut current, SnapshotLimits::default())?
        .ok_or("pinned artifact absent")?;
    assert_eq!(metadata.id(), &id);
    assert_eq!(current, wire);
    control::fixtures::current(&mut active)?;
    drop(active);
    s::assert_raw(&path, &expected)?;
    assert_eq!(expected[s::SNAP_META], unpinned[s::SNAP_META]);
    assert_eq!(expected[s::SNAP_ROWS], unpinned[s::SNAP_ROWS]);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn pinned_original_artifact_survives_real_later_workflow_application() -> TestResult {
    let dir = f::directory("snapshot-pin-later")?;
    let path = dir.join("state.redb");
    s::seed(&path)?;
    let wire = s::capture(&path)?;
    let before = s::raw(&path)?;
    let id: [u8; 32] = Sha256::digest(&wire).into();
    let mut active = f::open(&path)?;
    let saved = control::fixtures::current(&mut active)?;
    active.pin_snapshot(id, SnapshotLimits::default())?;
    s::apply(&mut active, b"after-pin", 4, 13)?;
    assert_eq!(
        control::fixtures::current(&mut active)?.position(),
        f::log(4).index()
    );
    assert_eq!(active.snapshot_pin(SnapshotLimits::default())?, Some(id));
    let mut retained = Vec::new();
    let observation = active
        .current_snapshot(&mut retained, SnapshotLimits::default())?
        .ok_or("pinned artifact absent after apply")?;
    assert_eq!(observation.control(), saved);
    assert_eq!(observation.id(), &id);
    assert_eq!(retained, wire);
    drop(active);
    let after = s::raw(&path)?;
    assert_eq!(after[s::SNAP_META], before[s::SNAP_META]);
    assert_eq!(after[s::SNAP_ROWS], before[s::SNAP_ROWS]);
    assert_ne!(after[s::ROWS], before[s::ROWS]);
    s::assert_raw(&path, &after)?;
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn scalar_pin_profiles_do_not_materialize_a_metadata_owner() -> TestResult {
    let dir = f::directory("snapshot-pin-profile")?;
    let path = dir.join("state.redb");
    drop(s::empty(&path)?);
    let wire = s::capture(&path)?;
    let before = s::raw(&path)?;
    let id: [u8; 32] = Sha256::digest(&wire).into();
    let c = f::MIB;
    let w = 514 + 2048 + 1 + c;
    let cap = limits::profile((3, 325, 118), (1, 1), (1, c), w)?;
    let short = limits::profile((3, 325, 118), (1, 1), (1, c), w - 1)?;
    let mut active = f::open(&path)?;
    limited(
        active
            .pin_snapshot(id, short)
            .err()
            .ok_or("short W accepted")?,
    );
    assert_eq!(active.snapshot_pin(cap)?, None);
    drop(active);
    s::assert_raw(&path, &before)?;
    let mut active = f::open(&path)?;
    active.pin_snapshot(id, cap)?;
    assert_eq!(active.snapshot_pin(cap)?, Some(id));
    let mut sink = Vec::new();
    limited(
        active
            .current_snapshot(&mut sink, cap)
            .err()
            .ok_or("metadata R1 accepted")?,
    );
    assert_eq!(sink, [] as [u8; 0]);
    let tiny = limits::profile((3, 325, 118), (1, 1), (1, 1), w)?;
    limited(active.snapshot_pin(tiny).err().ok_or("tiny C accepted")?);
    assert_eq!(active.snapshot_pin(cap)?, Some(id));
    control::fixtures::current(&mut active)?;
    drop(active);
    s::assert_raw(&path, &replace_descriptor(before, s::hex(PINNED)?)?)?;
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

fn checksum(prefix: &[u8]) -> Vec<u8> {
    let mut output = prefix.to_vec();
    let mut hash = Sha256::new();
    hash.update(b"wf-state-snapshot-descriptor-v1");
    hash.update(b"\0\x01g\0\x01z");
    hash.update(prefix);
    output.extend_from_slice(&hash.finalize());
    output
}

#[test]
fn pinned_descriptor_rejects_unsupported_or_unanchored_flags_without_repair() -> TestResult {
    let dir = f::directory("snapshot-pin-corrupt")?;
    let good = dir.join("good.redb");
    drop(s::empty(&good)?);
    s::capture(&good)?;
    let base = s::raw(&good)?;
    let pinned = s::hex(PINNED)?;
    let mut bad_some = pinned[..55].to_vec();
    bad_some[6] = 2;
    let mut bad_version = pinned[..55].to_vec();
    bad_version[5] = 3;
    let mut old_bad = pinned[..55].to_vec();
    old_bad[5] = 1;
    old_bad[6] = 2;
    let scope = f::scope()?;
    for (index, value) in [
        checksum(b"FWSD\0\x02\0"),
        checksum(&bad_some),
        checksum(&bad_version),
        checksum(&old_bad),
    ]
    .into_iter()
    .enumerate()
    {
        let path = dir.join(format!("bad-{index}.redb"));
        drop(s::empty(&path)?);
        let expected = replace_descriptor(base.clone(), value)?;
        s::replace(&path, &expected)?;
        assert!(matches!(
            RedbStateStore::open(
                &path,
                scope.group().clone(),
                scope.recovery().clone(),
                StateStoreLimits::default()
            ),
            Err(StateTransactionError::Corrupt)
        ));
        assert_eq!(s::raw(&path)?, expected);
    }
    let expected = replace_descriptor(base, s::hex(PINNED)?)?;
    s::replace(&good, &expected)?;
    s::assert_raw(&good, &expected)?;
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn pin_sync_uncertainty_fences_then_reconciles_exact_descriptor() -> TestResult {
    let dir = f::directory("snapshot-pin-uncertain")?;
    let path = dir.join("state.redb");
    drop(s::empty(&path)?);
    let wire = s::capture(&path)?;
    let old = s::raw(&path)?;
    let new = replace_descriptor(old.clone(), s::hex(PINNED)?)?;
    let id: [u8; 32] = Sha256::digest(&wire).into();
    let fault = Arc::new(AtomicU8::new(0));
    let mut active = wrapped(&path, Arc::clone(&fault))?;
    fault.store(4, Ordering::SeqCst);
    assert!(matches!(
        active.pin_snapshot(id, SnapshotLimits::default()),
        Err(SnapshotError::State(StateTransactionError::Uncertain))
    ));
    assert_eq!(
        fault.load(Ordering::SeqCst),
        0,
        "actual pin sync must be reached"
    );
    assert!(matches!(
        active.snapshot_pin(SnapshotLimits::default()),
        Err(SnapshotError::State(StateTransactionError::Unavailable))
    ));
    assert!(matches!(
        active.capture_snapshot(&mut Vec::new(), SnapshotLimits::default()),
        Err(SnapshotError::State(StateTransactionError::Unavailable))
    ));
    drop(active);
    let actual = s::raw(&path)?;
    assert!(actual == old || actual == new, "partial pinned cohort");
    let mut active = f::open(&path)?;
    active.pin_snapshot(id, SnapshotLimits::default())?;
    assert_eq!(active.snapshot_pin(SnapshotLimits::default())?, Some(id));
    drop(active);
    s::assert_raw(&path, &new)?;
    std::fs::remove_dir_all(dir)?;
    Ok(())
}
