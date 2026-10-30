use super::{fixtures as s, *};
use std::io::Cursor;
use wf_node::recovery::{install_received_snapshot, purge_snapshot, restore_received_snapshot};
use wf_ports::snapshot::{SnapshotLimits, StateSnapshotStore};
use wf_storage::snapshot::RedbSnapshotStage;
use wf_storage::state::StateStoreLimits;

#[test]
fn step_node_snapshot_receive_purge_and_suffix_preserve_complete_history() -> TestResult {
    let limits = s::limits()?;
    let dir = f::directory("step-node-snapshot")?;
    let source = dir.join("source");
    std::fs::create_dir(&source)?;
    let (wire, original, duplicate) = source_snapshot(&source, &limits)?;
    let expected = s::cohort(&source)?;
    for route in ["matching", "fresh"] {
        let target = dir.join(route);
        std::fs::create_dir(&target)?;
        received(&target, route, &wire, (&original, &duplicate), &limits)?;
        let observed = s::cohort(&target)?;
        assert_eq!(observed[1], expected[1]);
        assert_eq!(
            observed[0][b"manifest".as_slice()],
            expected[0][b"manifest".as_slice()]
        );
        assert_eq!(
            observed[0][b"origin".as_slice()],
            expected[0][b"origin".as_slice()]
        );
    }
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

fn source_snapshot(
    dir: &Path,
    limits: &Limits,
) -> TestResult<(Vec<u8>, PreparedBytes, PreparedBytes)> {
    let (mut log, mut state, _) = s::seed(dir, false, false)?;
    let mut old_wire = Vec::new();
    let old = state.capture_snapshot(&mut old_wire, SnapshotLimits::default())?;
    state.pin_snapshot(*old.id(), SnapshotLimits::default())?;
    assert!(old_wire.starts_with(b"FWSP\0\x04"));
    let state = state.upgrade_history_snapshot_layout()?;
    let original = s::frame(b"step", 200, Some(b"FSV2raw"))?;
    f::append(&mut log, 2, original.as_bytes())?;
    log.save_committed(f::at(2))?;
    let scope = f::scope()?;
    let (log, mut state) = recover_snapshotted(
        log,
        state,
        scope.group(),
        scope.recovery(),
        *limits,
        SnapshotLimits::default(),
    )?;
    assert_eq!(s::record(&mut state)?.record.version(), 2);
    let mut wire = Vec::new();
    let current = state.capture_snapshot(&mut wire, SnapshotLimits::default())?;
    assert_ne!(old.id(), current.id());
    let (mut log, state) = purge_snapshot(
        log,
        state,
        scope.group(),
        scope.recovery(),
        *limits,
        SnapshotLimits::default(),
    )?;
    assert_eq!(log.state()?.last_purged(), Some(f::at(2)));
    let duplicate = s::frame(b"step", 210, Some(b"FSV2raw"))?;
    append_suffix(&mut log, &duplicate)?;
    let (log, mut state) = recover_snapshotted(
        log,
        state,
        scope.group(),
        scope.recovery(),
        *limits,
        SnapshotLimits::default(),
    )?;
    assert_eq!(s::record(&mut state)?.record.version(), 2);
    assert_eq!(s::history(&state)?.entries.len(), 2);
    assert_eq!(
        v::outcome(&mut state, b"step")?.original_position(),
        f::at(2)
    );
    assert_eq!(v::control(&mut state)?.position(), f::at(3).index());
    drop((log, state));
    Ok((wire, original, duplicate))
}

fn received(
    dir: &Path,
    route: &str,
    wire: &[u8],
    frames: (&PreparedBytes, &PreparedBytes),
    limits: &Limits,
) -> TestResult {
    let scope = f::scope()?;
    let (mut log, state) = if route == "matching" {
        let (mut log, state, _) = s::seed(dir, false, true)?;
        f::append(&mut log, 2, frames.0.as_bytes())?;
        f::append(&mut log, 3, frames.1.as_bytes())?;
        f::append(&mut log, 4, b"uncommitted opaque tail")?;
        let candidate = stage(&dir.join("incoming.redb"), wire)?;
        install_received_snapshot(
            log,
            state,
            candidate,
            scope.group(),
            scope.recovery(),
            *limits,
            SnapshotLimits::default(),
        )?
    } else {
        assert_eq!(route, "fresh");
        let mut log = v::log(dir)?;
        std::fs::create_dir(dir.join("seed"))?;
        let (seed_log, seed_state, policy) = s::seed(&dir.join("seed"), false, true)?;
        drop((seed_log, seed_state));
        f::append(&mut log, 0, v::policy_frame(false, None, 0, 10)?.as_bytes())?;
        f::append(&mut log, 1, s::start_frame(&policy)?.as_bytes())?;
        f::append(&mut log, 2, frames.0.as_bytes())?;
        f::append(&mut log, 3, frames.1.as_bytes())?;
        f::append(&mut log, 4, b"uncommitted opaque tail")?;
        let candidate = stage(&dir.join("state.redb"), wire)?;
        restore_received_snapshot(
            log,
            candidate,
            scope.group(),
            scope.recovery(),
            *limits,
            SnapshotLimits::default(),
        )?
    };
    assert_eq!(log.state()?.committed(), Some(f::at(2)));
    log.save_committed(f::at(3))?;
    let (mut log, mut state) = recover_snapshotted(
        log,
        state,
        scope.group(),
        scope.recovery(),
        *limits,
        SnapshotLimits::default(),
    )?;
    assert_eq!(log.state()?.last_log(), Some(f::at(4)));
    assert_eq!(s::record(&mut state)?.record.version(), 2);
    assert_eq!(s::history(&state)?.entries.len(), 2);
    assert_eq!(
        v::outcome(&mut state, b"step")?.original_position(),
        f::at(2)
    );
    assert_eq!(v::control(&mut state)?.position(), f::at(3).index());
    drop((log, state));
    Ok(())
}

fn append_suffix(log: &mut RedbLogStore, duplicate: &PreparedBytes) -> TestResult {
    f::append(log, 3, duplicate.as_bytes())?;
    f::append(log, 4, b"uncommitted opaque tail")?;
    log.save_committed(f::at(3))?;
    Ok(())
}

fn stage(path: &Path, wire: &[u8]) -> TestResult<RedbSnapshotStage> {
    let scope = f::scope()?;
    Ok(RedbSnapshotStage::receive(
        path,
        scope.group().clone(),
        scope.recovery().clone(),
        &mut Cursor::new(wire),
        StateStoreLimits::default(),
        SnapshotLimits::default(),
    )?)
}
