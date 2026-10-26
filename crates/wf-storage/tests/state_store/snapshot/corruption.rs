//! Valid checksums cannot excuse invalid authority or an incompatible live join.

use sha2::{Digest, Sha256};
use std::{io::Cursor, path::Path};
use wf_domain::workflow::control::{ControlState, EntryIdentity, EntryStamp, PreparedDigest};
use wf_ports::state::StateTransactionError;
use wf_storage::state::{StateStoreLimits, key, value};

use super::super::control;
use super::{
    RawCohort, RedbSnapshotStage, SnapshotError, SnapshotLimits, StateSnapshotStore, TestResult, f,
    fixtures as s,
};

fn rehash(bytes: &mut [u8]) {
    let end = bytes.len() - 32;
    let sha = Sha256::digest(&bytes[..end]);
    bytes[end..].copy_from_slice(&sha);
}

fn receive_error(path: &Path, wire: &[u8], expected: SnapshotError) -> TestResult {
    let scope = f::scope()?;
    let error = RedbSnapshotStage::receive(
        path,
        scope.group().clone(),
        scope.recovery().clone(),
        &mut Cursor::new(wire),
        StateStoreLimits::default(),
        SnapshotLimits::default(),
    )
    .err()
    .ok_or("damaged snapshot accepted")?;
    assert_eq!(error, expected);
    Ok(())
}

#[test]
fn stream_framing_counts_order_checksum_and_real_eof() -> TestResult {
    let dir = f::directory("snapshot-framing")?;
    let literal = s::hex(s::INITIAL)?;
    for (index, label) in [
        "magic",
        "version",
        "group",
        "recovery",
        "meta-count",
        "row-count",
        "encoded-count",
        "key-limit",
        "value-limit",
        "metadata-key",
        "duplicate",
        "checksum",
        "truncated",
        "trailing",
        "legacy-body-as-v2",
        "legacy-body-as-v3",
        "legacy-body-as-v4",
    ]
    .into_iter()
    .enumerate()
    {
        let mut wire = literal.clone();
        let mut expected = SnapshotError::Malformed;
        match label {
            "magic" => wire[0] = 0,
            "version" => {
                wire[5] = u8::MAX;
                expected = SnapshotError::UnsupportedVersion;
            }
            "legacy-body-as-v2" => wire[5] = 2,
            "legacy-body-as-v3" => wire[5] = 3,
            "legacy-body-as-v4" => wire[5] = 4,
            "group" => {
                wire[8] = b'f';
                expected = SnapshotError::NamespaceMismatch;
            }
            "recovery" => {
                wire[11] = b'f';
                expected = SnapshotError::NamespaceMismatch;
            }
            "meta-count" => wire[12..20].copy_from_slice(&1_u64.to_be_bytes()),
            "row-count" => wire[20..28].copy_from_slice(&2_u64.to_be_bytes()),
            "encoded-count" => wire[28..36].copy_from_slice(&326_u64.to_be_bytes()),
            "key-limit" => {
                wire[36..40].copy_from_slice(&u32::MAX.to_be_bytes());
                expected = SnapshotError::Limit;
            }
            "value-limit" => {
                wire[40..48].copy_from_slice(&u64::MAX.to_be_bytes());
                expected = SnapshotError::Limit;
            }
            "metadata-key" => wire[55] = b'z',
            "duplicate" => {
                let row = wire[233..293].to_vec();
                wire.splice(293..293, row);
                wire[20..28].copy_from_slice(&2_u64.to_be_bytes());
                wire[28..36].copy_from_slice(&385_u64.to_be_bytes());
            }
            "checksum" => wire[324] ^= 1,
            "truncated" => {
                wire.pop();
            }
            "trailing" => wire.push(0),
            _ => return Err("unknown framing mutation".into()),
        }
        if !matches!(label, "checksum" | "truncated" | "trailing") {
            rehash(&mut wire);
        }
        receive_error(&dir.join(format!("{index}.redb")), &wire, expected)?;
    }
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn canonical_digest_still_requires_bidirectional_semantic_authority() -> TestResult {
    let dir = f::directory("snapshot-semantic")?;
    let source = dir.join("source.redb");
    let source_before = s::seed(&source)?;
    let original = s::authority(&source_before)?;
    let canonical = original[s::ROWS]
        .keys()
        .find(|k| k.get(6) == Some(&1))
        .ok_or("no canonical record")?
        .clone();
    let due = original[s::ROWS]
        .iter()
        .find(|(k, v)| k.get(6) == Some(&3) && v.as_slice() == [1])
        .ok_or("no head due")?
        .0
        .clone();
    let fifo = original[s::ROWS]
        .keys()
        .find(|k| k.get(6) == Some(&4))
        .ok_or("no FIFO member")?
        .clone();
    for (index, label) in [
        "missing-record",
        "missing-due",
        "missing-fifo",
        "wrong-due",
        "nonempty-fifo",
        "foreign-key",
        "unknown-family",
    ]
    .into_iter()
    .enumerate()
    {
        let mut data = original.clone();
        let rows = data.get_mut(s::ROWS).ok_or("rows")?;
        match label {
            "missing-record" => {
                rows.remove(&canonical);
            }
            "missing-due" => {
                rows.remove(&due);
            }
            "missing-fifo" => {
                rows.remove(&fifo);
            }
            "wrong-due" => {
                rows.insert(due.clone(), vec![0]);
            }
            "nonempty-fifo" => {
                rows.insert(fifo.clone(), vec![1]);
            }
            "foreign-key" => {
                let value = rows.remove(&canonical).ok_or("canonical")?;
                let mut k = canonical.clone();
                k[7] = b'f';
                rows.insert(k, value);
            }
            "unknown-family" => {
                let value = rows.remove(&canonical).ok_or("canonical")?;
                let mut k = canonical.clone();
                k[6] = 8;
                rows.insert(k, value);
            }
            _ => return Err("unknown semantic mutation".into()),
        }
        let wire = s::frame((b"g", b"z"), &data)?;
        assert_eq!(
            Sha256::digest(&wire[..wire.len() - 32]).as_slice(),
            &wire[wire.len() - 32..]
        );
        receive_error(
            &dir.join(format!("bad-{index}.redb")),
            &wire,
            SnapshotError::Stage(StateTransactionError::Corrupt),
        )?;
    }
    let wire = s::frame((b"g", b"z"), &original)?;
    drop(s::stage(&dir.join("valid.redb"), &wire)?.into_store()?);
    s::assert_raw(&source, &source_before)?;
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

fn artifact_wire(data: &RawCohort) -> TestResult<Vec<u8>> {
    let selected = std::collections::BTreeMap::from([
        (s::META.to_owned(), data[s::SNAP_META].clone()),
        (s::ROWS.to_owned(), data[s::SNAP_ROWS].clone()),
    ]);
    s::frame((b"g", b"z"), &selected)
}

#[test]
fn strict_reopen_checks_historical_artifact_beside_newer_valid_live_state() -> TestResult {
    let dir = f::directory("snapshot-reopen")?;
    let path = dir.join("state.redb");
    s::seed(&path)?;
    s::capture(&path)?;
    let mut state = f::open(&path)?;
    s::apply(&mut state, b"newer", 4, 13)?;
    drop(state);
    let original = s::raw(&path)?;
    for label in [
        "descriptor",
        "missing-frozen",
        "semantic-rehashed",
        "extra-live-meta",
        "missing-table",
        "none-with-data",
    ] {
        let mut data = original.clone();
        match label {
            "descriptor" => {
                let d = data
                    .get_mut(s::META)
                    .ok_or("metadata")?
                    .get_mut(b"snapshot".as_slice())
                    .ok_or("descriptor")?;
                d[86] ^= 1;
            }
            "missing-frozen" => {
                let k = data[s::SNAP_ROWS]
                    .keys()
                    .next()
                    .ok_or("snapshot rows")?
                    .clone();
                data.get_mut(s::SNAP_ROWS).ok_or("rows")?.remove(&k);
            }
            "semantic-rehashed" => {
                let due = data[s::SNAP_ROWS]
                    .iter()
                    .find(|(k, v)| k.get(6) == Some(&3) && v.as_slice() == [1])
                    .ok_or("head due")?
                    .0
                    .clone();
                data.get_mut(s::SNAP_ROWS)
                    .ok_or("rows")?
                    .insert(due, vec![0]);
                let wire = artifact_wire(&data)?;
                let n = data[s::SNAP_ROWS].len() + 2;
                data.get_mut(s::META)
                    .ok_or("metadata")?
                    .insert(b"snapshot".to_vec(), s::descriptor((b"g", b"z"), n, &wire)?);
            }
            "extra-live-meta" => {
                data.get_mut(s::META)
                    .ok_or("metadata")?
                    .insert(b"unexpected".to_vec(), vec![0]);
            }
            "missing-table" => {
                data.remove(s::SNAP_META);
            }
            "none-with-data" => {
                data.get_mut(s::META)
                    .ok_or("metadata")?
                    .insert(b"snapshot".to_vec(), s::hex(s::NONE)?);
            }
            _ => return Err("unknown persisted mutation".into()),
        }
        assert_eq!(data[s::ROWS], original[s::ROWS]);
        s::replace(&path, &data)?;
        let error = f::open(&path).err().ok_or("damaged artifact opened")?;
        assert_eq!(
            error.downcast_ref::<StateTransactionError>(),
            Some(&StateTransactionError::Corrupt),
            "{label}"
        );
        assert_eq!(s::raw(&path)?, data, "reopen repaired {label}");
    }
    s::replace(&path, &original)?;
    s::assert_raw(&path, &original)?;
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

fn install_refused(
    path: &Path,
    stage_path: &Path,
    data: &RawCohort,
    expected: SnapshotError,
) -> TestResult {
    let before = s::raw(path)?;
    let wire = s::frame((b"g", b"z"), data)?;
    let incoming_stage = s::stage(stage_path, &wire)?;
    let mut state = f::open(path)?;
    let prior = control::fixtures::current(&mut state)?;
    let error = state
        .install_snapshot(incoming_stage, SnapshotLimits::default())
        .err()
        .ok_or("incompatible install succeeded")?;
    assert_eq!(error, expected);
    assert_eq!(
        control::fixtures::current(&mut state)?,
        prior,
        "healthy refusal fenced active state"
    );
    drop(state);
    s::assert_raw(path, &before)
}

#[test]
fn live_install_preserves_genesis_control_clock_and_membership_lineage() -> TestResult {
    let dir = f::directory("snapshot-lineage")?;
    let target = dir.join("target.redb");
    let original = s::authority(&s::seed(&target)?)?;
    s::capture(&target)?;
    let mut active = f::open(&target)?;
    s::apply(&mut active, b"advance", 4, 100)?;
    drop(active);
    install_refused(
        &target,
        &dir.join("stale.redb"),
        &original,
        SnapshotError::StaleSnapshot,
    )?;
    let mut changed = original.clone();
    let scope = f::scope()?;
    let control_key = key::encode_control_key(scope.group(), scope.recovery(), f::MIB)?;
    let next = ControlState::restored(
        f::clock(101)?,
        EntryIdentity::new(f::log(5), PreparedDigest::new([7; 32])),
        EntryStamp::Unstamped,
    )?;
    changed.get_mut(s::ROWS).ok_or("rows")?.insert(
        control_key.clone(),
        value::encode_control(scope.group(), scope.recovery(), next, f::MIB)?,
    );
    changed.get_mut(s::META).ok_or("meta")?.insert(
        b"membership".to_vec(),
        control::fixtures::hex(control::fixtures::NONE)?,
    );
    install_refused(
        &target,
        &dir.join("none.redb"),
        &changed,
        SnapshotError::MembershipMismatch,
    )?;
    let mut prior_m = original.clone();
    let old = control::fixtures::frame(true, EntryStamp::Unstamped)?;
    prior_m.get_mut(s::META).ok_or("meta")?.insert(
        b"membership".to_vec(),
        control::fixtures::row(Some(f::log(0)), Some(old.as_bytes()))?,
    );
    prior_m.get_mut(s::ROWS).ok_or("rows")?.insert(
        control_key.clone(),
        value::encode_control(scope.group(), scope.recovery(), next, f::MIB)?,
    );
    install_refused(
        &target,
        &dir.join("old-m.redb"),
        &prior_m,
        SnapshotError::MembershipMismatch,
    )?;
    let mut wrong_m = original.clone();
    wrong_m.get_mut(s::META).ok_or("meta")?.insert(
        b"membership".to_vec(),
        control::fixtures::row(Some(f::log(1)), Some(old.as_bytes()))?,
    );
    wrong_m.get_mut(s::ROWS).ok_or("rows")?.insert(
        control_key,
        value::encode_control(scope.group(), scope.recovery(), next, f::MIB)?,
    );
    install_refused(
        &target,
        &dir.join("changed-m.redb"),
        &wrong_m,
        SnapshotError::MembershipMismatch,
    )?;
    clock_and_genesis(&dir)?;
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

fn clock_and_genesis(dir: &Path) -> TestResult {
    let path = dir.join("clocks.redb");
    let mut active = s::empty(&path)?;
    let body = control::fixtures::frame(false, control::fixtures::stamp(20, 3, 19)?)?;
    control::fixtures::apply(&mut active, &body, f::log(0))?;
    drop(active);
    let original = s::authority(&s::raw(&path)?)?;
    let scope = f::scope()?;
    let point = key::encode_control_key(scope.group(), scope.recovery(), f::MIB)?;
    for (index, (hlc, logical, wall)) in [(20, 2, 19), (21, 0, 18)].into_iter().enumerate() {
        let mut data = original.clone();
        let time = wf_types::version::CommittedTime::new(
            wf_types::version::HlcTimestamp::new(hlc, logical)?,
            wall,
        )?;
        let next = ControlState::restored(
            time,
            EntryIdentity::new(f::log(1), PreparedDigest::new([3; 32])),
            EntryStamp::Unstamped,
        )?;
        data.get_mut(s::ROWS).ok_or("rows")?.insert(
            point.clone(),
            value::encode_control(scope.group(), scope.recovery(), next, f::MIB)?,
        );
        install_refused(
            &path,
            &dir.join(format!("clock-{index}.redb")),
            &data,
            SnapshotError::StaleSnapshot,
        )?;
    }
    let mut same = original.clone();
    let next = ControlState::restored(
        control::fixtures::current(&mut f::open(&path)?)?.time(),
        EntryIdentity::new(
            wf_types::version::LogIdentity::new(2, 8, wf_types::version::AppliedPosition::new(0)),
            PreparedDigest::new([8; 32]),
        ),
        EntryStamp::Unstamped,
    )?;
    same.get_mut(s::ROWS).ok_or("rows")?.insert(
        point,
        value::encode_control(scope.group(), scope.recovery(), next, f::MIB)?,
    );
    install_refused(
        &path,
        &dir.join("equal-id.redb"),
        &same,
        SnapshotError::StaleSnapshot,
    )?;
    let source = dir.join("foreign-genesis.redb");
    let mut foreign = s::create(&source)?;
    let blank = control::fixtures::frame(false, control::fixtures::stamp(101, 0, 101)?)?;
    for index in 0..=2 {
        control::fixtures::apply(&mut foreign, &blank, f::log(index))?;
    }
    drop(foreign);
    install_refused(
        &path,
        &dir.join("genesis.redb"),
        &s::authority(&s::raw(&source)?)?,
        SnapshotError::GenesisMismatch,
    )
}
