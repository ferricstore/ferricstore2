//! Real FWSP4 routes, independent resource cuts and uncertain durable commit.

use std::io::Cursor;
use std::sync::{
    Arc,
    atomic::{AtomicU8, Ordering},
};

use wf_node::recovery::{install_received_snapshot, purge_snapshot, restore_received_snapshot};
use wf_ports::consensus::RaftMembershipLimits;
use wf_ports::snapshot::{SnapshotLimits, StateSnapshotStore};
use wf_ports::state::StateTransactionError;
use wf_storage::snapshot::RedbSnapshotStage;
use wf_storage::transaction::{RecordBatchTransactionError, WorkflowValueTransactionError};

use super::*;

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

fn append_tail(dir: &Path, suffix: &PreparedBytes, commit: bool) -> TestResult {
    let (mut log, state) = f::open_pair(dir)?;
    f::append(&mut log, 2, suffix.as_bytes())?;
    f::append(&mut log, 3, b"uncommitted opaque tail")?;
    if commit {
        log.save_committed(f::at(2))?;
    }
    drop((log, state));
    Ok(())
}

fn received(
    dir: &Path,
    fresh: bool,
    wire: &[u8],
    base: &PreparedBytes,
    suffix: &PreparedBytes,
) -> TestResult {
    let scope = f::scope()?;
    let (mut log, state) = if fresh {
        let mut log = v::log(dir)?;
        let policy = v::policy_frame(true, None, 0, 10)?;
        f::append(&mut log, 0, policy.as_bytes())?;
        f::append(&mut log, 1, base.as_bytes())?;
        f::append(&mut log, 2, suffix.as_bytes())?;
        f::append(&mut log, 3, b"uncommitted opaque tail")?;
        restore_received_snapshot(
            log,
            stage(&dir.join("state.redb"), wire)?,
            scope.group(),
            scope.recovery(),
            limits()?,
            SnapshotLimits::default(),
        )?
    } else {
        let (mut log, state, _) = seed(dir)?;
        f::append(&mut log, 1, base.as_bytes())?;
        drop((log, state));
        append_tail(dir, suffix, false)?;
        let candidate = stage(&dir.join("incoming.redb"), wire)?;
        let (log, state) = f::open_pair(dir)?;
        install_received_snapshot(
            log,
            state,
            candidate,
            scope.group(),
            scope.recovery(),
            limits()?,
            SnapshotLimits::default(),
        )?
    };
    assert_eq!(log.state()?.committed(), Some(f::at(1)));
    assert_eq!(log.state()?.last_log(), Some(f::at(3)));
    log.save_committed(f::at(2))?;
    drop((log, state));
    Ok(())
}

fn finish(
    dir: &Path,
    wire: &[u8],
    mode: u8,
    policy: &WorkflowValuePolicyWitness,
) -> TestResult<[f::Rows; 6]> {
    let scope = f::scope()?;
    let (log, state) = f::open_pair(dir)?;
    let (log, state) = purge_snapshot(
        log,
        state,
        scope.group(),
        scope.recovery(),
        limits()?,
        SnapshotLimits::default(),
    )?;
    let (mut log, mut state) = recover_snapshotted(
        log,
        state,
        scope.group(),
        scope.recovery(),
        limits()?,
        SnapshotLimits::default(),
    )?;
    assert_eq!(log.state()?.last_purged(), Some(f::at(1)));
    assert_eq!(log.state()?.committed(), Some(f::at(2)));
    assert_eq!(log.state()?.last_log(), Some(f::at(3)));
    assert_eq!(v::outcome(&mut state, b"suffix")?.intent().version(), 9);
    assert_eq!(
        v::outcome(&mut state, b"suffix")?.response_version(),
        if mode == 1 { 8 } else { u16::from(mode) }
    );
    assert_payload(&state, b"i", 1, b"FSV2\x02bad-etf")?;
    assert_payload(&state, b"tail", 1, b"FSV2\x01FSV2stored")?;
    v::binding(
        &state,
        b"tail",
        None,
        (2_u128 << 32, [Some(1), None, None], None),
        policy,
    )?;
    let mut artifact = Vec::new();
    state
        .current_snapshot(&mut artifact, SnapshotLimits::default())?
        .ok_or("snapshot artifact")?;
    assert_eq!(artifact, wire);
    drop((log, state));
    let before = v::cohort(dir)?;
    let (log, state) = f::open_pair(dir)?;
    drop(recover_snapshotted(
        log,
        state,
        scope.group(),
        scope.recovery(),
        limits()?,
        SnapshotLimits::default(),
    )?);
    assert_eq!(v::cohort(dir)?, before);
    Ok(before)
}

#[test]
fn stored_recovery_snapshot_receive_purge_and_fresh_restore() -> TestResult {
    for mode in [1, 5, 6, 7] {
        let dir = f::directory("stored-node-snapshots")?;
        let source = dir.join("source");
        std::fs::create_dir(&source)?;
        let (base, policy) = pending(&source, 5, Some(b"FSV2\x02bad-etf"), false)?;
        let result = recover(&source, &limits()?);
        assert!(
            result.is_ok(),
            "healthy stored Node replay: {:?}",
            result.as_ref().err()
        );
        let (log, mut state) = result?;
        let mut wire = Vec::new();
        let captured = state.capture_snapshot(&mut wire, SnapshotLimits::default())?;
        assert!(wire.starts_with(b"FWSP\0\x04"));
        assert_eq!(captured.control().position(), f::at(1).index());
        drop((log, state));
        let suffix = encode(
            mode,
            b"suffix",
            200,
            vec![item(mode, b"tail", 200, Some(b"FSV2\x01FSV2stored"))?],
            &policy,
        )?;
        append_tail(&source, &suffix, true)?;
        let expected = finish(&source, &wire, mode, &policy)?;
        for (route, fresh) in [("matching", false), ("fresh", true)] {
            let target = dir.join(route);
            std::fs::create_dir(&target)?;
            received(&target, fresh, &wire, &base, &suffix)?;
            let actual = finish(&target, &wire, mode, &policy)?;
            assert_eq!(actual[1], expected[1]);
            for key in [b"manifest".as_slice(), b"membership", b"origin"] {
                assert_eq!(actual[0].get(key), expected[0].get(key));
            }
        }
        std::fs::remove_dir_all(dir)?;
    }
    Ok(())
}

fn limited(case: u8) -> TestResult<Limits> {
    let normal = v::limits()?.normal_limits();
    let state = if case == 4 {
        (10_000, 1, 64 * f::MIB, 32 * f::MIB)
    } else {
        (10_000, 64 * f::MIB, 64 * f::MIB, 32 * f::MIB)
    };
    let workspace = if case == 5 { 1 } else { 1_024 * f::MIB };
    let profile = Limits::new(
        RaftMembershipLimits::default(),
        state,
        normal,
        (1_000, 64 * f::MIB),
        16 * f::MIB,
        workspace,
    )?
    .with_core_policy_patches()
    .with_original_policy_sets();
    let caps = match case {
        0 => [1, 64 * f::MIB, 64 * f::MIB, 64 * f::MIB],
        1 => [1_024, 1, 64 * f::MIB, 64 * f::MIB],
        2 => [1_024, 64 * f::MIB, 1, 64 * f::MIB],
        3 => [1_024, 64 * f::MIB, 64 * f::MIB, 1],
        _ => [1_024, 64 * f::MIB, 64 * f::MIB, 64 * f::MIB],
    };
    Ok(profile.with_workflow_stored_value_limits(caps[0], caps[1], caps[2], caps[3])?)
}

#[test]
fn stored_recovery_limits_preserve_the_complete_databases() -> TestResult {
    let healthy = f::directory("stored-node-funded")?;
    pending(&healthy, 5, Some(b"FSV2\x02raw"), false)?;
    let result = recover(&healthy, &limits()?);
    assert!(
        result.is_ok(),
        "healthy stored Node replay: {:?}",
        result.as_ref().err()
    );
    drop(result?);
    for caps in [[0, 1, 1, 1], [1, 0, 1, 1], [1, 1, 0, 1], [1, 1, 1, 0]] {
        assert!(matches!(
            v::limits()?.with_workflow_stored_value_limits(caps[0], caps[1], caps[2], caps[3]),
            Err(Error::InvalidLimits)
        ));
    }
    for case in 0..6 {
        let dir = f::directory("stored-node-limit")?;
        pending(&dir, 5, Some(b"FSV2\x02raw"), false)?;
        let expected = match case {
            0..=3 => Error::Value(WorkflowValueTransactionError::Record(
                RecordBatchTransactionError::Transaction(StateTransactionError::Limit),
            )),
            4 => Error::State(StateTransactionError::Limit),
            _ => Error::Limit,
        };
        refuse(&dir, &limited(case)?, expected)?;
        let (log, state) = recover(&dir, &limits()?)?;
        assert_payload(&state, b"i", 1, b"FSV2\x02raw")?;
        drop((log, state));
        std::fs::remove_dir_all(dir)?;
    }
    std::fs::remove_dir_all(healthy)?;
    Ok(())
}

#[test]
fn stored_recovery_uncertain_commit_keeps_a_reopenable_prefix() -> TestResult {
    let reference = f::directory("stored-node-fault-reference")?;
    pending(&reference, 5, Some(b"FSV2\x02raw"), false)?;
    let result = recover(&reference, &limits()?);
    assert!(
        result.is_ok(),
        "healthy stored Node replay: {:?}",
        result.as_ref().err()
    );
    drop(result?);
    let expected = v::cohort(&reference)?;
    let dir = f::directory("stored-node-fault-live")?;
    pending(&dir, 5, Some(b"FSV2\x02raw"), false)?;
    let before = v::cohort(&dir)?;
    assert_ne!(before, expected);
    let logs = f::log_rows(&dir)?;
    let meta = f::log_meta(&dir)?;
    let (log, state) = f::open_pair(&dir)?;
    drop(state);
    let fault = Arc::new(AtomicU8::new(0));
    let scope = f::scope()?;
    let state = RedbStateStore::open_database(
        disk_backend::fault_database(&dir.join("state.redb"), Arc::clone(&fault), 16 * f::MIB)?,
        scope.group().clone(),
        scope.recovery().clone(),
        StateStoreLimits::default(),
    )?;
    fault.store(4, Ordering::SeqCst);
    assert_eq!(
        recover_committed(log, state, scope.group(), scope.recovery(), limits()?).err(),
        Some(Error::Value(WorkflowValueTransactionError::Record(
            RecordBatchTransactionError::Transaction(StateTransactionError::Uncertain)
        )))
    );
    assert_eq!(fault.load(Ordering::SeqCst), 0);
    drop(f::open_pair(&dir)?);
    let actual = v::cohort(&dir)?;
    assert!(
        actual == before || actual == expected,
        "partial durable cohort"
    );
    assert_eq!(f::log_rows(&dir)?, logs);
    assert_eq!(f::log_meta(&dir)?, meta);
    drop(recover(&dir, &limits()?)?);
    assert_eq!(v::cohort(&dir)?, expected);
    assert_eq!(f::log_rows(&dir)?, logs);
    assert_eq!(f::log_meta(&dir)?, meta);
    std::fs::remove_dir_all(dir)?;
    std::fs::remove_dir_all(reference)?;
    Ok(())
}
