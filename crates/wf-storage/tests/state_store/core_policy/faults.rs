//! Delegated file failures and acknowledged process boundaries use real redb.

use super::super::faults::{
    fault_database_counted, parked, signal_reader, terminate, wrapped_with_cache,
};
use super::{TestResult, fixtures as f};
use std::{
    io::Write,
    path::Path,
    process::{Command, Stdio},
    sync::atomic::{AtomicU8, AtomicUsize, Ordering},
    sync::{Arc, mpsc},
    time::Duration,
};
use wf_domain::policy::complete::PolicySchemaCharge;
use wf_domain::policy::definition::StateMode;
use wf_domain::workflow::request::{RetainedStatus, gate::PointGateView};
use wf_ports::log::{
    CompleteCorePolicyPatchPointInspection, inspect_complete_core_policy_patch_point,
    prepare_complete_core_policy_patch_point_frame,
};
use wf_ports::state::{
    GroupApplyStore, GroupApplyTransaction, GroupCorePolicyTransaction, StateTransactionError,
};
use wf_storage::transaction::{
    CorePolicyTransactionError, RecordBatchTransactionOutcome,
    apply_complete_core_policy_patch_transaction,
};
use wf_types::identity::{PartitionId, RecordId, RequestId};
use wf_types::version::TimestampMillis;

fn initialized(path: &Path) -> TestResult<(f::Cohort, f::Cohort, Vec<u8>, Vec<u8>)> {
    let s = f::scope()?;
    let mut store = f::create(path, &s, &[b"p"], &[])?;
    let mut data = f::data(b"head", b"job", b"q", 0, Some(0))?;
    data.payload = Some(vec![0xa5; 131_072]);
    let head = f::owner(&s, b"p", data, 0, 0)?;
    let follower = f::owner(
        &s,
        b"p",
        f::data(b"follower", b"job", b"q", 0, Some(0))?,
        0,
        1,
    )?;
    f::seed(&mut store, &s, &[head.clone(), follower.clone()], 0, 0)?;
    drop(store);
    let old = f::cohort(path)?;
    let mut new = old.clone();
    let frame = f::frame(
        &s,
        b"publish",
        25,
        b"job",
        Some(0),
        f::mode_patch(&[(b"q", Some(StateMode::Fifo))]),
    )?;
    let response = f::policy_wire(b"job", 1, &[(b"q", StateMode::Fifo)], false)?;
    for row in [&head, &follower] {
        assert_eq!(
            f::rows_mut(&mut new)?.remove(&f::due(&s, row)?),
            Some(vec![0])
        );
        f::rows_mut(&mut new)?.insert(f::member(&s, row), Vec::new());
    }
    f::rows_mut(&mut new)?.insert(f::due(&s, &head)?, vec![1]);
    f::rows_mut(&mut new)?.insert(f::key(6, &[b"g", b"z", b"t", b"job"]), response.clone());
    let metadata = f::metadata(
        &s,
        b"publish",
        1,
        25,
        f::frame_intent(frame.as_bytes(), 1)?,
        RetainedStatus::Accepted,
    )?;
    f::finish(
        &mut new,
        &s,
        frame.as_bytes(),
        f::log(1),
        (f::clock(25)?, f::clock(25)?),
        Some((&metadata, 1, &response)),
    )?;
    f::upgrade5(
        &mut new,
        &s,
        f::catalog(&s, [None, None], None, None, None)?,
    )?;
    Ok((old, new, frame.as_bytes().to_vec(), response))
}

fn fenced(store: &mut wf_storage::state::RedbStateStore, frame: &[u8]) -> TestResult {
    let s = f::scope()?;
    assert!(matches!(
        store.begin_apply(
            s.group(),
            s.recovery(),
            f::limits().codec.admission(),
            100,
            1_000_000,
            1_000_000
        ),
        Err(StateTransactionError::Unavailable)
    ));
    assert_eq!(
        apply_complete_core_policy_patch_transaction(store, &s, frame, f::log(1), f::limits()),
        Err(CorePolicyTransactionError::Transaction(
            StateTransactionError::Unavailable
        ))
    );
    Ok(())
}

fn read_faults(dir: &Path) -> TestResult {
    let s = f::scope()?;
    for after_gate in [false, true] {
        let path = dir.join(if after_gate { "cold-body" } else { "point" });
        let (old, _, frame, _) = initialized(&path)?;
        let fault = Arc::new(AtomicU8::new(0));
        let mut store = wrapped_with_cache(&path, Arc::clone(&fault), 0)?;
        if after_gate {
            sealed_cold_read(&mut store, &frame, &fault)?;
        } else {
            fault.store(3, Ordering::SeqCst);
            assert_eq!(
                apply_complete_core_policy_patch_transaction(
                    &mut store,
                    &s,
                    &frame,
                    f::log(1),
                    f::limits()
                ),
                Err(CorePolicyTransactionError::Transaction(
                    StateTransactionError::Unavailable
                ))
            );
            assert_eq!(
                fault.load(Ordering::SeqCst),
                0,
                "actual point/control file read reached"
            );
        }
        fenced(&mut store, &frame)?;
        drop(store);
        assert_eq!(f::cohort(&path)?, old);
        drop(f::open(&path, &s)?);
    }
    Ok(())
}

fn sealed_cold_read(
    store: &mut wf_storage::state::RedbStateStore,
    frame: &[u8],
    fault: &AtomicU8,
) -> TestResult {
    let s = f::scope()?;
    let caps = f::limits();
    let mut charge = PolicySchemaCharge::new(caps.schema);
    let allowance = caps.workspace_bytes
        - caps.maximum_read_bytes
        - caps.maximum_staging_bytes
        - caps.maximum_adapter_bytes;
    let verified = prepare_complete_core_policy_patch_point_frame(
        frame,
        caps.schema,
        caps.codec.admission(),
        &mut charge,
        allowance,
    )?;
    let tx = store.begin_apply(
        s.group(),
        s.recovery(),
        caps.codec.admission(),
        10_000,
        64_000_000,
        32_000_000,
    )?;
    let control = tx.control()?;
    let retained_nonempty = tx.retained_nonempty(s.tenant())?;
    let original = tx.core_policy_outcome(
        s.tenant(),
        &RequestId::new(b"publish".to_vec())?,
        caps.schema,
        &mut charge,
    )?;
    assert!(matches!(
        inspect_complete_core_policy_patch_point(
            verified,
            f::log(1),
            PointGateView {
                scope: &s,
                control,
                retained_nonempty,
                outcome: original.as_ref()
            },
            (caps.schema, caps.codec.admission()),
            &mut charge,
            allowance
        )?,
        CompleteCorePolicyPatchPointInspection::Fresh(_)
    ));
    fault.store(3, Ordering::SeqCst);
    assert_eq!(
        tx.record(
            s.tenant(),
            &PartitionId::new(b"p".to_vec())?,
            &RecordId::new(b"head".to_vec())?
        ),
        Err(StateTransactionError::Unavailable)
    );
    assert_eq!(
        fault.load(Ordering::SeqCst),
        0,
        "large cold canonical body reached delegated read"
    );
    assert_eq!(tx.control(), Err(StateTransactionError::Unavailable));
    Ok(())
}

#[test]
fn real_core_policy_io_and_commit_uncertainty_preserve_whole_cohorts() -> TestResult {
    let dir = f::directory("policy-real-faults")?;
    read_faults(&dir)?;
    let path = dir.join("commit");
    let s = f::scope()?;
    let (old, new, frame, response) = initialized(&path)?;
    let fault = Arc::new(AtomicU8::new(0));
    let mut store = wrapped_with_cache(&path, Arc::clone(&fault), 0)?;
    // Existing mode4 skips growth syncs and fails AFTER the next delegated
    // commit-path sync. It is not a final-flush or power-loss simulation.
    fault.store(4, Ordering::SeqCst);
    assert_eq!(
        apply_complete_core_policy_patch_transaction(
            &mut store,
            &s,
            &frame,
            f::log(1),
            f::limits()
        ),
        Err(CorePolicyTransactionError::Transaction(
            StateTransactionError::Uncertain
        ))
    );
    assert_eq!(fault.load(Ordering::SeqCst), 0);
    fenced(&mut store, &frame)?;
    drop(store);
    let actual = f::cohort(&path)?;
    assert!(
        actual == old || actual == new,
        "partial policy/index/origin/current/outcome/control/slot publication"
    );
    drop(f::open(&path, &s)?);
    reconcile(&path, &frame, &response, new.clone(), actual == new)?;
    counted_faults(&dir)?;
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

fn counted_faults(dir: &Path) -> TestResult {
    let (reads, writes) = counted_case(&dir.join("calibrate"), None)?;
    assert!(
        (1..=4096).contains(&reads),
        "physical read count exceeds selected finite coverage"
    );
    assert!(
        (1..=4096).contains(&writes),
        "physical write count exceeds selected finite coverage"
    );
    // Calibration selects actual delegated calls only. It never supplies an
    // expected business result or a semantic phase label for a redb I/O call.
    for offset in 0..reads {
        counted_case(&dir.join(format!("read-{offset}")), Some((false, offset)))?;
    }
    for offset in [0, writes - 1] {
        counted_case(&dir.join(format!("write-{offset}")), Some((true, offset)))?;
    }
    Ok(())
}

fn counted_case(path: &Path, failure: Option<(bool, usize)>) -> TestResult<(usize, usize)> {
    let s = f::scope()?;
    let (old, new, frame, response) = initialized(path)?;
    let read_count = Arc::new(AtomicUsize::new(0));
    let read_left = Arc::new(AtomicUsize::new(usize::MAX));
    let write_count = Arc::new(AtomicUsize::new(0));
    let write_left = Arc::new(AtomicUsize::new(usize::MAX));
    let database = fault_database_counted(
        path,
        Arc::new(AtomicU8::new(0)),
        0,
        Some((Arc::clone(&read_count), Arc::clone(&read_left))),
        Some((Arc::clone(&write_count), Arc::clone(&write_left))),
    )?;
    let mut store = wf_storage::state::RedbStateStore::open_database(
        database,
        s.group().clone(),
        s.recovery().clone(),
        f::store_limits()?,
    )?;
    // Strict open is outside the measured/injected operation window.
    read_count.store(0, Ordering::SeqCst);
    write_count.store(0, Ordering::SeqCst);
    if let Some((write, offset)) = failure {
        if write {
            write_left.store(offset, Ordering::SeqCst);
        } else {
            read_left.store(offset, Ordering::SeqCst);
        }
    }
    let result = apply_complete_core_policy_patch_transaction(
        &mut store,
        &s,
        &frame,
        f::log(1),
        f::limits(),
    );
    let counts = (
        read_count.load(Ordering::SeqCst),
        write_count.load(Ordering::SeqCst),
    );
    let injected = failure.is_none_or(|(write, _)| {
        if write {
            write_left.load(Ordering::SeqCst) == usize::MAX
        } else {
            read_left.load(Ordering::SeqCst) == usize::MAX
        }
    });
    // Drop/scope/reopen traffic must not become part of the operation's sample.
    read_left.store(usize::MAX, Ordering::SeqCst);
    write_left.store(usize::MAX, Ordering::SeqCst);
    assert!(
        injected,
        "calibrated physical call was not reached by the identical reset"
    );
    if failure.is_none() {
        assert!(
            matches!(result, Ok(RecordBatchTransactionOutcome::Retained { outcome, .. })
            if outcome.status() == RetainedStatus::Accepted && outcome.response() == response)
        );
    } else {
        assert!(
            matches!(
                result,
                Err(CorePolicyTransactionError::Transaction(
                    StateTransactionError::Unavailable | StateTransactionError::Uncertain
                ))
            ),
            "injected I/O was acknowledged"
        );
        fenced(&mut store, &frame)?;
    }
    drop(store);
    let actual = f::cohort(path)?;
    if failure.is_none() {
        assert_eq!(actual, new);
    } else {
        assert!(
            actual == old || actual == new,
            "counted failure left a mixed complete cohort"
        );
    }
    drop(f::open(path, &s)?);
    Ok(counts)
}

fn reconcile(
    path: &Path,
    frame: &[u8],
    response: &[u8],
    mut expected: f::Cohort,
    committed: bool,
) -> TestResult {
    let s = f::scope()?;
    let at = if committed { 2 } else { 1 };
    if committed {
        f::finish(
            &mut expected,
            &s,
            frame,
            f::log(at),
            (f::clock(25)?, f::clock(25)?),
            None,
        )?;
    }
    let reply = f::apply(path, &s, frame, at, &expected, response)?;
    assert_eq!(reply.original_position(), f::log(1));
    assert_eq!(reply.original_time(), TimestampMillis::new(25));
    assert_eq!(reply.expires_at(), TimestampMillis::new(1025));
    Ok(())
}

fn killed_child(path: &Path, after: bool) -> TestResult {
    let mut child = Command::new(std::env::current_exe()?)
        .arg("--exact")
        .arg("core_policy::faults::core_policy_acknowledgement_kill_reopens_one_complete_revision")
        .arg("--nocapture")
        .arg("--test-threads=1")
        .env("WF_CORE_POLICY_CHILD_PATH", path)
        .env("WF_CORE_POLICY_CHILD_AFTER", if after { "1" } else { "0" })
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()?;
    let Some(stdout) = child.stdout.take() else {
        terminate(&mut child)?;
        return Err("child stdout absent".into());
    };
    let Some(mut stdin) = child.stdin.take() else {
        terminate(&mut child)?;
        return Err("child stdin absent".into());
    };
    let (sender, receiver) = mpsc::sync_channel(2);
    let reader = match signal_reader(stdout, sender, after) {
        Ok(reader) => reader,
        Err(error) => {
            terminate(&mut child)?;
            return Err(error.into());
        }
    };
    let acknowledged = (|| -> TestResult {
        receiver.recv_timeout(Duration::from_secs(20))??;
        stdin.write_all(b"A")?;
        stdin.flush()?;
        receiver.recv_timeout(Duration::from_secs(20))??;
        Ok(())
    })();
    let terminated = terminate(&mut child);
    drop(stdin);
    let joined = reader.join();
    acknowledged?;
    assert!(!terminated?.success());
    joined.map_err(|_| "child signal reader panicked")??;
    Ok(())
}

fn child(path: &Path, after: bool) -> TestResult {
    let s = f::scope()?;
    let fault = Arc::new(AtomicU8::new(0));
    let mut store = wrapped_with_cache(path, Arc::clone(&fault), 0)?;
    if !after {
        fault.store(6, Ordering::SeqCst);
    }
    let frame = f::frame(
        &s,
        b"publish",
        25,
        b"job",
        Some(0),
        f::mode_patch(&[(b"q", Some(StateMode::Fifo))]),
    )?;
    assert!(
        matches!(apply_complete_core_policy_patch_transaction(&mut store, &s, frame.as_bytes(), f::log(1), f::limits())?,
        RecordBatchTransactionOutcome::Retained { outcome, .. } if outcome.status() == RetainedStatus::Accepted)
    );
    if after {
        parked("after-return")?;
    }
    Err("child returned without requested acknowledged park".into())
}

#[test]
fn core_policy_acknowledgement_kill_reopens_one_complete_revision() -> TestResult {
    if let Some(path) = std::env::var_os("WF_CORE_POLICY_CHILD_PATH") {
        return child(
            Path::new(&path),
            std::env::var("WF_CORE_POLICY_CHILD_AFTER")? == "1",
        );
    }
    let dir = f::directory("policy-kill")?;
    let s = f::scope()?;
    for after in [false, true] {
        let path = dir.join(if after { "after" } else { "before" });
        let (old, new, frame, response) = initialized(&path)?;
        killed_child(&path, after)?;
        let actual = f::cohort(&path)?;
        if after {
            assert_eq!(actual, new);
        } else {
            assert!(
                actual == old || actual == new,
                "killed policy left a mixed physical generation"
            );
        }
        drop(f::open(&path, &s)?);
        reconcile(&path, &frame, &response, new.clone(), actual == new)?;
    }
    std::fs::remove_dir_all(dir)?;
    Ok(())
}
