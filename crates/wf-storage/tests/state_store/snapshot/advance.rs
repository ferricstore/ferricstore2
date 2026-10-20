//! Explicit catalog upgrade preserves legacy authority and canonical artifact IDs.

use sha2::{Digest, Sha256};
use std::path::Path;

use super::{
    RawCohort, SnapshotError, SnapshotLimits, StateSnapshotStore, TestResult, f, fixtures as s,
};
use wf_ports::snapshot::{SnapshotTransition, StateSnapshotTransitionStore};
use wf_ports::state::StateTransactionError;

mod catalog;
mod faults;
mod limits;

const ALT_META: &str = "wf_state_snapshot_alt_meta_v1";
const ALT_ROWS: &str = "wf_state_snapshot_alt_rows_v1";
const LIVE4: &str = "46575353000400016700017a00000000000000000000000000000000000000000000000000000000000000000000b73e51559c5ba504491174aa43b682d142e85ebcbdefa7e8946f730b4617c70e02e0d5e86ae46bdde97b26048e0be124aef9c2de3dda0b16695a15ac15abee59";
const EMPTY: &str =
    "46575344000300000000006b7f90addd469fa6cfa857c45d76e7d680190b7ed2114f8107b8133784749e51";
const INITIAL: &str = "465753440003010001000000000000000300000000000001451e4bc7621eeb5aba5567c8fc9c2470d758faf7cd7663e4eb4c156370dc7a808000004935987e60b845c99d773e3a3d05e746da74a9c50b50f90f6bde73eb11070756";
const PINNED_INITIAL: &str = "465753440003010101000000000000000300000000000001451e4bc7621eeb5aba5567c8fc9c2470d758faf7cd7663e4eb4c156370dc7a808000002b7913dc41953487c345bcf47d2349bba74b95ddadf73728b3deb97d7e410598";

fn expected_upgrade(mut old: RawCohort, descriptor: &str) -> TestResult<RawCohort> {
    let meta = old.get_mut(s::META).ok_or("old metadata absent")?;
    assert!(meta.insert(b"manifest".to_vec(), s::hex(LIVE4)?).is_some());
    assert!(
        meta.insert(b"snapshot".to_vec(), s::hex(descriptor)?)
            .is_some()
    );
    assert!(
        old.insert(ALT_META.to_owned(), std::collections::BTreeMap::default())
            .is_none()
    );
    assert!(
        old.insert(ALT_ROWS.to_owned(), std::collections::BTreeMap::default())
            .is_none()
    );
    Ok(old)
}

fn assert_advanced(path: &Path, expected: &RawCohort) -> TestResult {
    assert_eq!(s::raw(path)?, *expected);
    let state = f::open(path)?;
    assert!(state.supports_snapshot_transitions()?);
    drop(state);
    assert_eq!(
        s::raw(path)?,
        *expected,
        "strict open must not repair storage"
    );
    Ok(())
}

fn manifest_version(bytes: &[u8], version: u16) -> Vec<u8> {
    assert!(bytes.len() >= 38);
    let mut result = bytes[..bytes.len() - 32].to_vec();
    assert_eq!(&result[..4], b"FWSS");
    result[4..6].copy_from_slice(&version.to_be_bytes());
    let checksum = Sha256::digest(&result);
    result.extend_from_slice(&checksum);
    result
}

// Independent FWSD3 packing: facts come from complete canonical test streams.
fn catalog_bytes(
    slots: [Option<(usize, &[u8])>; 2],
    current: u8,
    anchor: u8,
    pending: Option<SnapshotTransition>,
) -> TestResult<Vec<u8>> {
    let mut out = b"FWSD\0\x03".to_vec();
    out.extend_from_slice(&[current, anchor]);
    for slot in slots {
        out.push(u8::from(slot.is_some()));
        if let Some((rows, wire)) = slot {
            out.extend_from_slice(&u64::try_from(rows)?.to_be_bytes());
            out.extend_from_slice(&u64::try_from(wire.len())?.to_be_bytes());
            out.extend_from_slice(&Sha256::digest(wire));
        }
    }
    out.push(u8::from(pending.is_some()));
    if let Some(t) = pending {
        out.push(u8::from(t.is_received()));
        let b: wf_ports::log::RaftLogState = t.before();
        out.push(
            u8::from(b.vote().is_some())
                | (u8::from(b.committed().is_some()) << 1)
                | (u8::from(b.last_log().is_some()) << 2)
                | (u8::from(b.last_purged().is_some()) << 3),
        );
        if let Some(vote) = b.vote() {
            out.extend_from_slice(&vote.term().to_be_bytes());
            out.extend_from_slice(&vote.node().to_be_bytes());
            out.push(u8::from(vote.committed()));
        }
        for id in [b.committed(), b.last_log(), b.last_purged()]
            .into_iter()
            .flatten()
        {
            append_log_id(&mut out, id);
        }
        let prior_id: Option<[u8; 32]> = b.purged_snapshot_id();
        if let Some(id) = prior_id.as_ref() {
            out.extend_from_slice(id);
        }
        append_log_id(&mut out, t.position());
        out.extend_from_slice(t.id());
    }
    let mut hash = Sha256::new();
    hash.update(b"wf-state-snapshot-catalog-v1");
    hash.update(b"\0\x01g\0\x01z");
    hash.update(&out);
    out.extend_from_slice(&hash.finalize());
    Ok(out)
}

fn append_log_id(out: &mut Vec<u8>, id: wf_types::version::LogIdentity) {
    out.extend_from_slice(&id.term().to_be_bytes());
    out.extend_from_slice(&id.leader_node().to_be_bytes());
    out.extend_from_slice(&id.index().get().to_be_bytes());
}

#[test]
fn legacy_capability_refusal_precedes_profile_work_and_output() -> TestResult {
    let dir = f::directory("snapshot-legacy-mode")?;
    let path = dir.join("state.redb");
    let mut state = s::empty(&path)?;
    assert!(!state.supports_snapshot_transitions()?);
    let tiny = SnapshotLimits::new((1, 1, 1), (1, 1, 1), (1, 1), (1, 1), 1)?;
    let mut output = Vec::new();
    assert!(matches!(
        state.snapshot_by_id([0; 32], &mut output, tiny),
        Err(SnapshotError::State(
            StateTransactionError::UnsupportedLayout
        ))
    ));
    assert_eq!(output, [] as [u8; 0]);
    assert!(matches!(
        state.snapshot_transition(tiny),
        Err(SnapshotError::State(
            StateTransactionError::UnsupportedLayout
        ))
    ));
    assert_eq!(state.snapshot_pin(SnapshotLimits::default())?, None);
    assert!(
        state
            .current_snapshot(&mut output, SnapshotLimits::default())?
            .is_none()
    );
    drop(state);
    let before = s::raw(&path)?;
    assert_eq!(before.len(), 4);
    let state = f::open(&path)?;
    assert!(!state.supports_snapshot_transitions()?);
    drop(state);
    assert_eq!(s::raw(&path)?, before);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn explicit_upgrade_preserves_empty_captured_and_initial_pinned_complete_cohorts() -> TestResult {
    let dir = f::directory("snapshot-catalog-upgrade")?;
    for (label, captured, pinned, descriptor) in [
        ("empty", false, false, EMPTY),
        ("captured", true, false, INITIAL),
        ("pinned-unapplied", true, true, PINNED_INITIAL),
    ] {
        let path = dir.join(format!("{label}.redb"));
        let mut state = s::empty(&path)?;
        let mut wire = Vec::new();
        let saved = if captured {
            let metadata = state.capture_snapshot(&mut wire, SnapshotLimits::default())?;
            assert_eq!(wire, s::hex(s::INITIAL)?);
            assert_eq!(wire.len(), 325);
            assert!(metadata.control().last_entry().is_none());
            Some(*metadata.id())
        } else {
            None
        };
        if let Some(id) = saved.filter(|_| pinned) {
            // Legacy pin is local retention, including unapplied initial DATA.
            state.pin_snapshot(id, SnapshotLimits::default())?;
        }
        drop(state);
        let before = s::raw(&path)?;
        assert_eq!(before.len(), 4);
        let expected = expected_upgrade(before.clone(), descriptor)?;
        let state = f::open(&path)?;
        assert!(!state.supports_snapshot_transitions()?);
        let mut state = state.upgrade_snapshot_layout(SnapshotLimits::default())?;
        assert!(state.supports_snapshot_transitions()?);
        assert_eq!(state.snapshot_transition(SnapshotLimits::default())?, None);
        assert_eq!(
            state.snapshot_pin(SnapshotLimits::default())?,
            saved.filter(|_| pinned)
        );
        let mut retained = Vec::new();
        let metadata = state.current_snapshot(&mut retained, SnapshotLimits::default())?;
        assert_eq!(metadata.as_ref().map(|x| *x.id()), saved);
        assert_eq!(retained, wire);
        drop(metadata);
        if let Some(id) = saved {
            let mut by_id = Vec::new();
            let metadata = state
                .snapshot_by_id(id, &mut by_id, SnapshotLimits::default())?
                .ok_or("upgraded original artifact absent by ID")?;
            assert_eq!(metadata.id(), &id);
            assert_eq!(by_id, wire);
        }
        let mut absent = Vec::new();
        assert!(
            state
                .snapshot_by_id([0; 32], &mut absent, SnapshotLimits::default())?
                .is_none()
        );
        assert_eq!(absent, [] as [u8; 0]);
        drop(state);
        assert_advanced(&path, &expected)?;
        assert_eq!(expected[s::SNAP_META], before[s::SNAP_META]);
        assert_eq!(expected[s::SNAP_ROWS], before[s::SNAP_ROWS]);
        assert_eq!(expected[s::ROWS], before[s::ROWS]);
        let state = f::open(&path)?.upgrade_snapshot_layout(SnapshotLimits::default())?;
        assert!(state.supports_snapshot_transitions()?);
        drop(state);
        assert_advanced(&path, &expected)?;
    }
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn unchanged_applied_authority_after_upgrade_keeps_its_original_canonical_id() -> TestResult {
    let dir = f::directory("snapshot-upgrade-same-capture")?;
    let path = dir.join("state.redb");
    let mut state = s::create(&path)?;
    s::apply(&mut state, b"before-upgrade", 0, 10)?;
    let mut wire = Vec::new();
    let saved = state.capture_snapshot(&mut wire, SnapshotLimits::default())?;
    let id = *saved.id();
    let control = saved.control();
    drop(saved);
    state.pin_snapshot(id, SnapshotLimits::default())?;
    let mut state = state.upgrade_snapshot_layout(SnapshotLimits::default())?;
    let mut recaptured = Vec::new();
    let metadata = state.capture_snapshot(&mut recaptured, SnapshotLimits::default())?;
    assert_eq!(
        metadata.id(),
        &id,
        "local layout must not rename canonical DATA"
    );
    assert_eq!(metadata.control(), control);
    assert_eq!(recaptured, wire);
    drop(metadata);
    assert_eq!(state.snapshot_pin(SnapshotLimits::default())?, Some(id));
    assert_eq!(state.snapshot_transition(SnapshotLimits::default())?, None);
    drop(state);
    let before = s::raw(&path)?;
    let mut state = f::open(&path)?;
    let mut retained = Vec::new();
    let metadata = state
        .snapshot_by_id(id, &mut retained, SnapshotLimits::default())?
        .ok_or("same original artifact missing after reopen")?;
    assert_eq!(metadata.control(), control);
    assert_eq!(retained, wire);
    drop((metadata, state));
    assert_eq!(s::raw(&path)?, before);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}
