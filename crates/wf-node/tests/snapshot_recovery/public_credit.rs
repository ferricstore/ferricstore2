//! Explicit public recovery, complete history and original retained snapshots.
use super::{TestResult, caps, f};
use wf_domain::governance::credit::CreditLimits;
use wf_node::recovery::{
    CommittedRecoveryError as E, CommittedRecoveryLimits, purge_snapshot, recover_committed,
    recover_snapshotted, restore_received_snapshot,
};
use wf_ports::{
    log::{CodecError, RaftLogStore},
    snapshot::{SnapshotLimits, StateSnapshotStore},
    state::{GroupApplyStore, GroupApplyTransaction},
};
use wf_storage::{log::RedbLogStore, snapshot::RedbSnapshotStage, state::RedbStateStore};
mod application;
mod fixtures;
use fixtures as p;
#[test]
fn public_credit_node_requires_explicit_selection_and_replays_original_caller_receipts()
-> TestResult {
    let dir = f::directory("public-credit-node")?;
    let (mut log, state) = p::pair(&dir)?;
    let frames = p::frames()?;
    p::append(&mut log, &frames, false)?;
    drop((log, state));
    let before = p::cohort(&dir)?;
    let logs = f::log_rows(&dir)?;
    let scope = f::scope()?;
    let old = caps()?
        .with_identified_credit_catalog_limits(4, CreditLimits::default())?
        .with_governance_catalog_pruning();
    let (log, state) = f::open_pair(&dir)?;
    assert_eq!(
        recover_committed(log, state, scope.group(), scope.recovery(), old).err(),
        Some(E::UnsupportedEntry)
    );
    assert_eq!(p::cohort(&dir)?, before);
    assert_eq!(f::log_rows(&dir)?, logs);
    let selected = p::selected()?;
    let (log, state) = f::open_pair(&dir)?;
    let (log, mut state) =
        recover_committed(log, state, scope.group(), scope.recovery(), selected)?;
    p::check(&mut state)?;
    drop((log, state));
    std::fs::remove_dir_all(dir)?;
    Ok(())
}
#[test]
fn public_credit_node_audits_older_complete_body_behind_a_valid_last_applied_entry() -> TestResult {
    let dir = f::directory("public-credit-history")?;
    let (mut log, mut state) = p::pair(&dir)?;
    let frames = p::frames()?;
    p::append(&mut log, &frames, true)?;
    p::apply(&mut state, &frames, 3)?;
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
#[test]
fn public_credit_node_purged_and_received_suffixes_retain_original_ids_before_new_preparation()
-> TestResult {
    let dir = f::directory("public-credit-snapshots")?;
    let source = dir.join("source");
    std::fs::create_dir(&source)?;
    let (mut log, mut state) = p::pair(&source)?;
    let frames = p::frames()?;
    p::append(&mut log, &frames, false)?;
    p::apply(&mut state, &frames, 2)?;
    let mut wire = Vec::new();
    state.capture_snapshot(&mut wire, SnapshotLimits::default())?;
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
    drop((log, state));
    let expected = f::state_rows(&source)?;
    let target = dir.join("received");
    std::fs::create_dir(&target)?;
    let mut log = p::log(&target)?;
    for (i, frame) in frames.iter().enumerate() {
        f::append(&mut log, u64::try_from(i)?, frame.as_bytes())?;
    }
    f::append(&mut log, 3, b"opaque uncommitted public-credit suffix")?;
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
    log.save_committed(f::at(2))?;
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
    assert_eq!(f::state_rows(&target)?, expected);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}
