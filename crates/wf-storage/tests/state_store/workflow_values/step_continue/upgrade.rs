use super::{fixtures as f, *};
use wf_ports::snapshot::{SnapshotLimits, StateSnapshotStore};

#[test]
fn history_upgrade_preserves_legacy_rows_and_reopens_explicit_format() -> TestResult {
    let directory = common::directory("step-upgrade-legacy")?;
    let path = directory.join("state");
    let mut store = values::fresh(&path)??;
    f::seed(&mut store)?;
    let captured = store.capture_snapshot(&mut Vec::new(), SnapshotLimits::default())?;
    store.pin_snapshot(*captured.id(), SnapshotLimits::default())?;
    drop(store);
    let old = common::raw_rows(&path)?;
    let manifest = values::raw_manifest(&path)?;
    assert!(manifest.starts_with(b"FWSS\0\x07"));
    let store = f::upgrade(common::open(&path)?)?;
    drop(store);
    assert_eq!(common::raw_rows(&path)?, old);
    let current = values::raw_manifest(&path)?;
    assert_eq!(current.len(), manifest.len());
    assert!(current.starts_with(b"FWSS\0\x08"));
    assert_eq!(
        &current[6..current.len() - 32],
        &manifest[6..manifest.len() - 32]
    );
    let mut store = common::open(&path)?;
    assert_eq!(f::record(&mut store)?.record.version(), 1);
    let history = f::history(&store)?;
    assert_eq!(history.entries.len(), 1);
    assert_eq!(history.entries[0].event.event_id, "0-1");
    let store = f::upgrade(store)?;
    drop(store);
    assert_eq!(values::raw_manifest(&path)?, current);
    assert_eq!(common::raw_rows(&path)?, old);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

#[test]
fn history_upgrade_refuses_non_value_layout_without_rewriting() -> TestResult {
    let directory = common::directory("step-upgrade-closed")?;
    let healthy = f::fresh(&directory.join("healthy"))?;
    drop(healthy);
    let path = directory.join("legacy");
    drop(common::create(&path)?);
    let rows = common::raw_rows(&path)?;
    let manifest = common::raw_manifest(&path)?;
    assert!(matches!(
        common::open(&path)?.upgrade_history_snapshot_layout(),
        Err(StateTransactionError::UnsupportedLayout)
    ));
    assert_eq!(common::raw_rows(&path)?, rows);
    assert_eq!(common::raw_manifest(&path)?, manifest);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}
