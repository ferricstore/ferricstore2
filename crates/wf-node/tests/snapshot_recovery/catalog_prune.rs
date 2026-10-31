//! Actual GC3 startup and complete guarded-prune snapshot custody.
use super::{TestResult, caps, f};
use wf_node::recovery::{
    CommittedRecoveryError as E, CommittedRecoveryLimits, purge_snapshot, recover_committed,
    recover_snapshotted, restore_received_snapshot,
};
use wf_ports::{
    log::{CodecError, PreparedBytes, RaftLogStore},
    snapshot::{SnapshotLimits, StateSnapshotStore},
    state::{
        GovernanceCatalogPruneReadPort, GovernanceCatalogReadLimits, GovernanceCatalogReadPort,
        GroupApplyStore, GroupApplyTransaction, StateTransactionError,
    },
};
use wf_storage::{log::RedbLogStore, snapshot::RedbSnapshotStage, state::RedbStateStore};
use wf_types::identity::RequestId;
mod fixtures;
use fixtures as p;

#[test]
fn catalog_prune_node_older_modes_refuse_empty_gc3_and_selected_mode_requires_actual_profile()
-> TestResult {
    let dir = f::directory("prune-node-modes")?;
    drop(p::pair(&dir, true)?);
    let before = p::cohort(&dir)?;
    let logs = f::log_rows(&dir)?;
    let scope = f::scope()?;
    for limits in [
        caps()?,
        caps()?.with_governance_catalogs(),
        caps()?.with_governance_catalog_progress(),
    ] {
        let (log, state) = f::open_pair(&dir)?;
        assert_eq!(
            recover_committed(log, state, scope.group(), scope.recovery(), limits).err(),
            Some(E::State(StateTransactionError::UnsupportedPolicyProfile))
        );
        assert_eq!(p::cohort(&dir)?, before);
        assert_eq!(f::log_rows(&dir)?, logs);
    }
    let (log, state) = f::open_pair(&dir)?;
    drop(recover_committed(
        log,
        state,
        scope.group(),
        scope.recovery(),
        p::selected()?,
    )?);
    let old = dir.join("gc2");
    std::fs::create_dir(&old)?;
    let (log, state) = p::pair(&old, false)?;
    assert_eq!(
        recover_committed(log, state, scope.group(), scope.recovery(), p::selected()?).err(),
        Some(E::State(StateTransactionError::UnsupportedPolicyProfile))
    );
    std::fs::remove_dir_all(dir)?;
    Ok(())
}
#[test]
fn catalog_prune_node_audits_complete_historical_prune_behind_valid_last_applied_body() -> TestResult
{
    let dir = f::directory("prune-node-history")?;
    let (mut log, mut state) = p::pair(&dir, true)?;
    let frames = p::frames()?;
    p::append(
        &mut log,
        &frames,
        p::PruneHistoryFault::TrailingHistoricalBody,
    )?;
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
    f::append(&mut log, 9, b"opaque uncommitted guarded prune suffix")?;
    let scope = f::scope()?;
    let stage = RedbSnapshotStage::receive(
        &dir.join("state.redb"),
        scope.group().clone(),
        scope.recovery().clone(),
        &mut std::io::Cursor::new(wire),
        wf_storage::state::StateStoreLimits::default(),
        SnapshotLimits::default(),
    )?;
    let (mut log, state) = restore_received_snapshot(
        log,
        stage,
        scope.group(),
        scope.recovery(),
        p::selected()?,
        SnapshotLimits::default(),
    )?;
    log.save_committed(f::at(8))?;
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
fn catalog_prune_node_purged_and_received_snapshots_preserve_changed_and_original_removed_results()
-> TestResult {
    let dir = f::directory("prune-node-snapshots")?;
    let source = dir.join("source");
    std::fs::create_dir(&source)?;
    let (mut log, mut state) = p::pair(&source, true)?;
    let frames = p::frames()?;
    p::append(&mut log, &frames, p::PruneHistoryFault::Healthy)?;
    p::apply(&mut state, &frames, 3)?;
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
    assert_eq!(checkpoint.control().position(), f::at(8).index());
    drop((log, state));
    let expected = f::state_rows(&source)?;
    received(&dir.join("prefix"), &prefix, &frames, &expected)?;
    received(&dir.join("checkpoint"), &complete, &frames, &expected)?;
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn catalog_prune_node_older_modes_refuse_received_gc3_with_generic_last_entry_before_log_write()
-> TestResult {
    let dir = f::directory("prune-node-received-old")?;
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
    for (index, limits) in [
        caps()?,
        caps()?.with_governance_catalogs(),
        caps()?.with_governance_catalog_progress(),
    ]
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
