//! Full-wire prefix and retained-neighbor work, independent of the physical tail.

use super::super::{
    MIB, TestResult, assert_entries, create, directory, entries, entry_wire, expected_cohort, id,
    open, raw_cohort,
};
use super::{SNAPSHOT, anchored, assert_anchor};
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
fn purge_admits_only_deleted_prefix_plus_one_fully_charged_neighbor() -> TestResult {
    let dir = directory("purge-prefix-budget")?;
    assert_eq!(BLANK.len(), 13);
    assert_eq!(entry_wire(b"g", b"z", id(1, 7, 0), BLANK)?.len(), 83);
    let specs: Vec<_> = (0..10).map(|i| (id(1, 7, i), BLANK)).collect();
    for (label, count, bytes, accepted) in [
        ("exact", 3, 249, true),
        ("count-tight", 2, 249, false),
        ("wire-tight", 3, 248, false),
    ] {
        let path = dir.join(label);
        // Append uses its independent generous caps. Full reopen would require
        // ten rows/830 bytes; the live purge visits only 0,1 and retained2.
        let mut store = create(&path, profile(count, bytes)?)?;
        store.append(entries(&specs))?;
        store.save_committed(id(1, 7, 9))?;
        let result = store.purge_through(id(1, 7, 1), SNAPSHOT);
        if accepted {
            result?;
            assert_anchor(&mut store, id(1, 7, 1))?;
            assert_entries(&mut store, &specs[2..])?;
        } else {
            assert!(matches!(result, Err(RaftLogError::Limit)));
            assert_eq!(store.state()?.last_purged(), None);
            assert_entries(&mut store, &specs)?;
        }
        drop(store);
        let expected = if accepted {
            anchored(&specs[2..], None, id(1, 7, 9), id(1, 7, 1))?
        } else {
            expected_cohort(b"g", b"z", &specs, None, Some(id(1, 7, 9)))?
        };
        assert_eq!(raw_cohort(&path)?, expected);
        drop(open(&path, RaftLogLimits::default())?);
    }
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn maximum_anchor_has_zero_physical_points_and_no_wrapped_successor() -> TestResult {
    let dir = directory("purge-max-anchor")?;
    let path = dir.join("log.redb");
    drop(create(&path, RaftLogLimits::default())?);
    let p = id(u64::MAX, u64::MAX, u64::MAX);
    let expected = anchored(&[], None, p, p)?;
    // Trusted preexisting adapter boundary, not a locally committed-history or
    // received-snapshot provenance proof. Exercises strict reopen at MAX.
    super::corruption::replace(&path, &expected)?;
    let mut store = open(&path, profile(1, 83)?)?;
    assert_anchor(&mut store, p)?;
    assert_eq!(store.state()?.last_log(), Some(p));
    assert!(store.read_entry(u64::MAX, 525)?.is_none());
    assert_entries(&mut store, &[])?;
    store.purge_through(p, SNAPSHOT)?;
    store.save_committed(p)?;
    store.append(Vec::new())?;
    assert!(matches!(
        store.append(entries(&[(p, b"resurrect")])),
        Err(RaftLogError::Conflict)
    ));
    assert!(matches!(
        store.truncate_from(u64::MAX),
        Err(RaftLogError::Conflict)
    ));
    drop(store);
    assert_eq!(raw_cohort(&path)?, expected);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}
