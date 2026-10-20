//! Independent scalar transitions and checksum-correct malformed catalogs.

use super::{
    ALT_META, ALT_ROWS, INITIAL, LIVE4, RawCohort, SnapshotLimits, StateSnapshotStore, TestResult,
    assert_advanced, catalog_bytes, f, manifest_version, s,
};
use sha2::{Digest, Sha256};
use std::path::Path;
use wf_ports::log::{RaftLogState, RaftVote};
use wf_ports::snapshot::{SnapshotError, SnapshotTransition, StateSnapshotTransitionStore};
use wf_ports::state::StateTransactionError;
use wf_storage::state::{RedbStateStore, StateStoreLimits};
use wf_types::version::{AppliedPosition, LogIdentity};

fn at(term: u64, node: u64, index: u64) -> LogIdentity {
    LogIdentity::new(term, node, AppliedPosition::new(index))
}

#[test]
fn transition_constructors_preserve_exact_math_without_issuing_authority() -> TestResult {
    let old = RaftLogState::new_with_anchor(
        Some(RaftVote::new(9, 7, true)),
        Some(at(4, 7, 8)),
        Some(at(6, 9, 12)),
        Some((at(2, 7, 3), [17; 32])),
    );
    let local = SnapshotTransition::local(old, at(3, 7, 6), [34; 32])?;
    assert_eq!(local.before(), old);
    assert_eq!(local.position(), at(3, 7, 6));
    assert_eq!(local.id(), &[34; 32]);
    assert!(!local.is_received());
    let received = SnapshotTransition::received(old, at(7, 9, 13), [34; 32])?;
    assert_eq!(received.before(), old);
    assert_eq!(received.position(), at(7, 9, 13));
    assert_eq!(received.id(), &[34; 32]);
    assert!(received.is_received());
    // Constructors carry checked mathematical inputs, not a consensus issuer.
    for (position, id) in [
        (at(2, 7, 2), [17; 32]),
        (at(2, 8, 3), [17; 32]),
        (at(2, 7, 3), [34; 32]),
        (at(5, 7, 8), [34; 32]),
        (at(5, 7, 9), [34; 32]),
    ] {
        assert!(matches!(
            SnapshotTransition::local(old, position, id),
            Err(SnapshotError::TransitionConflict)
        ));
    }
    for position in [at(4, 7, 8), at(3, 7, 9), at(4, 6, 9)] {
        assert!(matches!(
            SnapshotTransition::received(old, position, [34; 32]),
            Err(SnapshotError::TransitionConflict)
        ));
    }
    let empty = RaftLogState::new(None, None, None);
    assert!(matches!(
        SnapshotTransition::local(empty, at(1, 7, 0), [34; 32]),
        Err(SnapshotError::TransitionConflict)
    ));
    let first = SnapshotTransition::received(empty, at(1, 7, 0), [34; 32])?;
    assert!(first.is_received());
    let max = at(u64::MAX, u64::MAX, u64::MAX);
    assert_eq!(
        SnapshotTransition::received(empty, max, [34; 32])?.position(),
        max
    );
    Ok(())
}

fn checked(prefix: &[u8]) -> Vec<u8> {
    let mut bytes = prefix.to_vec();
    let mut hash = Sha256::new();
    hash.update(b"wf-state-snapshot-catalog-v1");
    hash.update(b"\0\x01g\0\x01z");
    hash.update(prefix);
    bytes.extend_from_slice(&hash.finalize());
    bytes
}

fn assert_corrupt(path: &Path, expected: &RawCohort) -> TestResult {
    s::replace(path, expected)?;
    let scope = f::scope()?;
    assert!(
        matches!(
            RedbStateStore::open(
                path,
                scope.group().clone(),
                scope.recovery().clone(),
                StateStoreLimits::default()
            ),
            Err(StateTransactionError::Corrupt)
        ),
        "{}",
        path.display()
    );
    assert_eq!(s::raw(path)?, *expected, "{}", path.display());
    Ok(())
}

#[test]
fn strict_catalog_rejects_checksum_correct_flags_selectors_facts_and_eof_without_repair()
-> TestResult {
    let dir = f::directory("snapshot-catalog-corrupt")?;
    let base_path = dir.join("base.redb");
    let mut state = s::empty(&base_path)?;
    state.capture_snapshot(&mut std::io::sink(), SnapshotLimits::default())?;
    drop(state.upgrade_snapshot_layout(SnapshotLimits::default())?);
    let base = s::raw(&base_path)?;
    let golden = s::hex(INITIAL)?;
    assert_eq!(golden.len(), 91);
    for case in 0..12 {
        let path = dir.join(format!("bad-{case}.redb"));
        drop(s::empty(&path)?);
        let mut expected = base.clone();
        let mut prefix = golden[..59].to_vec();
        match case {
            0 => prefix[5] = 4,
            1 => prefix[6] = 3,
            2 => prefix[6] = 0,
            3 => prefix[7] = 2,
            4 => prefix[8] = 2,
            5 => prefix[57] = 2,
            6 => prefix[58] = 2,
            7 => prefix[16] = 4,
            8 => prefix[24] = 0,
            9 => prefix[25] ^= 1,
            10 => prefix.push(0),
            11 => {
                prefix.pop().ok_or("missing pending tag")?;
            }
            _ => return Err("unknown malformed catalog case".into()),
        }
        expected
            .get_mut(s::META)
            .ok_or("no metadata")?
            .insert(b"snapshot".to_vec(), checked(&prefix));
        assert_corrupt(&path, &expected)?;
    }
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn duplicate_ids_fail_with_two_individually_complete_artifact_cohorts() -> TestResult {
    let dir = f::directory("snapshot-duplicate-catalog-id")?;
    let path = dir.join("state.redb");
    let mut state = s::empty(&path)?;
    state.capture_snapshot(&mut std::io::sink(), SnapshotLimits::default())?;
    drop(state.upgrade_snapshot_layout(SnapshotLimits::default())?);
    let mut expected = s::raw(&path)?;
    let original_meta = expected[s::SNAP_META].clone();
    let original_rows = expected[s::SNAP_ROWS].clone();
    expected.insert(ALT_META.to_owned(), original_meta);
    expected.insert(ALT_ROWS.to_owned(), original_rows);
    let golden = s::hex(INITIAL)?;
    let mut prefix = b"FWSD\0\x03\x01\x02".to_vec();
    prefix.extend_from_slice(&golden[8..57]);
    prefix.extend_from_slice(&golden[8..57]);
    prefix.push(0);
    expected
        .get_mut(s::META)
        .ok_or("no metadata")?
        .insert(b"snapshot".to_vec(), checked(&prefix));
    assert_corrupt(&path, &expected)?;
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn catalog_and_six_table_layout_are_required_together_without_open_migration() -> TestResult {
    let dir = f::directory("snapshot-catalog-inventory")?;
    let good = dir.join("good.redb");
    let mut state = s::empty(&good)?;
    state.capture_snapshot(&mut std::io::sink(), SnapshotLimits::default())?;
    drop(state);
    let old = s::raw(&good)?;
    drop(f::open(&good)?.upgrade_snapshot_layout(SnapshotLimits::default())?);
    let base = s::raw(&good)?;
    for case in 0..4 {
        let path = dir.join(format!("bad-{case}.redb"));
        drop(s::empty(&path)?);
        let mut expected = base.clone();
        match case {
            0 => {
                expected.remove(ALT_ROWS).ok_or("missing alternate table")?;
            }
            1 => {
                expected.insert(
                    "unexpected".to_owned(),
                    std::collections::BTreeMap::default(),
                );
            }
            2 => {
                expected
                    .get_mut(s::META)
                    .ok_or("no metadata")?
                    .remove(b"snapshot".as_slice())
                    .ok_or("missing catalog")?;
            }
            3 => {
                assert_eq!(expected[s::META][b"manifest".as_slice()], s::hex(LIVE4)?);
                let original = old[s::META][b"manifest".as_slice()].clone();
                expected
                    .get_mut(s::META)
                    .ok_or("no metadata")?
                    .insert(b"manifest".to_vec(), original);
            }
            _ => return Err("unknown inventory case".into()),
        }
        assert_corrupt(&path, &expected)?;
    }
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn unanchored_alternate_current_is_replaced_without_unreferenced_data() -> TestResult {
    let dir = f::directory("snapshot-mirrored-current")?;
    for install in [false, true] {
        let path = dir.join(format!("target-{install}.redb"));
        let mut state = s::create(&path)?;
        s::apply(&mut state, b"before-mirror", 0, 10)?;
        let mut old_wire = Vec::new();
        let metadata = state.capture_snapshot(&mut old_wire, SnapshotLimits::default())?;
        let old_id = *metadata.id();
        drop(metadata);
        drop(
            state
                .upgrade_snapshot_layout(SnapshotLimits::default())
                .map_err(|error| format!("initial upgrade failed: {error:?}"))?,
        );
        let mut mirrored = s::raw(&path)?;
        let old_meta = std::mem::take(mirrored.get_mut(s::SNAP_META).ok_or("no old META")?);
        let old_rows = std::mem::take(mirrored.get_mut(s::SNAP_ROWS).ok_or("no old ROWS")?);
        let count = old_meta.len() + old_rows.len();
        mirrored.insert(ALT_META.to_owned(), old_meta);
        mirrored.insert(ALT_ROWS.to_owned(), old_rows);
        mirrored.get_mut(s::META).ok_or("no LIVE metadata")?.insert(
            b"snapshot".to_vec(),
            catalog_bytes([None, Some((count, &old_wire))], 2, 0, None)?,
        );
        s::replace(&path, &mirrored)?;
        assert_advanced(&path, &mirrored)
            .map_err(|error| format!("valid mirrored precondition failed: {error:?}"))?;
        let mut state = f::open(&path)?;
        let mut original = Vec::new();
        state
            .current_snapshot(&mut original, SnapshotLimits::default())?
            .ok_or("valid mirrored current absent")?;
        assert_eq!(original, old_wire);
        s::apply(&mut state, b"after-mirror", 1, 11)?;
        drop(state);
        let before = s::raw(&path)?;
        let mut authority = s::authority(&before)?;
        let meta = authority.get_mut(s::META).ok_or("no canonical metadata")?;
        let canonical = manifest_version(&meta[b"manifest".as_slice()], 3);
        meta.insert(b"manifest".to_vec(), canonical);
        let wire = s::frame((b"g", b"z"), &authority)?;
        let count = authority
            .values()
            .map(std::collections::BTreeMap::len)
            .sum();
        let mut expected = before.clone();
        expected.insert(ALT_META.to_owned(), authority[s::META].clone());
        expected.insert(ALT_ROWS.to_owned(), authority[s::ROWS].clone());
        expected
            .get_mut(s::META)
            .ok_or("no expected metadata")?
            .insert(
                b"snapshot".to_vec(),
                catalog_bytes([None, Some((count, &wire))], 2, 0, None)?,
            );
        let mut state = f::open(&path)?;
        let metadata = if install {
            state
                .install_snapshot(
                    s::stage(&dir.join("incoming.redb"), &wire)?,
                    SnapshotLimits::default(),
                )
                .map_err(|error| format!("mirrored install replacement failed: {error:?}"))?
        } else {
            let mut output = Vec::new();
            let metadata = state
                .capture_snapshot(&mut output, SnapshotLimits::default())
                .map_err(|error| format!("mirrored capture replacement failed: {error:?}"))?;
            assert_eq!(output, wire);
            metadata
        };
        assert_eq!(metadata.id().as_slice(), Sha256::digest(&wire).as_slice());
        drop(metadata);
        let mut retired = Vec::new();
        assert!(
            state
                .snapshot_by_id(old_id, &mut retired, SnapshotLimits::default())?
                .is_none()
        );
        assert_eq!(retired, []);
        let mut current = Vec::new();
        state
            .current_snapshot(&mut current, SnapshotLimits::default())?
            .ok_or("new mirrored current absent")?;
        assert_eq!(current, wire);
        drop(state);
        assert_advanced(&path, &expected)?;
        assert!(expected[s::SNAP_META].is_empty());
        assert!(expected[s::SNAP_ROWS].is_empty());
    }
    std::fs::remove_dir_all(dir)?;
    Ok(())
}
