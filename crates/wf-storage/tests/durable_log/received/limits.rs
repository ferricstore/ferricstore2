//! Exact reached-row census distinguishes matched prefix from discarded branch.

use super::super::{
    MIB, TestResult, assert_entries, create, directory, entries, id, open, raw_cohort,
};
use super::{ARTIFACT, boundary, digest, expected};
use wf_ports::log::{RaftLogError, RaftLogLimits, RaftLogStore};

const BLANK: &[u8] = b"FWBL\0\x01\0\x01g\0\x01z\0";

fn profile(count: usize, bytes: usize) -> TestResult<RaftLogLimits> {
    Ok(RaftLogLimits::new(
        13,
        (10, 5250),
        (10, 5250),
        (count, bytes),
        MIB,
    )?)
}

#[test]
fn matching_prefix_pays_three_full_rows_and_never_the_unrelated_tail() -> TestResult {
    let dir = directory("received-prefix-caps")?;
    let specs: Vec<_> = (0..10).map(|i| (id(1, 7, i), BLANK)).collect();
    for (label, count, bytes, accepted) in [
        ("exact", 3, 249, true),
        ("count-tight", 2, 249, false),
        ("wire-tight", 3, 248, false),
    ] {
        let path = dir.join(label);
        let mut store = create(&path, profile(count, bytes)?)?;
        store.append(entries(&specs))?;
        store.save_committed(specs[0].0)?;
        let old = store.state()?;
        let result = store.install_snapshot_boundary(old, specs[1].0, digest(BLANK), ARTIFACT);
        if accepted {
            result?;
            boundary(&mut store, specs[1].0, specs[9].0, None)?;
            assert_entries(&mut store, &specs[2..])?;
            // Retry is scalar even though eight retained rows exceed cap3.
            store.install_snapshot_boundary(old, specs[1].0, digest(BLANK), ARTIFACT)?;
        } else {
            assert!(matches!(result, Err(RaftLogError::Limit)));
            assert_eq!(store.state()?, old);
            assert_entries(&mut store, &specs)?;
        }
        drop(store);
        let whole = if accepted {
            expected(&specs[2..], None, specs[1].0)?
        } else {
            super::super::expected_cohort(b"g", b"z", &specs, None, Some(specs[0].0))?
        };
        assert_eq!(raw_cohort(&path)?, whole);
        drop(open(&path, RaftLogLimits::default())?);
    }
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn conflict_and_beyond_tail_pay_all_deleted_rows_before_publication() -> TestResult {
    let dir = directory("received-all-caps")?;
    let specs: Vec<_> = (0..10).map(|i| (id(1, 7, i), BLANK)).collect();
    for index in [1, 20] {
        let target = id(2, 9, index);
        for (label, count, bytes, accepted) in [
            ("exact", 10, 830, true),
            ("count-tight", 9, 830, false),
            ("wire-tight", 10, 829, false),
        ] {
            let path = dir.join(format!("{index}-{label}"));
            let mut store = create(&path, profile(count, bytes)?)?;
            store.append(entries(&specs))?;
            store.save_committed(specs[0].0)?;
            let old = store.state()?;
            let result = store.install_snapshot_boundary(old, target, digest(b"remote"), ARTIFACT);
            if accepted {
                result?;
                boundary(&mut store, target, target, None)?;
                assert_entries(&mut store, &[])?;
            } else {
                assert!(matches!(result, Err(RaftLogError::Limit)));
                assert_eq!(store.state()?, old);
                assert_entries(&mut store, &specs)?;
            }
            drop(store);
            let whole = if accepted {
                expected(&[], None, target)?
            } else {
                super::super::expected_cohort(b"g", b"z", &specs, None, Some(specs[0].0))?
            };
            assert_eq!(raw_cohort(&path)?, whole);
            drop(open(&path, RaftLogLimits::default())?);
        }
    }
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn stale_metadata_conflict_precedes_a_census_that_cannot_fit() -> TestResult {
    let dir = directory("received-cas-before-census")?;
    let path = dir.join("log.redb");
    let specs: Vec<_> = (0..10).map(|i| (id(1, 7, i), BLANK)).collect();
    let mut store = create(&path, profile(1, 83)?)?;
    store.append(entries(&specs))?;
    store.save_committed(specs[0].0)?;
    let old = store.state()?;
    store.save_vote(wf_ports::log::RaftVote::new(3, 8, true))?;
    let current = store.state()?;
    assert!(matches!(
        store.install_snapshot_boundary(old, specs[1].0, digest(BLANK), ARTIFACT),
        Err(RaftLogError::Conflict)
    ));
    assert_eq!(store.state()?, current);
    drop(store);
    assert_eq!(
        raw_cohort(&path)?,
        super::super::expected_cohort(b"g", b"z", &specs, Some((3, 8, true)), Some(specs[0].0))?
    );
    drop(open(&path, RaftLogLimits::default())?);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}
