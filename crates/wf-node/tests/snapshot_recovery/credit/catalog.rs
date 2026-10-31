//! Explicit immutable publication capability across real log/state recovery.

use super::*;
mod ack;
use std::path::Path;
use wf_storage::{
    log::RedbLogStore,
    state::{StateStoreGenesis, StateStoreLimits},
};

fn selected() -> TestResult<CommittedRecoveryLimits> {
    Ok(caps()?.with_identified_credit_catalog_limits(4, CreditLimits::default())?)
}

fn pair(dir: &Path) -> TestResult<(RedbLogStore, RedbStateStore)> {
    let scope = f::scope()?;
    let state = RedbStateStore::create_credit_catalog_database(
        redb::Builder::new().create(dir.join("state.redb"))?,
        scope.group().clone(),
        scope.recovery().clone(),
        StateStoreGenesis {
            initial_time: f::clock(0, 0, 0)?,
            placements: &[],
            policies: &[],
        },
        c::defaults(),
        4,
        StateStoreLimits::default(),
    )?;
    Ok((c::log(dir)?, state))
}

fn publication(state: &RedbStateStore) -> TestResult {
    let page = state.read_credit_catalog(&f::scope()?, 256, 10_000, 64 * f::MIB)?;
    assert_eq!(
        (page.meta.head(), page.meta.tail(), page.more),
        (1, 1, false)
    );
    assert_eq!(page.entries.len(), 1);
    assert_eq!(
        (
            page.entries[0].sequence,
            page.entries[0].credit_scope.as_slice()
        ),
        (1, b"global".as_slice())
    );
    Ok(())
}

#[test]
fn catalog_node_replays_owner_publication_once_and_reaudits_without_new_effects() -> TestResult {
    let dir = f::directory("catalog-node-replay")?;
    let (mut log, mut state) = pair(&dir)?;
    let frames = c::frames()?;
    for (index, frame) in frames.iter().enumerate() {
        f::append(&mut log, u64::try_from(index)?, frame.as_bytes())?;
    }
    f::append(&mut log, 6, b"opaque uncommitted catalog tail")?;
    log.save_committed(f::at(5))?;
    c::apply(&mut state, &frames[0], 0)?;
    let scope = f::scope()?;
    let (log, mut state) =
        recover_committed(log, state, scope.group(), scope.recovery(), selected()?)?;
    verify(&mut state)?;
    publication(&state)?;
    drop((log, state));
    let before = c::cohort(&dir)?;
    let logs = f::log_rows(&dir)?;
    let (log, state) = f::open_pair(&dir)?;
    drop(recover_committed(
        log,
        state,
        scope.group(),
        scope.recovery(),
        selected()?,
    )?);
    assert_eq!(c::cohort(&dir)?, before);
    assert_eq!(f::log_rows(&dir)?, logs);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn catalog_node_requires_exact_format_and_count_before_even_an_empty_recovery() -> TestResult {
    let dir = f::directory("catalog-node-empty-profile")?;
    drop(pair(&dir)?);
    let before = c::cohort(&dir)?;
    let scope = f::scope()?;
    let (log, state) = f::open_pair(&dir)?;
    assert_eq!(
        recover_committed(log, state, scope.group(), scope.recovery(), limits()?).err(),
        Some(E::Credit(CreditTransactionError::State(
            StateTransactionError::InvalidChanges
        )))
    );
    assert_eq!(c::cohort(&dir)?, before);
    let (log, state) = f::open_pair(&dir)?;
    drop(recover_committed(
        log,
        state,
        scope.group(),
        scope.recovery(),
        selected()?,
    )?);
    let old = dir.join("old");
    std::fs::create_dir(&old)?;
    drop(c::pair(&old, Some(4))?);
    let before = c::cohort(&old)?;
    let (log, state) = f::open_pair(&old)?;
    assert_eq!(
        recover_committed(log, state, scope.group(), scope.recovery(), selected()?).err(),
        Some(E::Credit(CreditTransactionError::State(
            StateTransactionError::InvalidChanges
        )))
    );
    assert_eq!(c::cohort(&old)?, before);
    let (log, state) = f::open_pair(&dir)?;
    let wrong = caps()?.with_identified_credit_catalog_limits(8, CreditLimits::default())?;
    assert_eq!(
        recover_committed(log, state, scope.group(), scope.recovery(), wrong).err(),
        Some(E::Credit(CreditTransactionError::State(
            StateTransactionError::InvalidChanges
        )))
    );
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn catalog_node_full_historical_audit_rejects_trailing_body_before_replay() -> TestResult {
    let dir = f::directory("catalog-node-history")?;
    let (mut log, mut state) = pair(&dir)?;
    let frames = c::frames()?;
    for (index, frame) in frames.iter().enumerate() {
        let mut bytes = frame.as_bytes().to_vec();
        if index == 0 {
            bytes.push(0);
        }
        f::append(&mut log, u64::try_from(index)?, &bytes)?;
        c::apply(&mut state, frame, u64::try_from(index)?)?;
    }
    log.save_committed(f::at(5))?;
    drop((log, state));
    let before = c::cohort(&dir)?;
    let logs = f::log_rows(&dir)?;
    let scope = f::scope()?;
    let (log, state) = f::open_pair(&dir)?;
    assert_eq!(
        recover_committed(log, state, scope.group(), scope.recovery(), selected()?).err(),
        Some(E::Codec(CodecError::TrailingBytes))
    );
    assert_eq!(c::cohort(&dir)?, before);
    assert_eq!(f::log_rows(&dir)?, logs);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn catalog_node_snapshot_purge_and_received_suffix_preserve_the_same_pending_intent() -> TestResult
{
    let dir = f::directory("catalog-node-snapshots")?;
    let source = dir.join("source");
    std::fs::create_dir(&source)?;
    let (mut log, mut state) = pair(&source)?;
    let frames = c::frames()?;
    for (index, frame) in frames.iter().enumerate() {
        f::append(&mut log, u64::try_from(index)?, frame.as_bytes())?;
    }
    f::append(&mut log, 6, b"opaque uncommitted catalog tail")?;
    log.save_committed(f::at(5))?;
    c::apply(&mut state, &frames[0], 0)?;
    c::apply(&mut state, &frames[1], 1)?;
    let mut wire = Vec::new();
    let snapshot = state.capture_snapshot(&mut wire, SnapshotLimits::default())?;
    assert_eq!(snapshot.control().position(), f::at(1).index());
    let scope = f::scope()?;
    let (log, state) = purge_snapshot(
        log,
        state,
        scope.group(),
        scope.recovery(),
        selected()?,
        SnapshotLimits::default(),
    )?;
    let (log, mut state) = recover_snapshotted(
        log,
        state,
        scope.group(),
        scope.recovery(),
        selected()?,
        SnapshotLimits::default(),
    )?;
    verify(&mut state)?;
    publication(&state)?;
    drop((log, state));
    let expected = f::state_rows(&source)?;
    let target = dir.join("target");
    std::fs::create_dir(&target)?;
    let mut log = c::log(&target)?;
    for (index, frame) in frames.iter().enumerate() {
        f::append(&mut log, u64::try_from(index)?, frame.as_bytes())?;
    }
    f::append(&mut log, 6, b"opaque uncommitted catalog tail")?;
    let candidate = RedbSnapshotStage::receive(
        &target.join("state.redb"),
        scope.group().clone(),
        scope.recovery().clone(),
        &mut std::io::Cursor::new(&wire),
        StateStoreLimits::default(),
        SnapshotLimits::default(),
    )?;
    let (mut log, state) = restore_received_snapshot(
        log,
        candidate,
        scope.group(),
        scope.recovery(),
        selected()?,
        SnapshotLimits::default(),
    )?;
    log.save_committed(f::at(5))?;
    let (log, mut state) = recover_snapshotted(
        log,
        state,
        scope.group(),
        scope.recovery(),
        selected()?,
        SnapshotLimits::default(),
    )?;
    verify(&mut state)?;
    publication(&state)?;
    drop((log, state));
    assert_eq!(f::state_rows(&target)?, expected);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}
