//! Strict anchored inventory, framing, identity and continuity without repair.

use super::super::{
    LOGS, META, TestResult, assert_state, checksum, create, directory, id, open, raw_cohort,
    state_wire,
};
use super::{anchor_wire, anchored, assert_anchor};
use std::path::Path;
use wf_ports::log::{RaftLogError, RaftLogLimits};

pub(super) fn replace(path: &Path, rows: &[(Vec<u8>, Vec<u8>)]) -> TestResult {
    let database = redb::Builder::new().open(path)?;
    let mut tx = database.begin_write()?;
    tx.set_durability(redb::Durability::Immediate)?;
    {
        let mut meta = tx.open_table(META)?;
        let mut logs = tx.open_table(LOGS)?;
        meta.retain(|_, _| false)?;
        logs.retain(|_, _| false)?;
        for (key, value) in rows {
            if key[0] == b'M' {
                meta.insert(&key[1..], value.as_slice())?;
            } else {
                let index = u64::from_be_bytes(key[1..].try_into()?);
                logs.insert(index, value.as_slice())?;
            }
        }
    }
    tx.commit()?;
    Ok(())
}

fn changed_state(
    rows: &[(Vec<u8>, Vec<u8>)],
    prefix: &[u8],
) -> TestResult<Vec<(Vec<u8>, Vec<u8>)>> {
    let mut result = rows.to_vec();
    let mut wire = prefix.to_vec();
    wire.extend_from_slice(&checksum(b"g", b"z", prefix)?);
    result
        .iter_mut()
        .find(|(k, _)| k == b"Mstate")
        .ok_or("state missing")?
        .1 = wire;
    Ok(result)
}

fn refused(path: &Path, rows: &[(Vec<u8>, Vec<u8>)]) -> TestResult {
    drop(create(path, RaftLogLimits::default())?);
    replace(path, rows)?;
    let error = open(path, RaftLogLimits::default())
        .err()
        .ok_or("bad anchor accepted")?;
    assert_eq!(
        error.downcast_ref::<RaftLogError>(),
        Some(&RaftLogError::Corrupt)
    );
    assert_eq!(raw_cohort(path)?, rows);
    // A second open observes the same refusal, rather than silently repairing.
    assert!(open(path, RaftLogLimits::default()).is_err());
    Ok(())
}

#[test]
fn strict_reopen_refuses_rehashed_version_flag_and_anchor_shape_errors() -> TestResult {
    let dir = directory("purge-strict-format")?;
    let p = id(1, 7, 1);
    let specs = [(id(1, 7, 2), b"two".as_slice())];
    let good = anchored(&specs, None, id(1, 7, 2), p)?;
    let full = anchor_wire(None, id(1, 7, 2), id(1, 7, 2), p)?;
    let mut cases = Vec::new();
    for (version, flags) in [(1, 14), (2, 6), (2, 8), (2, 10), (2, 12), (2, 30), (3, 14)] {
        let mut prefix = full[..111].to_vec();
        prefix[5] = version;
        prefix[6] = flags;
        cases.push(changed_state(&good, &prefix)?);
    }
    let old = state_wire(b"g", b"z", None, Some(id(1, 7, 2)), Some(id(1, 7, 2)))?;
    let mut unanchored = old[..old.len() - 32].to_vec();
    unanchored[5] = 2;
    cases.push(changed_state(&good, &unanchored)?);
    cases.push(changed_state(&good, &full[..110])?);
    let mut extra = full[..111].to_vec();
    extra.push(0);
    cases.push(changed_state(&good, &extra)?);
    for (i, rows) in cases.iter().enumerate() {
        refused(&dir.join(format!("bad-{i}")), rows)?;
    }
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn strict_reopen_refuses_protected_rows_gaps_missing_commit_and_full_id_conflicts() -> TestResult {
    let dir = directory("purge-strict-cohort")?;
    let p = id(1, 7, 1);
    let cases = [
        anchored(
            &[(p, b"resurrected"), (id(1, 7, 2), b"two")],
            None,
            id(1, 7, 2),
            p,
        )?,
        anchored(&[(id(1, 7, 3), b"gap")], None, id(1, 7, 3), p)?,
        anchored(&[(id(1, 6, 2), b"leader-regression")], None, id(1, 6, 2), p)?,
        anchored(
            &[(id(1, 7, 2), b"two"), (id(1, 7, 4), b"gap")],
            None,
            id(1, 7, 4),
            p,
        )?,
        anchored(&[(id(1, 7, 2), b"two")], None, id(1, 7, 3), p)?,
        anchored(&[], None, id(1, 8, 1), p)?,
        anchored(&[], None, id(1, 7, 0), p)?,
    ];
    for (i, rows) in cases.iter().enumerate() {
        refused(&dir.join(format!("bad-{i}")), rows)?;
    }
    let good = anchored(&[(id(1, 7, 2), b"two")], None, p, p)?;
    let path = dir.join("good");
    drop(create(&path, RaftLogLimits::default())?);
    replace(&path, &good)?;
    let mut store = open(&path, RaftLogLimits::default())?;
    assert_state(&mut store, None, Some(p), Some(id(1, 7, 2)))?;
    assert_anchor(&mut store, p)?;
    drop(store);
    assert_eq!(raw_cohort(&path)?, good);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}
