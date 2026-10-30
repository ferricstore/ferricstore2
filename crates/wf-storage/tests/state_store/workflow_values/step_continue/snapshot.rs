use super::{fixtures as f, *};
use std::io::Cursor;
use wf_ports::snapshot::{SnapshotLimits, StateSnapshotStore};
use wf_storage::snapshot::RedbSnapshotStage;

#[test]
fn durable_step_snapshot_keeps_old_anchor_and_restores_complete_new_closure() -> TestResult {
    let directory = common::directory("step-snapshot")?;
    let source = directory.join("source");
    let target = directory.join("target");
    let mut store = values::fresh(&source)??;
    f::seed(&mut store)?;
    let mut old_wire = Vec::new();
    let old = store.capture_snapshot(&mut old_wire, SnapshotLimits::default())?;
    store.pin_snapshot(*old.id(), SnapshotLimits::default())?;
    let mut store = f::upgrade(store)?;
    let frame = f::frame(2, Some(b"FSV2raw"), b"w:0:1")?;
    let original = f::retained(f::apply(&mut store, &frame, 1)?)?;
    let current = f::record(&mut store)?;
    let mut wire = Vec::new();
    let captured = store.capture_snapshot(&mut wire, SnapshotLimits::default())?;
    assert!(wire.starts_with(b"FWSP\0\x04"));
    assert_ne!(captured.id(), old.id());
    drop(store);
    let rows = common::raw_rows(&source)?;
    refuse_profile_change(&directory, &wire, false)?;
    refuse_profile_change(&directory, &old_wire, true)?;
    let stage = RedbSnapshotStage::receive(
        &directory.join("stage"),
        common::scope()?.group().clone(),
        common::scope()?.recovery().clone(),
        &mut Cursor::new(&wire),
        StateStoreLimits::default(),
        SnapshotLimits::default(),
    )?;
    let mut restored = f::fresh(&target)?;
    let installed = restored.install_snapshot(stage, SnapshotLimits::default())?;
    assert_eq!(installed.id(), captured.id());
    assert_eq!(f::record(&mut restored)?, current);
    assert_eq!(f::history(&restored)?.entries.len(), 2);
    let duplicate = f::frame(3, Some(b"FSV2raw"), b"w:0:1")?;
    assert_eq!(
        f::retained(f::apply(&mut restored, &duplicate, 2)?)?.as_ref(),
        original.as_ref()
    );
    drop(restored);
    let mut reopened = common::open(&target)?;
    assert_eq!(f::record(&mut reopened)?, current);
    drop(reopened);
    let restored_rows = common::raw_rows(&target)?;
    for (key, bytes) in rows.iter().filter(|(key, _)| key.get(6) != Some(&0)) {
        assert_eq!(restored_rows.get(key), Some(bytes));
    }
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

fn refuse_profile_change(directory: &std::path::Path, wire: &[u8], upgraded: bool) -> TestResult {
    let path = directory.join(if upgraded {
        "refuse-downgrade"
    } else {
        "refuse-implicit-upgrade"
    });
    let fresh = values::fresh(&path)??;
    drop(if upgraded { f::upgrade(fresh)? } else { fresh });
    let rows = common::raw_rows(&path)?;
    let manifest = values::raw_manifest(&path)?;
    let stage = RedbSnapshotStage::receive(
        &directory.join(if upgraded { "old-stage" } else { "new-stage" }),
        common::scope()?.group().clone(),
        common::scope()?.recovery().clone(),
        &mut Cursor::new(wire),
        StateStoreLimits::default(),
        SnapshotLimits::default(),
    )?;
    let mut target = common::open(&path)?;
    assert!(
        target
            .install_snapshot(stage, SnapshotLimits::default())
            .is_err()
    );
    drop(target);
    assert_eq!(common::raw_rows(&path)?, rows);
    assert_eq!(values::raw_manifest(&path)?, manifest);
    Ok(())
}
