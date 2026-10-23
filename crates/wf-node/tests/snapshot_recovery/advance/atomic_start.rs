//! Trusted received DATA uses matching-log audits and the actual suffix drivers.

use super::{
    RaftLogStore, SnapshotLimits, TestResult, caps, f, fixtures, received, recover_snapshotted,
};
use std::path::Path;
use wf_domain::workflow::control::{EntryIdentity, EntryStamp};
use wf_node::recovery::{install_received_snapshot, restore_received_snapshot};
use wf_storage::{log::RedbLogStore, state::RedbStateStore};

fn receive(dir: &Path, wire: &[u8], restore: bool) -> TestResult<(RedbLogStore, RedbStateStore)> {
    let (mut log, mut state) = f::create_pair(dir)?;
    let bootstrap = f::frame(5, b"b", 1, vec![(b"b", f::create_op())])?;
    f::append(&mut log, 0, bootstrap.as_bytes())?;
    drop(f::apply_normal(&mut state, 0, bootstrap.as_bytes(), 5)?);
    let atomic = f::frame(7, b"q", 17, vec![(b"i", f::atomic_start_op(b"u"))])?;
    f::append(&mut log, 1, atomic.as_bytes())?;
    log.save_committed(f::at(0))?;
    let candidate = received::stage(dir, wire)?;
    let s = f::scope()?;
    // The caller supplies accepted-snapshot authority; DATA supplies no quorum.
    if restore {
        drop(state);
        let owners = restore_received_snapshot(
            log,
            candidate,
            s.group(),
            s.recovery(),
            caps()?,
            SnapshotLimits::default(),
        )?;
        drop(owners);
        std::fs::rename(dir.join("state.redb"), dir.join("discarded-initial.redb"))?;
        std::fs::rename(dir.join("candidate.redb"), dir.join("state.redb"))?;
        f::open_pair(dir)
    } else {
        state = state.upgrade_snapshot_layout(SnapshotLimits::default())?;
        Ok(install_received_snapshot(
            log,
            state,
            candidate,
            s.group(),
            s.recovery(),
            caps()?,
            SnapshotLimits::default(),
        )?)
    }
}

#[test]
fn atomic_start_received_snapshot_matching_and_restart_use_real_drivers() -> TestResult {
    let source = f::directory("atomic-received-source")?;
    let wire = super::super::atomic_start::seed(&source, false)?;
    let source_rows = super::super::fixtures::cohort(&source)?;
    let id = fixtures::facts(&wire)?.0;
    let s = f::scope()?;
    for restore in [false, true] {
        let reference = f::directory("atomic-received-reference")?;
        let (mut log, mut state) = receive(&reference, &wire, restore)?;
        assert_eq!(fixtures::by_id(&mut state, id)?, wire);
        drop(f::atomic_original(&mut state)?);
        let frames = [
            (5, f::atomic_complete_frame(&mut state)?),
            (
                7,
                f::frame(7, b"q", 18, vec![(b"i", f::atomic_start_op(b"u"))])?,
            ),
            (0, f::blank(EntryStamp::Unstamped)?),
        ];
        for (ordinal, (profile, frame)) in frames.iter().enumerate() {
            let at = 2 + u64::try_from(ordinal)?;
            f::append(&mut log, at, frame.as_bytes())?;
            if *profile == 0 {
                f::apply_control(&mut state, at, frame.as_bytes())?;
            } else {
                drop(f::apply_normal(&mut state, at, frame.as_bytes(), *profile)?);
            }
        }
        f::append(&mut log, 5, b"uncommitted received atomic tail")?;
        log.save_committed(f::at(4))?;
        drop((log, state));
        let expected = fixtures::cohort(&reference)?;
        let logs = (f::log_rows(&reference)?, f::log_meta(&reference)?);
        let dir = f::directory("atomic-received-replay")?;
        let (mut log, state) = receive(&dir, &wire, restore)?;
        for (ordinal, (_, frame)) in frames.iter().enumerate() {
            f::append(&mut log, 2 + u64::try_from(ordinal)?, frame.as_bytes())?;
        }
        f::append(&mut log, 5, b"uncommitted received atomic tail")?;
        log.save_committed(f::at(4))?;
        drop((log, state));
        for _ in 0..2 {
            let (log, state) = f::open_pair(&dir)?;
            let (mut log, mut state) = recover_snapshotted(
                log,
                state,
                s.group(),
                s.recovery(),
                caps()?,
                SnapshotLimits::default(),
            )?;
            drop(f::atomic_original(&mut state)?);
            assert_eq!(
                f::applied(&mut state)?
                    .0
                    .last_entry()
                    .map(EntryIdentity::log),
                Some(f::at(4))
            );
            assert_eq!(log.state()?.last_purged(), Some(f::at(1)));
            assert_eq!(log.state()?.committed(), Some(f::at(4)));
            assert_eq!(log.state()?.last_log(), Some(f::at(5)));
            assert_eq!(fixtures::by_id(&mut state, id)?, wire);
            assert_eq!(fixtures::artifact(&mut state, false)?, wire);
            drop((log, state));
            assert_eq!(fixtures::cohort(&dir)?, expected);
            assert_eq!((f::log_rows(&dir)?, f::log_meta(&dir)?), logs);
            assert_eq!(super::super::fixtures::cohort(&source)?, source_rows);
        }
        std::fs::remove_dir_all(dir)?;
        std::fs::remove_dir_all(reference)?;
    }
    std::fs::remove_dir_all(source)?;
    Ok(())
}
