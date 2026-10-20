//! Real engine failures and acknowledged kills at each catalog publication.

use std::{
    io::{Cursor, Write},
    path::Path,
    process::{Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicU8, Ordering},
        mpsc,
    },
    time::Duration,
};
use wf_ports::log::RaftLogState;
use wf_ports::snapshot::{SnapshotTransition, StateSnapshotTransitionStore};
use wf_ports::state::StateTransactionError;
use wf_storage::snapshot::RedbSnapshotStage;
use wf_storage::state::{RedbStateStore, StateStoreLimits};

use super::super::super::faults::{
    fault_database, parked, signal_reader, terminate, wrapped_with_cache,
};
use super::limits::acknowledged;
use super::{
    ALT_META, ALT_ROWS, RawCohort, SnapshotError, SnapshotLimits, StateSnapshotStore, TestResult,
    assert_advanced, catalog_bytes, f, manifest_version, s,
};

fn seed(path: &Path, operation: u8) -> TestResult<(SnapshotTransition, Vec<u8>)> {
    let mut state = s::create(path)?;
    s::apply(&mut state, b"target", 0, 10)?;
    let mut wire = Vec::new();
    let metadata = state.capture_snapshot(&mut wire, SnapshotLimits::default())?;
    let id = *metadata.id();
    drop(metadata);
    let before = RaftLogState::new(None, Some(f::log(0)), Some(f::log(0)));
    let t = SnapshotTransition::local(before, f::log(0), id)?;
    if operation != 0 {
        state = state.upgrade_snapshot_layout(SnapshotLimits::default())?;
    }
    if operation == 2 {
        state.prepare_snapshot_transition(t, SnapshotLimits::default())?;
    }
    drop(state);
    Ok((t, wire))
}

fn expected(
    mut before: RawCohort,
    t: SnapshotTransition,
    wire: &[u8],
    operation: u8,
) -> TestResult<RawCohort> {
    let rows = before[s::SNAP_META].len() + before[s::SNAP_ROWS].len();
    let meta = before.get_mut(s::META).ok_or("missing live metadata")?;
    if operation == 0 {
        let manifest = manifest_version(&meta[b"manifest".as_slice()], 4);
        meta.insert(b"manifest".to_vec(), manifest);
    }
    meta.insert(
        b"snapshot".to_vec(),
        catalog_bytes(
            [Some((rows, wire)), None],
            1,
            u8::from(operation == 2),
            (operation == 1).then_some(t),
        )?,
    );
    if operation == 0 {
        assert!(
            before
                .insert(ALT_META.to_owned(), std::collections::BTreeMap::default())
                .is_none()
        );
        assert!(
            before
                .insert(ALT_ROWS.to_owned(), std::collections::BTreeMap::default())
                .is_none()
        );
    }
    Ok(before)
}

fn mutate(
    state: &mut RedbStateStore,
    t: SnapshotTransition,
    operation: u8,
) -> Result<(), SnapshotError> {
    assert!((1..=2).contains(&operation), "upgrade consumes its owner");
    if operation == 1 {
        state.prepare_snapshot_transition(t, SnapshotLimits::default())
    } else {
        state.finish_snapshot_transition(acknowledged(t), SnapshotLimits::default())
    }
}

fn fenced(state: &mut RedbStateStore) {
    assert!(matches!(
        state.supports_snapshot_transitions(),
        Err(SnapshotError::State(StateTransactionError::Unavailable))
    ));
    assert!(matches!(
        state.snapshot_transition(SnapshotLimits::default()),
        Err(SnapshotError::State(StateTransactionError::Unavailable))
    ));
    let mut forbidden = Vec::new();
    assert!(matches!(
        state.current_snapshot(&mut forbidden, SnapshotLimits::default()),
        Err(SnapshotError::State(StateTransactionError::Unavailable))
    ));
    assert_eq!(forbidden, []);
}

fn reconcile(path: &Path, operation: u8, t: SnapshotTransition) -> TestResult {
    let mut state = f::open(path)?;
    match operation {
        0 => {
            state = state.upgrade_snapshot_layout(SnapshotLimits::default())?;
        }
        1 => {
            state.prepare_snapshot_transition(t, SnapshotLimits::default())?;
        }
        2 => {
            if state
                .snapshot_transition(SnapshotLimits::default())?
                .is_some()
            {
                state.finish_snapshot_transition(acknowledged(t), SnapshotLimits::default())?;
            }
            assert_eq!(
                state.snapshot_pin(SnapshotLimits::default())?,
                Some(*t.id())
            );
        }
        _ => return Err("unknown catalog operation".into()),
    }
    assert!(state.supports_snapshot_transitions()?);
    Ok(())
}

#[test]
fn upgrade_prepare_finish_disk_failures_fence_and_reopen_as_complete_cohorts() -> TestResult {
    let dir = f::directory("advance-disk-faults")?;
    for operation in 0..=2 {
        for mode in [3, 4] {
            let path = dir.join(format!("op-{operation}-mode-{mode}.redb"));
            let (t, wire) = seed(&path, operation)?;
            let before = s::raw(&path)?;
            let after = expected(before.clone(), t, &wire, operation)?;
            let fault = Arc::new(AtomicU8::new(0));
            let mut state = wrapped_with_cache(&path, Arc::clone(&fault), 0)?;
            fault.store(mode, Ordering::SeqCst);
            let error = if operation == 0 {
                state
                    .upgrade_snapshot_layout(SnapshotLimits::default())
                    .err()
                    .ok_or("faulted consuming upgrade succeeded")?
            } else {
                let error = mutate(&mut state, t, operation)
                    .err()
                    .ok_or("faulted mutation succeeded")?;
                fenced(&mut state);
                drop(state);
                error
            };
            assert_eq!(
                error,
                SnapshotError::State(if mode == 3 {
                    StateTransactionError::Unavailable
                } else {
                    StateTransactionError::Uncertain
                })
            );
            assert_eq!(
                fault.load(Ordering::SeqCst),
                0,
                "actual engine fault was reached"
            );
            let actual = s::raw(&path)?;
            if mode == 3 {
                assert_eq!(actual, before);
            } else {
                assert!(
                    actual == before || actual == after,
                    "partial uncertain cohort"
                );
            }
            reconcile(&path, operation, t)?;
            assert_advanced(&path, &after)?;
        }
    }
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn received_prepare_source_failure_consumes_stage_and_preserves_the_healthy_active_owner()
-> TestResult {
    let dir = f::directory("advance-stage-fault")?;
    let source = dir.join("source.redb");
    let (source_t, wire) = seed(&source, 0)?;
    let target = dir.join("target.redb");
    drop(s::create(&target)?.upgrade_snapshot_layout(SnapshotLimits::default())?);
    let before = s::raw(&target)?;
    let stage_path = dir.join("stage.redb");
    drop(redb::Builder::new().create(&stage_path)?);
    let fault = Arc::new(AtomicU8::new(0));
    let database = fault_database(&stage_path, Arc::clone(&fault), 0)?;
    let scope = f::scope()?;
    let candidate = RedbSnapshotStage::receive_database(
        database,
        scope.group().clone(),
        scope.recovery().clone(),
        &mut Cursor::new(&wire),
        StateStoreLimits::default(),
        SnapshotLimits::default(),
    )?;
    let t = SnapshotTransition::received(
        RaftLogState::new(None, None, None),
        f::log(0),
        *source_t.id(),
    )?;
    let mut state = f::open(&target)?;
    fault.store(3, Ordering::SeqCst);
    assert!(matches!(
        state.prepare_received_snapshot(candidate, t, SnapshotLimits::default()),
        Err(SnapshotError::Stage(StateTransactionError::Unavailable))
    ));
    assert_eq!(fault.load(Ordering::SeqCst), 0);
    assert!(state.supports_snapshot_transitions()?);
    assert_eq!(state.snapshot_transition(SnapshotLimits::default())?, None);
    assert_eq!(state.snapshot_pin(SnapshotLimits::default())?, None);
    drop(state);
    assert_advanced(&target, &before)?;
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn catalog_process_kills_preserve_atomic_upgrade_intent_and_finish_cohorts() -> TestResult {
    if let Some(path) = std::env::var_os("WF_ADVANCE_STORAGE_CHILD") {
        return child(Path::new(&path));
    }
    let dir = f::directory("advance-catalog-kills")?;
    for operation in 0..=2 {
        for after_return in [false, true] {
            let path = dir.join(format!("op-{operation}-{after_return}.redb"));
            let (t, wire) = seed(&path, operation)?;
            let before = s::raw(&path)?;
            let after = expected(before.clone(), t, &wire, operation)?;
            kill(&path, operation, after_return)?;
            let actual = s::raw(&path)?;
            if after_return {
                assert_eq!(actual, after);
            } else {
                assert!(
                    actual == before || actual == after,
                    "partial killed catalog"
                );
            }
            reconcile(&path, operation, t)?;
            assert_advanced(&path, &after)?;
        }
    }
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

fn child(path: &Path) -> TestResult {
    let operation: u8 = std::env::var("WF_ADVANCE_STORAGE_OP")?.parse()?;
    let after = std::env::var("WF_ADVANCE_STORAGE_AFTER")? == "1";
    let fault = Arc::new(AtomicU8::new(0));
    let mut state = wrapped_with_cache(path, Arc::clone(&fault), 0)?;
    let metadata = state
        .current_snapshot(&mut Vec::new(), SnapshotLimits::default())?
        .ok_or("child target absent")?;
    let t = SnapshotTransition::local(
        RaftLogState::new(None, Some(f::log(0)), Some(f::log(0))),
        f::log(0),
        *metadata.id(),
    )?;
    drop(metadata);
    if !after {
        fault.store(6, Ordering::SeqCst);
    }
    if operation == 0 {
        state = state.upgrade_snapshot_layout(SnapshotLimits::default())?;
    } else {
        mutate(&mut state, t, operation)?;
    }
    if !after {
        return Err("before-sync catalog child unexpectedly returned".into());
    }
    parked("after-return")?;
    drop(state);
    Ok(())
}

fn kill(path: &Path, operation: u8, after: bool) -> TestResult {
    let mut child = Command::new(std::env::current_exe()?)
        .args(["--exact", "snapshot::advance::faults::catalog_process_kills_preserve_atomic_upgrade_intent_and_finish_cohorts",
            "--nocapture", "--test-threads=1"])
        .env("WF_ADVANCE_STORAGE_CHILD", path)
        .env("WF_ADVANCE_STORAGE_OP", operation.to_string())
        .env("WF_ADVANCE_STORAGE_AFTER", if after { "1" } else { "0" })
        .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::inherit()).spawn()?;
    let result = (|| -> TestResult {
        let stdout = child.stdout.take().ok_or("catalog child stdout")?;
        let mut stdin = child.stdin.take().ok_or("catalog child stdin")?;
        let (sender, receiver) = mpsc::sync_channel(1);
        let reader = signal_reader(stdout, sender, after)?;
        let acknowledged = (|| -> TestResult {
            for phase in 0..2 {
                receiver.recv_timeout(Duration::from_secs(20))??;
                if phase == 0 {
                    stdin.write_all(b"A")?;
                    stdin.flush()?;
                }
            }
            if child.try_wait()?.is_some() {
                return Err("acknowledged catalog child exited before termination".into());
            }
            Ok(())
        })();
        let exit = terminate(&mut child);
        drop(stdin);
        let joined = reader.join();
        acknowledged?;
        assert!(!exit?.success());
        joined.map_err(|_| "catalog child reader panicked")??;
        Ok(())
    })();
    if result.is_err() {
        let _ = terminate(&mut child);
    }
    result
}
