//! Owned fields through actual reopen and complete FWSP4 receive/install.

use std::io::Cursor;

use wf_ports::snapshot::{SnapshotLimits, StateSnapshotStore};
use wf_storage::snapshot::RedbSnapshotStage;

use super::*;

fn fingerprint(page: &WorkflowHistoryValuePage) -> TestResult<Vec<(usize, &'static str, Vec<u8>)>> {
    page.writes
        .iter()
        .map(|write| {
            let bytes = match &write.value {
                HistoryValue::Decoded(doc) => [b"D".as_slice(), doc.encoded()].concat(),
                HistoryValue::Nil => b"N".to_vec(),
                HistoryValue::Boolean(value) => vec![b'B', u8::from(*value)],
                HistoryValue::StoredSize(bytes) => {
                    [b"S".as_slice(), &u64::try_from(*bytes)?.to_be_bytes()].concat()
                }
                HistoryValue::Error(error) => [b"E".as_slice(), error.as_bytes()].concat(),
            };
            Ok((write.entry_index, write.field.key(), bytes))
        })
        .collect()
}

#[test]
fn history_hydration_roundtrip_reopen_and_snapshot_keep_full_fields() -> TestResult {
    let directory = crate::fixtures::directory("hydration-snapshot")?;
    let path = directory.join("source");
    let mut store = fresh(&path)?;
    let id = b"i\0\xff";
    apply(
        &mut store,
        &batch(
            b"three",
            vec![
                create(id, Some(b"raw"))?,
                transition(id, b"q", b"a", Some(b"FSV2\x02\x83\x61\x07"))?,
                cancel(id, b"a", 2, b"FSV2\x02bad")?,
            ],
        )?,
        0,
    )?;
    let mut clock = clock(101)?;
    let request = selection(2, HistoryValueReturn::Enabled { max_bytes: 100 });
    let original = read(&store, &mut clock, id, request)?;
    assert_eq!(original.history.continuation.as_deref(), Some("100-2"));
    assert_eq!(
        original
            .history
            .lookahead
            .as_ref()
            .ok_or("peek")?
            .event
            .event_id,
        "100-3"
    );
    let expected = fingerprint(&original)?;
    let mut wire = Vec::new();
    let captured = store.capture_snapshot(&mut wire, SnapshotLimits::default())?;
    store.pin_snapshot(*captured.id(), SnapshotLimits::default())?;
    assert!(wire.starts_with(b"FWSP\0\x04"));
    drop(store);
    let rows = crate::fixtures::raw_rows(&path)?;
    let reopened = crate::fixtures::open(&path)?;
    assert_eq!(
        fingerprint(&read(&reopened, &mut clock, id, request)?)?,
        expected
    );
    drop(reopened);
    let scope = crate::fixtures::scope()?;
    let stage = RedbSnapshotStage::receive(
        &directory.join("stage"),
        scope.group().clone(),
        scope.recovery().clone(),
        &mut Cursor::new(&wire),
        StateStoreLimits::default(),
        SnapshotLimits::default(),
    )?;
    let target_path = directory.join("target");
    let mut target = fresh(&target_path)?;
    assert_eq!(
        target
            .install_snapshot(stage, SnapshotLimits::default())?
            .id(),
        captured.id()
    );
    let installed = read(&target, &mut clock, id, request)?;
    assert_eq!(installed.history.control, original.history.control);
    assert_eq!(installed.history.record_version, Some(3));
    assert_eq!(fingerprint(&installed)?, expected);
    let mut artifact = Vec::new();
    assert_eq!(
        target
            .current_snapshot(&mut artifact, SnapshotLimits::default())?
            .ok_or("artifact")?
            .id(),
        captured.id()
    );
    assert_eq!(artifact, wire);
    drop(target);
    assert_eq!(crate::fixtures::raw_rows(&target_path)?, rows);
    drop(scope);
    binary(&installed, 0, "payload", b"raw")?;
    assert!(matches!(
        document(&installed, 1, "payload")?.root(),
        Some(OriginalValueNode::Integer(7))
    ));
    assert!(matches!(
        field(&installed, 2, "error_error")?,
        HistoryValue::Error("ERR invalid flow value")
    ));
    std::fs::remove_dir_all(directory)?;
    Ok(())
}
