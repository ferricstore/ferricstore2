use super::{TestResult, f, fixtures};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::Path;
use wf_domain::workflow::control::{ControlState, EntryIdentity, EntryStamp};
use wf_ports::state::StateTransactionError;
use wf_storage::state::{key, value};
use wf_types::version::{AppliedPosition, CommittedTime, HlcTimestamp, LogIdentity};

fn rehash(wire: &mut [u8]) {
    let at = wire.len() - 32;
    let hash = Sha256::digest(&wire[..at]);
    wire[at..].copy_from_slice(&hash);
}

fn rejected(
    path: &Path,
    rows: &BTreeMap<Vec<u8>, Vec<u8>>,
    meta: &BTreeMap<Vec<u8>, Vec<u8>>,
    error: StateTransactionError,
) -> TestResult {
    fixtures::replace(path, rows, meta)?;
    let actual = f::open(path)
        .err()
        .ok_or("damaged state unexpectedly opened")?;
    assert_eq!(actual.downcast_ref::<StateTransactionError>(), Some(&error));
    assert_eq!(f::raw_rows(path)?, *rows);
    assert_eq!(fixtures::raw_meta(path)?, *meta);
    Ok(())
}

#[test]
fn membership_recovery_requires_complete_bound_metadata() -> TestResult {
    let dir = f::directory("member-recovery")?;
    let path = dir.join("state.redb");
    let mut store = f::create(&path)?;
    let member = fixtures::frame(true, fixtures::stamp(20, 3, 19)?)?;
    fixtures::apply(&mut store, &member, f::log(0))?;
    let blank = fixtures::frame(false, fixtures::stamp(30, 0, 28)?)?;
    fixtures::apply(&mut store, &blank, f::log(1))?;
    drop(store);
    let rows = f::raw_rows(&path)?;
    let meta = fixtures::raw_meta(&path)?;
    fixtures::assert_pair(&path, &rows, &meta)?;
    for case in [
        "missing",
        "unknown-key",
        "magic",
        "version",
        "foreign",
        "declared-length",
        "checksum",
        "short",
        "trailing",
        "body",
        "blank-body",
        "ahead",
    ] {
        let mut changed = meta.clone();
        match case {
            "missing" => {
                changed.remove(fixtures::MEMBER_KEY);
            }
            "unknown-key" => {
                changed.insert(b"extra".to_vec(), vec![0]);
            }
            "blank-body" => {
                changed.insert(
                    fixtures::MEMBER_KEY.to_vec(),
                    fixtures::row(Some(f::log(0)), Some(blank.as_bytes()))?,
                );
            }
            "ahead" => {
                changed.insert(
                    fixtures::MEMBER_KEY.to_vec(),
                    fixtures::row(Some(f::log(2)), Some(member.as_bytes()))?,
                );
            }
            _ => {
                let wire = changed.get_mut(fixtures::MEMBER_KEY).ok_or("member")?;
                match case {
                    "magic" => {
                        wire[0] = 0;
                        rehash(wire);
                    }
                    "version" => {
                        wire[5] = 2;
                        rehash(wire);
                    }
                    "foreign" => {
                        wire[8] = b'f';
                        rehash(wire);
                    }
                    "declared-length" => {
                        wire[37..45].copy_from_slice(&u64::MAX.to_be_bytes());
                        rehash(wire);
                    }
                    "checksum" => {
                        let end = wire.len() - 1;
                        wire[end] ^= 1;
                    }
                    "short" => {
                        wire.pop();
                    }
                    "trailing" => {
                        wire.insert(wire.len() - 32, 0);
                        rehash(wire);
                    }
                    "body" => {
                        wire[45] = 0;
                        rehash(wire);
                    }
                    _ => return Err("unknown test mutation".into()),
                }
            }
        }
        rejected(&path, &rows, &changed, StateTransactionError::Corrupt)?;
    }
    clock_and_identity_cases(&path, &rows, &meta, &member)?;
    legacy_layout(&dir)?;
    fixtures::replace(&path, &rows, &meta)?;
    fixtures::assert_pair(&path, &rows, &meta)?;
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

fn clock_and_identity_cases(
    path: &Path,
    rows: &BTreeMap<Vec<u8>, Vec<u8>>,
    meta: &BTreeMap<Vec<u8>, Vec<u8>>,
    member: &wf_ports::log::PreparedBytes,
) -> TestResult {
    let s = f::scope()?;
    let original = fixtures::stamp(20, 3, 19)?;
    for (case, time, identity, stamp) in [
        (
            "equal-valid",
            CommittedTime::new(HlcTimestamp::new(30, 0)?, 28)?,
            f::log(0),
            original,
        ),
        (
            "wrong-term",
            CommittedTime::new(HlcTimestamp::new(30, 0)?, 28)?,
            LogIdentity::new(2, 7, AppliedPosition::new(0)),
            original,
        ),
        (
            "wrong-node",
            CommittedTime::new(HlcTimestamp::new(30, 0)?, 28)?,
            LogIdentity::new(1, 8, AppliedPosition::new(0)),
            original,
        ),
        (
            "wrong-stamp",
            CommittedTime::new(HlcTimestamp::new(30, 0)?, 28)?,
            f::log(0),
            fixtures::stamp(21, 0, 20)?,
        ),
        (
            "old-hlc",
            CommittedTime::new(HlcTimestamp::new(20, 2)?, 19)?,
            f::log(1),
            EntryStamp::Unstamped,
        ),
        (
            "old-wall",
            CommittedTime::new(HlcTimestamp::new(30, 0)?, 18)?,
            f::log(1),
            EntryStamp::Unstamped,
        ),
    ] {
        let mut changed = rows.clone();
        let control =
            ControlState::restored(time, EntryIdentity::new(identity, member.digest()), stamp)?;
        fixtures::set_control(&mut changed, control)?;
        if case == "equal-valid" {
            fixtures::replace(path, &changed, meta)?;
            fixtures::assert_pair(path, &changed, meta)?;
        } else {
            rejected(path, &changed, meta, StateTransactionError::Corrupt)?;
        }
    }
    let mut changed = rows.clone();
    fixtures::set_control(&mut changed, ControlState::initial(f::clock(0)?))?;
    rejected(path, &changed, meta, StateTransactionError::Corrupt)?;
    let mut changed = rows.clone();
    let key = key::encode_control_key(s.group(), s.recovery(), f::MIB)?;
    let control = ControlState::restored(
        f::clock(30)?,
        EntryIdentity::new(
            f::log(0),
            wf_domain::workflow::control::PreparedDigest::new([9; 32]),
        ),
        original,
    )?;
    changed.insert(
        key,
        value::encode_control(s.group(), s.recovery(), control, f::MIB)?,
    );
    rejected(path, &changed, meta, StateTransactionError::Corrupt)
}

fn legacy_layout(dir: &Path) -> TestResult {
    for version in [1, 2] {
        let path = dir.join(format!("legacy-{version}.redb"));
        drop(f::create(&path)?);
        let rows = f::raw_rows(&path)?;
        let mut meta = fixtures::raw_meta(&path)?;
        if version == 1 {
            meta.remove(fixtures::MEMBER_KEY);
        }
        meta.remove(b"snapshot".as_slice());
        let manifest = meta.get_mut(b"manifest".as_slice()).ok_or("manifest")?;
        assert_eq!(&manifest[..6], b"FWSS\0\x03");
        manifest[5] = version;
        rehash(manifest);
        let database = redb::Builder::new().open(&path)?;
        let write = database.begin_write()?;
        for name in ["wf_state_snapshot_meta_v1", "wf_state_snapshot_rows_v1"] {
            assert!(write.delete_table(redb::TableDefinition::<&[u8], &[u8]>::new(name))?);
        }
        write.commit()?;
        drop(database);
        rejected(
            &path,
            &rows,
            &meta,
            StateTransactionError::UnsupportedLayout,
        )?;
    }
    Ok(())
}
