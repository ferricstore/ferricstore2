//! Paid-Claim suffix replay through actual received and purged checkpoints.
use super::*;
use super::{fixtures as p, frames as q};
use wf_node::recovery::{purge_snapshot, recover_snapshotted, restore_received_snapshot};
use wf_ports::{
    log::RaftLogStore,
    snapshot::{SnapshotLimits, StateSnapshotStore},
};
use wf_storage::{snapshot::RedbSnapshotStage, state::StateStoreLimits};

#[test]
fn release_node_received_shard_profile_mismatch_refuses_before_log_publication() -> TestResult {
    use wf_domain::workflow::control::EntryStamp;
    use wf_ports::state::StateTransactionError as S;
    let dir = f::directory("release-node-received-shards")?;
    let source = dir.join("source");
    std::fs::create_dir(&source)?;
    let (log, mut state) = p::pair(&source, true)?;
    let blank = f::blank(EntryStamp::Unstamped)?;
    f::apply_control(&mut state, 0, blank.as_bytes())?;
    let mut wire = Vec::new();
    state.capture_snapshot(&mut wire, SnapshotLimits::default())?;
    drop((log, state));
    let expected = p::cohort(&source)?;
    let target = dir.join("target");
    std::fs::create_dir(&target)?;
    let s = f::scope()?;
    let candidate = RedbSnapshotStage::receive(
        &target.join("state.redb"),
        s.group().clone(),
        s.recovery().clone(),
        &mut std::io::Cursor::new(wire),
        StateStoreLimits::default(),
        SnapshotLimits::default(),
    )?;
    let limits =
        p::selected(true)?.with_identified_credit_catalog_limits(8, CreditLimits::default())?;
    assert_eq!(
        restore_received_snapshot(
            p::log(&target)?,
            candidate,
            s.group(),
            s.recovery(),
            limits,
            SnapshotLimits::default()
        )
        .err(),
        Some(wf_node::recovery::SnapshotRecoveryError::Recovery(
            E::Credit(wf_storage::transaction::CreditTransactionError::State(
                S::InvalidChanges
            ))
        ))
    );
    assert!(f::log_rows(&target)?.is_empty());
    assert_eq!(p::cohort(&target)?, expected);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn release_node_older_modes_refuse_received_gc5_before_log_publication() -> TestResult {
    use wf_domain::workflow::control::EntryStamp;
    use wf_ports::state::StateTransactionError;
    let dir = f::directory("release-node-received-old")?;
    let source = dir.join("source");
    std::fs::create_dir(&source)?;
    let (log, mut state) = p::pair(&source, true)?;
    let blank = f::blank(EntryStamp::Unstamped)?;
    f::apply_control(&mut state, 0, blank.as_bytes())?;
    let mut wire = Vec::new();
    state.capture_snapshot(&mut wire, SnapshotLimits::default())?;
    drop((log, state));
    let expected = p::cohort(&source)?;
    let s = f::scope()?;
    for (index, limits) in [
        caps()?,
        caps()?.with_governance_catalogs(),
        caps()?.with_governance_catalog_progress(),
        caps()?.with_governance_catalog_pruning(),
        p::selected(false)?,
    ]
    .into_iter()
    .enumerate()
    {
        let child = dir.join(index.to_string());
        std::fs::create_dir(&child)?;
        let log = p::log(&child)?;
        let candidate = RedbSnapshotStage::receive(
            &child.join("state.redb"),
            s.group().clone(),
            s.recovery().clone(),
            &mut std::io::Cursor::new(&wire),
            StateStoreLimits::default(),
            SnapshotLimits::default(),
        )?;
        assert_eq!(
            restore_received_snapshot(
                log,
                candidate,
                s.group(),
                s.recovery(),
                limits,
                SnapshotLimits::default()
            )
            .err(),
            Some(wf_node::recovery::SnapshotRecoveryError::Recovery(
                E::State(StateTransactionError::UnsupportedPolicyProfile)
            ))
        );
        assert!(f::log_rows(&child)?.is_empty());
        assert_eq!(p::cohort(&child)?, expected);
    }
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn release_node_purged_and_received_snapshots_replay_paid_claim_and_exact_release() -> TestResult {
    let dir = f::directory("release-node-snapshots")?;
    let source = dir.join("source");
    std::fs::create_dir(&source)?;
    let frames = q::all()?;
    let (mut log, mut state) = p::pair(&source, true)?;
    p::append(&mut log, &frames, None)?;
    p::apply(&mut state, &frames, 4)?;
    let mut prefix = Vec::new();
    state.capture_snapshot(&mut prefix, SnapshotLimits::default())?;
    let s = f::scope()?;
    let (log, state) = purge_snapshot(
        log,
        state,
        s.group(),
        s.recovery(),
        p::selected(true)?,
        SnapshotLimits::default(),
    )?;
    let (log, mut state) = recover_snapshotted(
        log,
        state,
        s.group(),
        s.recovery(),
        p::selected(true)?,
        SnapshotLimits::default(),
    )?;
    p::check(&mut state)?;
    drop((log, state));
    let expected = p::cohort(&source)?;
    let received = dir.join("received");
    std::fs::create_dir(&received)?;
    let mut log = p::log(&received)?;
    for (index, frame) in frames.iter().enumerate() {
        f::append(&mut log, u64::try_from(index)?, frame.as_bytes())?;
    }
    f::append(&mut log, 9, b"opaque uncommitted release suffix")?;
    let candidate = RedbSnapshotStage::receive(
        &received.join("state.redb"),
        s.group().clone(),
        s.recovery().clone(),
        &mut std::io::Cursor::new(prefix),
        StateStoreLimits::default(),
        SnapshotLimits::default(),
    )?;
    let (mut log, state) = restore_received_snapshot(
        log,
        candidate,
        s.group(),
        s.recovery(),
        p::selected(true)?,
        SnapshotLimits::default(),
    )?;
    log.save_committed(f::at(8))?;
    let (log, mut state) = recover_snapshotted(
        log,
        state,
        s.group(),
        s.recovery(),
        p::selected(true)?,
        SnapshotLimits::default(),
    )?;
    p::check(&mut state)?;
    drop((log, state));
    assert_eq!(p::cohort(&received)?, expected);
    let (mut log, state) = f::open_pair(&received)?;
    assert_eq!(log.state()?.last_log(), Some(f::at(9)));
    drop((log, state));
    std::fs::remove_dir_all(dir)?;
    Ok(())
}
