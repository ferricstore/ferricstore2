//! Exact point ownership admission, preserving the existing page contract.

use super::{TestResult, create, directory, entries, expected_cohort, id, open, raw_cohort};
use wf_ports::log::{RaftLogError, RaftLogLimits, RaftLogStore};

#[test]
fn log_point_read_admits_before_copy() -> TestResult {
    let dir = directory("point")?;
    let path = dir.join("log.redb");
    let mut store = create(&path, RaftLogLimits::default())?;
    for index in [0, u64::MAX] {
        assert!(matches!(
            store.read_entry(index, 511),
            Err(RaftLogError::Limit)
        ));
        assert!(store.read_entry(index, 512)?.is_none());
    }
    let large = vec![0xa5; 1_048_576];
    let specs = [
        (id(1, 7, 0), b"".as_slice()),
        (id(1, 7, 1), b"a"),
        (id(1, 7, 2), large.as_slice()),
    ];
    store.append(entries(&specs))?;
    let empty = store.read_entry(0, 512)?.ok_or("missing empty body")?;
    assert_eq!(empty.id(), id(1, 7, 0));
    assert_eq!(empty.body(), b"");
    assert!(matches!(store.read_entry(1, 512), Err(RaftLogError::Limit)));
    let one = store.read_entry(1, 513)?.ok_or("missing one-byte body")?;
    assert_eq!(one.id(), id(1, 7, 1));
    assert!(matches!(
        store.read_entry(2, 512 + large.len() - 1),
        Err(RaftLogError::Limit)
    ));
    let owned = store
        .read_entry(2, 512 + large.len())?
        .ok_or("missing large body")?;
    assert!(store.read_entry(u64::MAX, 512)?.is_none());
    drop(store);
    assert_eq!(one.body(), b"a");
    assert_eq!(owned.body(), large);
    assert_eq!(
        raw_cohort(&path)?,
        expected_cohort(b"g", b"z", &specs, None, None)?
    );
    drop(open(&path, RaftLogLimits::default())?);
    // R has to cover maximal E, but actual W may independently refuse a point.
    let path = dir.join("tight.redb");
    let mut store = create(&path, RaftLogLimits::default())?;
    store.append(entries(&[(id(1, 7, 0), b"abc")]))?;
    drop(store);
    for (work, accepted) in [(3077, true), (3076, false)] {
        let limits = RaftLogLimits::new(3, (1, 515), (1, 515), (10, 730), work)?;
        let mut store = open(&path, limits)?;
        let result = store.read_entry(0, 515);
        if accepted {
            assert_eq!(result?.ok_or("missing tight body")?.body(), b"abc");
        } else {
            assert!(matches!(result, Err(RaftLogError::Limit)));
        }
        assert_eq!(store.state()?.last_log(), Some(id(1, 7, 0)));
    }
    assert_eq!(
        raw_cohort(&path)?,
        expected_cohort(b"g", b"z", &[(id(1, 7, 0), b"abc")], None, None)?
    );
    std::fs::remove_dir_all(dir)?;
    Ok(())
}
