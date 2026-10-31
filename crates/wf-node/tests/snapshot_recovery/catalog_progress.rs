//! Explicit durable cleaner progress through real startup and snapshot paths.
use super::{TestResult, caps, f};
use wf_node::recovery::{
    CommittedRecoveryError as E, CommittedRecoveryLimits, purge_snapshot, recover_committed,
    recover_snapshotted, restore_received_snapshot,
};
use wf_ports::{
    log::{CodecError, PreparedBytes, RaftLogStore},
    snapshot::{SnapshotLimits, StateSnapshotStore},
    state::{
        GovernanceCatalogProgress, GovernanceCatalogProgressPort, GovernanceCatalogReadLimits,
        GroupApplyStore, GroupApplyTransaction, StateTransactionError,
    },
};
use wf_storage::{log::RedbLogStore, snapshot::RedbSnapshotStage, state::RedbStateStore};
use wf_types::identity::RequestId;
mod fixtures;
use fixtures as p;

#[test]
fn catalog_progress_node_older_modes_refuse_empty_gc2_without_changing_either_store() -> TestResult
{
    let dir = f::directory("progress-node-old-modes")?;
    let scope = f::scope()?;
    drop(p::pair(&dir, true)?);
    let before = p::cohort(&dir)?;
    let logs = f::log_rows(&dir)?;
    for limits in [caps()?, caps()?.with_governance_catalogs()] {
        let (log, state) = f::open_pair(&dir)?;
        assert_eq!(
            recover_committed(log, state, scope.group(), scope.recovery(), limits).err(),
            Some(E::State(StateTransactionError::UnsupportedPolicyProfile))
        );
        assert_eq!(p::cohort(&dir)?, before);
        assert_eq!(f::log_rows(&dir)?, logs);
    }
    std::fs::remove_dir_all(dir)?;
    Ok(())
}
#[test]
fn catalog_progress_node_requires_explicit_actual_gc2_including_empty_state() -> TestResult {
    let dir = f::directory("progress-node-selected")?;
    let scope = f::scope()?;
    for progress in [true, false] {
        let child = dir.join(if progress { "gc2" } else { "gc1" });
        std::fs::create_dir(&child)?;
        let (log, state) = p::pair(&child, progress)?;
        let result = recover_committed(log, state, scope.group(), scope.recovery(), p::selected()?);
        if progress {
            drop(result?);
        } else {
            assert_eq!(
                result.err(),
                Some(E::State(StateTransactionError::UnsupportedPolicyProfile))
            );
        }
    }
    std::fs::remove_dir_all(dir)?;
    Ok(())
}
#[test]
fn catalog_progress_node_replays_once_and_preserves_newer_cursor_and_original_receipt() -> TestResult
{
    let dir = f::directory("progress-node-replay")?;
    let (mut log, mut state) = p::pair(&dir, true)?;
    let frames = p::frames()?;
    p::append(&mut log, &frames, p::ProgressFault::Healthy)?;
    p::apply(&mut state, &frames, 2)?;
    drop((log, state));
    let logs = f::log_rows(&dir)?;
    let scope = f::scope()?;
    let (log, state) = f::open_pair(&dir)?;
    let (log, mut state) =
        recover_committed(log, state, scope.group(), scope.recovery(), p::selected()?)?;
    p::check(&mut state)?;
    drop((log, state));
    let before = p::cohort(&dir)?;
    let (log, state) = f::open_pair(&dir)?;
    drop(recover_committed(
        log,
        state,
        scope.group(),
        scope.recovery(),
        p::selected()?,
    )?);
    assert_eq!(p::cohort(&dir)?, before);
    assert_eq!(f::log_rows(&dir)?, logs);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}
#[test]
fn catalog_progress_node_audits_entire_historical_body_behind_valid_last_applied() -> TestResult {
    let dir = f::directory("progress-node-history")?;
    let (mut log, mut state) = p::pair(&dir, true)?;
    let frames = p::frames()?;
    p::append(&mut log, &frames, p::ProgressFault::TrailingHistoricalBody)?;
    p::apply(&mut state, &frames, frames.len())?;
    p::check(&mut state)?;
    drop((log, state));
    let before = p::cohort(&dir)?;
    let logs = f::log_rows(&dir)?;
    let scope = f::scope()?;
    let (log, state) = f::open_pair(&dir)?;
    assert_eq!(
        recover_committed(log, state, scope.group(), scope.recovery(), p::selected()?).err(),
        Some(E::Codec(CodecError::TrailingBytes))
    );
    assert_eq!(p::cohort(&dir)?, before);
    assert_eq!(f::log_rows(&dir)?, logs);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}
fn received(
    dir: &std::path::Path,
    wire: &[u8],
    frames: &[PreparedBytes],
    expected: &f::Rows,
) -> TestResult {
    std::fs::create_dir(dir)?;
    let mut log = p::log(dir)?;
    for (index, frame) in frames.iter().enumerate() {
        f::append(&mut log, u64::try_from(index)?, frame.as_bytes())?;
    }
    f::append(&mut log, 6, b"opaque uncommitted progress suffix")?;
    let scope = f::scope()?;
    let candidate = RedbSnapshotStage::receive(
        &dir.join("state.redb"),
        scope.group().clone(),
        scope.recovery().clone(),
        &mut std::io::Cursor::new(wire),
        wf_storage::state::StateStoreLimits::default(),
        SnapshotLimits::default(),
    )?;
    let (mut log, state) = restore_received_snapshot(
        log,
        candidate,
        scope.group(),
        scope.recovery(),
        p::selected()?,
        SnapshotLimits::default(),
    )?;
    log.save_committed(f::at(5))?;
    let (log, mut state) = recover_snapshotted(
        log,
        state,
        scope.group(),
        scope.recovery(),
        p::selected()?,
        SnapshotLimits::default(),
    )?;
    p::check(&mut state)?;
    drop((log, state));
    assert_eq!(&f::state_rows(dir)?, expected);
    Ok(())
}
#[test]
fn catalog_progress_node_purged_and_received_snapshots_keep_receipt_only_prefix_and_suffix()
-> TestResult {
    let dir = f::directory("progress-node-snapshot")?;
    let source = dir.join("source");
    std::fs::create_dir(&source)?;
    let (mut log, mut state) = p::pair(&source, true)?;
    let frames = p::frames()?;
    p::append(&mut log, &frames, p::ProgressFault::Healthy)?;
    p::apply(&mut state, &frames, 3)?;
    assert_eq!(
        state.governance_catalog_progress(
            &f::scope()?,
            b"limit",
            GovernanceCatalogReadLimits::default()
        )?,
        GovernanceCatalogProgress::Missing
    );
    let mut prefix = Vec::new();
    state.capture_snapshot(&mut prefix, SnapshotLimits::default())?;
    let scope = f::scope()?;
    let (log, state) = purge_snapshot(
        log,
        state,
        scope.group(),
        scope.recovery(),
        p::selected()?,
        SnapshotLimits::default(),
    )?;
    let (log, mut state) = recover_snapshotted(
        log,
        state,
        scope.group(),
        scope.recovery(),
        p::selected()?,
        SnapshotLimits::default(),
    )?;
    p::check(&mut state)?;
    let mut complete = Vec::new();
    let checkpoint = state.capture_snapshot(&mut complete, SnapshotLimits::default())?;
    assert_eq!(checkpoint.control().position(), f::at(5).index());
    drop((log, state));
    let expected = f::state_rows(&source)?;
    received(&dir.join("suffix"), &prefix, &frames, &expected)?;
    received(&dir.join("checkpoint"), &complete, &frames, &expected)?;
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn catalog_progress_node_older_modes_refuse_received_gc2_with_generic_last_entry_before_log_write()
-> TestResult {
    let dir = f::directory("progress-node-received-old")?;
    let source = dir.join("source");
    std::fs::create_dir(&source)?;
    let (log, mut state) = p::pair(&source, true)?;
    let blank = f::blank(wf_domain::workflow::control::EntryStamp::Unstamped)?;
    f::apply_control(&mut state, 0, blank.as_bytes())?;
    let mut wire = Vec::new();
    state.capture_snapshot(&mut wire, SnapshotLimits::default())?;
    drop((log, state));
    let state_before = p::cohort(&source)?;
    let scope = f::scope()?;
    for (index, limits) in [caps()?, caps()?.with_governance_catalogs()]
        .into_iter()
        .enumerate()
    {
        let child = dir.join(index.to_string());
        std::fs::create_dir(&child)?;
        let mut log = p::log(&child)?;
        let before = log.state()?;
        let candidate = RedbSnapshotStage::receive(
            &child.join("state.redb"),
            scope.group().clone(),
            scope.recovery().clone(),
            &mut std::io::Cursor::new(&wire),
            wf_storage::state::StateStoreLimits::default(),
            SnapshotLimits::default(),
        )?;
        assert_eq!(
            restore_received_snapshot(
                log,
                candidate,
                scope.group(),
                scope.recovery(),
                limits,
                SnapshotLimits::default()
            )
            .err(),
            Some(wf_node::recovery::SnapshotRecoveryError::Recovery(
                E::State(StateTransactionError::UnsupportedPolicyProfile)
            ))
        );
        let (mut log, state) = f::open_pair(&child)?;
        assert_eq!(log.state()?, before);
        drop((log, state));
        assert_eq!(p::cohort(&child)?, state_before);
        assert!(f::log_rows(&child)?.is_empty());
    }
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn catalog_progress_node_received_invalid_hint_is_preserved_until_committed_clear() -> TestResult {
    let dir = f::directory("progress-node-invalid-hint")?;
    let source = dir.join("source");
    std::fs::create_dir(&source)?;
    let (mut log, mut state) = p::pair(&source, true)?;
    let frames = p::frames()?;
    f::append(&mut log, 0, frames[0].as_bytes())?;
    log.save_committed(f::at(0))?;
    p::apply(&mut state, &frames, 1)?;
    drop((log, state));
    p::damage_hint(&source)?;
    let scope = f::scope()?;
    let (log, mut state) = f::open_pair(&source)?;
    assert_eq!(
        state.governance_catalog_progress(
            &scope,
            b"limit",
            GovernanceCatalogReadLimits::default()
        )?,
        GovernanceCatalogProgress::Invalid
    );
    let mut wire = Vec::new();
    state.capture_snapshot(&mut wire, SnapshotLimits::default())?;
    drop((log, state));
    let target = dir.join("target");
    std::fs::create_dir(&target)?;
    let log = p::log(&target)?;
    let candidate = RedbSnapshotStage::receive(
        &target.join("state.redb"),
        scope.group().clone(),
        scope.recovery().clone(),
        &mut std::io::Cursor::new(&wire),
        wf_storage::state::StateStoreLimits::default(),
        SnapshotLimits::default(),
    )?;
    let (mut log, state) = restore_received_snapshot(
        log,
        candidate,
        scope.group(),
        scope.recovery(),
        p::selected()?,
        SnapshotLimits::default(),
    )?;
    assert_eq!(state.governance_catalog_profile(), 2);
    assert_eq!(
        state.governance_catalog_progress(
            &scope,
            b"limit",
            GovernanceCatalogReadLimits::default()
        )?,
        GovernanceCatalogProgress::Invalid
    );
    f::append(&mut log, 1, frames[2].as_bytes())?;
    log.save_committed(f::at(1))?;
    let (log, state) = recover_snapshotted(
        log,
        state,
        scope.group(),
        scope.recovery(),
        p::selected()?,
        SnapshotLimits::default(),
    )?;
    assert_eq!(
        state.governance_catalog_progress(
            &scope,
            b"limit",
            GovernanceCatalogReadLimits::default()
        )?,
        GovernanceCatalogProgress::Missing
    );
    drop((log, state));
    let (log, state) = f::open_pair(&target)?;
    assert_eq!(
        state.governance_catalog_progress(
            &scope,
            b"limit",
            GovernanceCatalogReadLimits::default()
        )?,
        GovernanceCatalogProgress::Missing
    );
    drop((log, state));
    std::fs::remove_dir_all(dir)?;
    Ok(())
}
