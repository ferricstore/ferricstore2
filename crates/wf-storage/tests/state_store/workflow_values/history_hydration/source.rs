//! Pinned source fixtures through actual publication, hydration and reopen.
//! These conformance checks follow implementation; they are not tests-first RED.

use super::*;

#[test]
fn history_hydration_preserves_original_file_reference_size() -> TestResult {
    // HistoryValuesTest bdafcf374348156d: stored size100, return cap10.
    // The real Rust alias replaces the source helper's arbitrary "ref".
    let directory = crate::fixtures::directory("hydration-source-size")?;
    let path = directory.join("state");
    let mut store = fresh(&path)?;
    let payload = [b'x'; 100];
    apply(
        &mut store,
        &batch(b"source-size", vec![create(b"i", Some(&payload))?])?,
        0,
    )?;
    let scope = crate::fixtures::scope()?;
    let mut clock = clock(101)?;
    for reopen in [false, true] {
        if reopen {
            drop(store);
            store = RedbStateStore::open(
                &path,
                scope.group().clone(),
                scope.recovery().clone(),
                StateStoreLimits::default(),
            )?;
        }
        let page = read(
            &store,
            &mut clock,
            b"i",
            selection(10, HistoryValueReturn::Enabled { max_bytes: 10 }),
        )?;
        assert_eq!(page.history.entries.len(), 1);
        assert!(matches!(
            field(&page, 0, "payload_omitted")?,
            HistoryValue::Boolean(true)
        ));
        assert!(matches!(
            field(&page, 0, "payload_size")?,
            HistoryValue::StoredSize(100)
        ));
        assert!(
            !page
                .writes
                .iter()
                .any(|write| write.field.key() == "payload")
        );
        let full = read(
            &store,
            &mut clock,
            b"i",
            selection(10, HistoryValueReturn::Enabled { max_bytes: 100 }),
        )?;
        binary(&full, 0, "payload", &payload)?;
        assert!(matches!(
            field(&full, 0, "payload_size")?,
            HistoryValue::StoredSize(100)
        ));
    }
    drop(store);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

#[test]
fn history_hydration_preserves_original_corrupted_typed_value() -> TestResult {
    // HistoryValuesTest381f48c9d21d1d52: the exact source bytes and cap1024.
    let directory = crate::fixtures::directory("hydration-source-corrupt")?;
    let path = directory.join("state");
    let mut store = fresh(&path)?;
    let corrupted = b"FSV2\x02not-an-external-term";
    apply(
        &mut store,
        &batch(b"source-corrupt", vec![create(b"i", Some(corrupted))?])?,
        0,
    )?;
    let scope = crate::fixtures::scope()?;
    let mut clock = clock(101)?;
    for reopen in [false, true] {
        if reopen {
            drop(store);
            store = RedbStateStore::open(
                &path,
                scope.group().clone(),
                scope.recovery().clone(),
                StateStoreLimits::default(),
            )?;
        }
        let page = read(
            &store,
            &mut clock,
            b"i",
            selection(10, HistoryValueReturn::Enabled { max_bytes: 1_024 }),
        )?;
        assert_eq!(page.history.entries.len(), 1);
        assert!(matches!(
            field(&page, 0, "payload_error")?,
            HistoryValue::Error("ERR invalid flow value")
        ));
        assert!(
            !page
                .writes
                .iter()
                .any(|write| write.field.key() == "payload")
        );
        let omitted = read(
            &store,
            &mut clock,
            b"i",
            selection(10, HistoryValueReturn::Enabled { max_bytes: 10 }),
        )?;
        assert!(matches!(
            field(&omitted, 0, "payload_omitted")?,
            HistoryValue::Boolean(true)
        ));
        assert!(matches!(
            field(&omitted, 0, "payload_size")?,
            HistoryValue::StoredSize(bytes) if *bytes == corrupted.len()
        ));
        assert!(
            !omitted
                .writes
                .iter()
                .any(|write| write.field.key() == "payload_error")
        );
    }
    drop(store);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}
