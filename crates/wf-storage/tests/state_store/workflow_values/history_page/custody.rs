//! Complete physical history custody through reopen and actual FWSP4 install.

use std::io::Cursor;

use wf_ports::snapshot::{SnapshotLimits, StateSnapshotStore};
use wf_storage::snapshot::RedbSnapshotStage;

use super::*;

#[test]
fn history_page_roundtrip_reopen_and_full_snapshots_keep_rows() -> TestResult {
    let directory = crate::fixtures::directory("history-snapshot")?;
    let path = directory.join("source");
    let mut store = fresh(&path)?;
    let scope = crate::fixtures::scope()?;
    apply(
        &mut store,
        &batch(
            b"history",
            vec![
                create(b"i", Some(b"FSV2\x02bad-etf"))?,
                transition(b"i", b"q", b"a", None)?,
                transition(b"i", b"a", b"b", Some(b"FSV2\x02\x83\x61\x07"))?,
            ],
        )?,
        0,
    )?;
    let original = read(&store, &scope, b"i", query(2))?;
    assert_eq!(ids(&original), ["100-1", "100-2"]);
    assert_eq!(
        original
            .lookahead
            .as_ref()
            .ok_or("lookahead")?
            .event
            .event_id,
        "100-3"
    );
    let mut wire = Vec::new();
    let captured = store.capture_snapshot(&mut wire, SnapshotLimits::default())?;
    store.pin_snapshot(*captured.id(), SnapshotLimits::default())?;
    assert!(wire.starts_with(b"FWSP\0\x04"));
    drop(store);
    let rows = crate::fixtures::raw_rows(&path)?;
    let reopened = crate::fixtures::open(&path)?;
    let again = read(&reopened, &scope, b"i", query(2))?;
    assert_eq!(again.control, original.control);
    assert_eq!(ids(&again), ids(&original));
    drop(reopened);
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
    let restored = read(&target, &scope, b"i", query(2))?;
    assert_eq!(restored.control, original.control);
    assert_eq!(restored.record_version, Some(3));
    assert_eq!(ids(&restored), ids(&original));
    assert_eq!(restored.continuation, original.continuation);
    assert_eq!(versions(&restored.current_values)?, [Some(3), None, None]);
    for (old, new) in original
        .entries
        .iter()
        .chain(original.lookahead.iter())
        .zip(restored.entries.iter().chain(restored.lookahead.iter()))
    {
        assert_eq!(new.event, old.event);
        assert_eq!(versions(&new.values)?, versions(&old.values)?);
    }
    let mut installed = Vec::new();
    assert_eq!(
        target
            .current_snapshot(&mut installed, SnapshotLimits::default())?
            .ok_or("snapshot")?
            .id(),
        captured.id()
    );
    assert_eq!(installed, wire);
    drop(target);
    assert_eq!(crate::fixtures::raw_rows(&target_path)?, rows);
    drop(crate::fixtures::open(&target_path)?);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}
