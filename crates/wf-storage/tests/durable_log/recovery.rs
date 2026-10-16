//! Correctly checksummed hostile cohorts and healthy stores under tighter profiles.

use std::io::{Seek, SeekFrom, Write};
use std::path::Path;

use redb::{MultimapTableDefinition, ReadableDatabase, TableDefinition};
use wf_ports::log::{RaftLogError, RaftLogLimits, RaftLogStore, RaftVote};
use wf_storage::log::RedbLogStore;
use wf_types::identity::{GroupId, RecoveryId};

use super::{
    LOGS, META, MIB, TestResult, checksum, create, directory, entries, entry_wire, expected_cohort,
    id, manifest, open, raw_cohort, state_wire,
};

fn hex(value: &str) -> TestResult<Vec<u8>> {
    value
        .as_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| Ok(u8::from_str_radix(std::str::from_utf8(pair)?, 16)?))
        .collect()
}

fn literal_frames() -> TestResult {
    // Packed independently with Python struct + hashlib before backend source.
    assert_eq!(
        manifest(b"g", b"z")?,
        hex(
            "4657524d000100016700017ae1ca12ef4f79722086a5f689caa3f4412c97202bdf5a1a5a03fd17912452d2bf"
        )?
    );
    assert_eq!(
        state_wire(b"g", b"z", None, None, None)?,
        hex("46574c530001008b1a6d032f00cfed65169b9c14c18a369d84fbab9c11a50c49f9f179c16124e5")?
    );
    assert_eq!(
        entry_wire(b"g", b"z", id(1, 7, 0), b"a\0\xff")?,
        hex(
            "4657524c000100000000000000010000000000000007000000000000000000000000000000036100ff1a07d0884a87de2615786e18e72f09bcd9560c3b046ba6da59a9b913aa1f5cce"
        )?
    );
    Ok(())
}

fn seeded(path: &Path) -> TestResult {
    let mut store = create(path, RaftLogLimits::default())?;
    store.append(entries(&[(id(1, 7, 0), b"abc"), (id(1, 7, 1), b"def")]))?;
    store.save_vote(RaftVote::new(1, 7, true))?;
    store.save_committed(id(1, 7, 0))?;
    Ok(())
}

fn resign(wire: &mut [u8]) -> TestResult {
    let end = wire.len().checked_sub(32).ok_or("short fixture")?;
    let digest = checksum(b"g", b"z", &wire[..end])?;
    wire[end..].copy_from_slice(&digest);
    Ok(())
}

fn corrupt_metadata(path: &Path, kind: u8) -> TestResult {
    let database = redb::Builder::new().set_cache_size(16 * MIB).open(path)?;
    let tx = database.begin_write()?;
    match kind {
        0 | 1 => {
            tx.open_table(META)?.remove(if kind == 0 {
                b"manifest".as_slice()
            } else {
                b"state"
            })?;
        }
        2 => {
            tx.delete_table(LOGS)?;
        }
        3 => {
            tx.open_table(META)?
                .insert(b"extra".as_slice(), b"unknown".as_slice())?;
        }
        4 | 5 => {
            let mut wire = manifest(b"g", b"z")?;
            if kind == 4 {
                wire.push(0);
            } else {
                wire[43] ^= 1;
            }
            tx.open_table(META)?
                .insert(b"manifest".as_slice(), wire.as_slice())?;
        }
        6 | 7 | 13 | 14 => {
            let committed = if kind == 14 { id(2, 7, 0) } else { id(1, 7, 0) };
            let last = if kind == 13 { id(1, 7, 0) } else { id(1, 7, 1) };
            let mut wire = state_wire(b"g", b"z", Some((1, 7, true)), Some(committed), Some(last))?;
            if kind == 6 {
                wire[6] |= 0x80;
            }
            if kind == 7 {
                wire[23] = 2;
            }
            resign(&mut wire)?;
            tx.open_table(META)?
                .insert(b"state".as_slice(), wire.as_slice())?;
        }
        16 => {
            let extra: TableDefinition<'static, &[u8], &[u8]> = TableDefinition::new("foreign");
            tx.open_table(extra)?;
        }
        17 => {
            tx.delete_table(META)?;
        }
        20 => {
            let extra: MultimapTableDefinition<'static, &[u8], &[u8]> =
                MultimapTableDefinition::new("foreign_multi");
            tx.open_multimap_table(extra)?;
        }
        _ => return Err("unknown metadata fixture".into()),
    }
    tx.commit()?;
    Ok(())
}

fn corrupt_entry(path: &Path, kind: u8) -> TestResult {
    let database = redb::Builder::new().set_cache_size(16 * MIB).open(path)?;
    let tx = database.begin_write()?;
    if kind == 12 {
        tx.open_table(LOGS)?.remove(0)?;
    } else if kind == 15 || kind == 19 {
        // Full valid SHA and coherent last; only leader order regresses.
        let identity = if kind == 15 {
            id(1, 6, 1)
        } else {
            id(0, 99, 1)
        };
        let wire = entry_wire(b"g", b"z", identity, b"def")?;
        tx.open_table(LOGS)?.insert(1, wire.as_slice())?;
        let state = state_wire(
            b"g",
            b"z",
            Some((1, 7, true)),
            Some(id(1, 7, 0)),
            Some(identity),
        )?;
        tx.open_table(META)?
            .insert(b"state".as_slice(), state.as_slice())?;
    } else {
        let identity = if kind == 8 { id(1, 7, 1) } else { id(1, 7, 0) };
        let mut wire = entry_wire(b"g", b"z", identity, b"abc")?;
        match kind {
            8 => {}
            9 => {
                wire[30..38].copy_from_slice(&4_u64.to_be_bytes());
                resign(&mut wire)?;
            }
            10 => {
                wire[72] ^= 1;
            }
            11 => {
                wire.insert(41, 0);
                resign(&mut wire)?;
            }
            18 => {
                wire[5] = 2;
                resign(&mut wire)?;
            }
            _ => return Err("unknown entry fixture".into()),
        }
        tx.open_table(LOGS)?.insert(0, wire.as_slice())?;
    }
    tx.commit()?;
    Ok(())
}

fn malformed_cohorts(dir: &Path) -> TestResult {
    for kind in 0..21 {
        let path = dir.join(format!("corrupt-{kind}"));
        seeded(&path)?;
        if kind <= 7 || [13, 14, 16, 17, 20].contains(&kind) {
            corrupt_metadata(&path, kind)?;
        } else {
            corrupt_entry(&path, kind)?;
        }
        assert!(
            matches!(open(&path, RaftLogLimits::default()), Err(error) if error.downcast_ref::<RaftLogError>() == Some(&RaftLogError::Corrupt)),
            "fixture {kind}"
        );
    }
    Ok(())
}

fn namespaces(dir: &Path) -> TestResult {
    let path = dir.join("scope");
    let mut store = RedbLogStore::create(
        &path,
        GroupId::new(b"a".to_vec())?,
        RecoveryId::new(b"bc".to_vec())?,
        RaftLogLimits::default(),
    )?;
    store.append(entries(&[(id(1, 7, 0), b"abc")]))?;
    drop(store);
    assert!(matches!(
        RedbLogStore::open(
            &path,
            GroupId::new(b"ab".to_vec())?,
            RecoveryId::new(b"c".to_vec())?,
            RaftLogLimits::default()
        ),
        Err(RaftLogError::Conflict)
    ));
    assert!(matches!(
        RedbLogStore::create(
            &path,
            GroupId::new(b"a".to_vec())?,
            RecoveryId::new(b"bc".to_vec())?,
            RaftLogLimits::default()
        ),
        Err(RaftLogError::Conflict)
    ));
    assert_eq!(
        raw_cohort(&path)?,
        expected_cohort(b"a", b"bc", &[(id(1, 7, 0), b"abc")], None, None)?
    );
    let max_path = dir.join("max-scope");
    let group = vec![0xff; 256];
    let recovery = vec![0; 256];
    let mut store = RedbLogStore::create(
        &max_path,
        GroupId::new(group.clone())?,
        RecoveryId::new(recovery.clone())?,
        RaftLogLimits::default(),
    )?;
    store.append(entries(&[(id(1, 7, 0), b"abc")]))?;
    drop(store);
    let store = RedbLogStore::open(
        &max_path,
        GroupId::new(group.clone())?,
        RecoveryId::new(recovery.clone())?,
        RaftLogLimits::default(),
    )?;
    drop(store);
    assert_eq!(
        raw_cohort(&max_path)?,
        expected_cohort(&group, &recovery, &[(id(1, 7, 0), b"abc")], None, None)?
    );
    Ok(())
}

fn bootstrap(dir: &Path) -> TestResult {
    let empty_path = dir.join("empty-engine");
    let database = redb::Builder::new()
        .set_cache_size(16 * MIB)
        .create(&empty_path)?;
    assert!(matches!(
        RedbLogStore::open_database(
            database,
            GroupId::new(b"g".to_vec())?,
            RecoveryId::new(b"z".to_vec())?,
            RaftLogLimits::default()
        ),
        Err(RaftLogError::Corrupt)
    ));
    let database = redb::Builder::new()
        .set_cache_size(16 * MIB)
        .open(&empty_path)?;
    assert_eq!(database.begin_read()?.list_tables()?.count(), 0);
    assert_eq!(database.begin_read()?.list_multimap_tables()?.count(), 0);
    let store = RedbLogStore::create_database(
        database,
        GroupId::new(b"g".to_vec())?,
        RecoveryId::new(b"z".to_vec())?,
        RaftLogLimits::default(),
    )?;
    drop(store);
    assert_eq!(
        raw_cohort(&empty_path)?,
        expected_cohort(b"g", b"z", &[], None, None)?
    );
    foreign_bootstrap(dir)?;
    Ok(())
}

fn foreign_bootstrap(dir: &Path) -> TestResult {
    for multimap in [false, true] {
        let foreign_path = dir.join(if multimap {
            "foreign-multimap"
        } else {
            "foreign-only"
        });
        let database = redb::Builder::new()
            .set_cache_size(16 * MIB)
            .create(&foreign_path)?;
        let tx = database.begin_write()?;
        if multimap {
            let foreign: MultimapTableDefinition<'static, &[u8], &[u8]> =
                MultimapTableDefinition::new("foreign");
            tx.open_multimap_table(foreign)?;
        } else {
            let foreign: TableDefinition<'static, &[u8], &[u8]> = TableDefinition::new("foreign");
            tx.open_table(foreign)?;
        }
        tx.commit()?;
        assert!(matches!(
            RedbLogStore::create_database(
                database,
                GroupId::new(b"g".to_vec())?,
                RecoveryId::new(b"z".to_vec())?,
                RaftLogLimits::default()
            ),
            Err(RaftLogError::Conflict)
        ));
        let database = redb::Builder::new()
            .set_cache_size(16 * MIB)
            .open(&foreign_path)?;
        assert_eq!(
            database.begin_read()?.list_tables()?.count(),
            usize::from(!multimap)
        );
        assert_eq!(
            database.begin_read()?.list_multimap_tables()?.count(),
            usize::from(multimap)
        );
    }
    Ok(())
}

fn recovery_caps(dir: &Path) -> TestResult {
    let path = dir.join("healthy-limits");
    seeded(&path)?;
    let expected = raw_cohort(&path)?;
    for (entry, count, bytes, work) in [
        (2, 2, 146, 10_000),
        (3, 1, 146, 10_000),
        (3, 2, 145, 10_000),
        (3, 2, 146, 2_561),
    ] {
        let profile = RaftLogLimits::new(entry, (2, 1_030), (2, 1_030), (count, bytes), work)?;
        assert!(
            matches!(open(&path, profile), Err(error) if error.downcast_ref::<RaftLogError>() == Some(&RaftLogError::Limit))
        );
        assert_eq!(raw_cohort(&path)?, expected);
    }
    let profile = RaftLogLimits::new(3, (2, 1_030), (2, 1_030), (2, 146), 2_562)?;
    let mut store = open(&path, profile)?;
    assert_eq!(store.state()?.last_log(), Some(id(1, 7, 1)));
    drop(store);
    let suffix_path = dir.join("truncate-cap");
    let profile = RaftLogLimits::new(3, (3, 1_545), (3, 1_545), (1, 73), 10_000)?;
    let mut store = create(&suffix_path, profile)?;
    let specs = [
        (id(1, 7, 0), b"abc".as_slice()),
        (id(1, 7, 1), b"def"),
        (id(1, 7, 2), b"ghi"),
    ];
    store.append(entries(&specs))?;
    assert!(matches!(store.truncate_from(0), Err(RaftLogError::Limit)));
    drop(store);
    assert_eq!(
        raw_cohort(&suffix_path)?,
        expected_cohort(b"g", b"z", &specs, None, None)?
    );
    Ok(())
}

fn physical_header_corruption(dir: &Path) -> TestResult {
    let path = dir.join("invalid-engine-geometry");
    seeded(&path)?;
    // Pinned redb 4.4.0: immutable region_max_data_pages is u32 at offset 20.
    // Zero is rejected as explicit StorageError::Corrupted, before repair.
    let mut file = std::fs::OpenOptions::new().write(true).open(&path)?;
    file.seek(SeekFrom::Start(20))?;
    file.write_all(&[0; 4])?;
    file.sync_all()?;
    drop(file);
    assert!(matches!(
        redb::Builder::new().set_cache_size(16 * MIB).open(&path),
        Err(redb::DatabaseError::Storage(redb::StorageError::Corrupted(
            _
        )))
    ));
    assert!(matches!(open(&path, RaftLogLimits::default()), Err(error)
        if error.downcast_ref::<RaftLogError>() == Some(&RaftLogError::Corrupt)));
    assert!(matches!(create(&path, RaftLogLimits::default()), Err(error)
        if error.downcast_ref::<RaftLogError>() == Some(&RaftLogError::Corrupt)));
    Ok(())
}

#[test]
fn log_recovery_rejects_invalid_cohorts() -> TestResult {
    literal_frames()?;
    let dir = directory("recovery")?;
    malformed_cohorts(&dir)?;
    namespaces(&dir)?;
    bootstrap(&dir)?;
    recovery_caps(&dir)?;
    physical_header_corruption(&dir)?;
    std::fs::remove_dir_all(dir)?;
    Ok(())
}
