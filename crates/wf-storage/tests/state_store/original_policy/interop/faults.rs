//! Real Core-over-ETF delegated cuts and acknowledged process boundaries.
use super::super::super::faults::{
    fault_database, fault_database_counted, parked, signal_reader, terminate,
};
use super::{TestResult, f};
use std::{
    collections::BTreeMap,
    io::Write,
    path::Path,
    process::{Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicU8, AtomicUsize, Ordering},
        mpsc,
    },
    time::Duration,
};
use wf_domain::policy::complete::{
    CompleteCorePolicyPatchInput, CompleteCorePolicyPatchRequestInput, PolicySchemaCharge,
};
use wf_domain::policy::definition::{
    FieldPatch, PolicyPatch, StateChange, StateMode, StatePolicyPatch,
};
use wf_domain::workflow::request::{RequestScope, RetainedStatus};
use wf_ports::log::prepare_complete_core_policy_patch_frame;
use wf_ports::state::{GroupApplyStore, StateTransactionError};
use wf_storage::state::RedbStateStore;
use wf_storage::transaction::{
    CorePolicyTransactionError, RecordBatchTransactionOutcome,
    apply_complete_core_policy_patch_transaction,
};
use wf_types::identity::{RequestId, TenantId};
use wf_types::version::TimestampMillis;
mod consumed;

fn policy(generation: u64, mode: &[u8]) -> TestResult<Vec<u8>> {
    let state = f::etf_map(&[(f::atom(b"mode")?, f::atom(mode)?)])?;
    f::etf_policy(
        generation,
        b"job",
        &[(b"states", &f::etf_map(&[(f::binary(b"q")?, state)])?)],
    )
}
fn frame() -> TestResult<Vec<u8>> {
    let caps = f::limits();
    Ok(prepare_complete_core_policy_patch_frame(
        CompleteCorePolicyPatchRequestInput::new(
            f::scope()?,
            RequestId::new(b"publish".to_vec())?,
            f::clock(25)?,
            CompleteCorePolicyPatchInput {
                workflow_type: b"job".to_vec(),
                expected_generation: Some(1),
                patch: PolicyPatch {
                    states: BTreeMap::from([(
                        b"q".to_vec(),
                        StateChange::Patch(StatePolicyPatch {
                            mode: FieldPatch::Set(StateMode::Fifo),
                            ..StatePolicyPatch::default()
                        }),
                    )]),
                    ..PolicyPatch::default()
                },
            },
        ),
        caps.schema,
        caps.codec.admission(),
        &mut PolicySchemaCharge::new(caps.schema),
    )?
    .as_bytes()
    .to_vec())
}
fn initialized(path: &Path) -> TestResult<(f::Cohort, f::Cohort, Vec<u8>, Vec<u8>)> {
    let s = f::scope()?;
    let foreign_scope = RequestScope::new(
        TenantId::new(b"u".to_vec())?,
        s.group().clone(),
        s.recovery().clone(),
    );
    let foreign = f::owner(
        &foreign_scope,
        b"p",
        f::data(b"x", b"foreign", b"q", 0, Some(0))?,
        0,
        0,
    )?;
    let mut store = f::create(path, &[], 64)?;
    f::seed(&mut store, &foreign_scope, &[foreign], 0, 0)?;
    drop(store);
    let mut current = f::cohort(path)?;
    f::rows_mut(&mut current)?.insert(
        f::key(6, &[b"g", b"z", b"t", b"job"]),
        policy(1, b"parallel")?,
    );
    f::upgrade6(
        &mut current,
        &s,
        f::catalog(&s, [None, None], None, None, None)?,
    )?;
    f::replace(path, &current)?;
    let head = f::owner(
        &s,
        b"p",
        f::data(b"h", b"job", b"q", 1, Some(1))?,
        0,
        1_u128 << 32,
    )?;
    let next = f::owner(
        &s,
        b"p",
        f::data(b"i", b"job", b"q", 1, Some(1))?,
        0,
        (1_u128 << 32) + 1,
    )?;
    let mut store = f::open(path, 64)?;
    f::seed(&mut store, &s, &[head.clone(), next.clone()], 1, 1)?;
    drop(store);
    let old = f::cohort(path)?;
    let mut new = old.clone();
    let frame = frame()?;
    let response = policy(2, b"fifo")?;
    assert_eq!((policy(1, b"parallel")?.len(), response.len()), (80, 76));
    for row in [&head, &next] {
        assert_eq!(
            f::rows_mut(&mut new)?.remove(&f::due(&s, row)?),
            Some(vec![0])
        );
        f::rows_mut(&mut new)?.insert(f::member(&s, row), Vec::new());
    }
    f::rows_mut(&mut new)?.insert(f::due(&s, &head)?, vec![1]);
    let intent = super::w::core_intent(&frame, 2)?;
    f::rows_mut(&mut new)?.insert(f::key(6, &[b"g", b"z", b"t", b"job"]), response.clone());
    let metadata = f::metadata(&s, b"publish", 2, 25, intent, RetainedStatus::Accepted)?;
    f::finish(
        &mut new,
        &s,
        &frame,
        f::log(2),
        (f::clock(25)?, f::clock(25)?),
        Some((&metadata, 2, &response)),
    )?;
    f::upgrade6(
        &mut new,
        &s,
        f::catalog(&s, [None, None], None, None, None)?,
    )?;
    Ok((old, new, frame, response))
}
fn wrapped(path: &Path, fault: Arc<AtomicU8>) -> TestResult<RedbStateStore> {
    let s = f::scope()?;
    Ok(RedbStateStore::open_database(
        fault_database(path, fault, 0)?,
        s.group().clone(),
        s.recovery().clone(),
        f::store_limits(64)?,
    )?)
}
fn perform(
    store: &mut RedbStateStore,
    bytes: &[u8],
    at: u64,
) -> Result<RecordBatchTransactionOutcome, CorePolicyTransactionError> {
    apply_complete_core_policy_patch_transaction(
        store,
        &f::scope().map_err(|_| CorePolicyTransactionError::InvalidLimits)?,
        bytes,
        f::log(at),
        f::limits(),
    )
}
fn fenced(store: &mut RedbStateStore, bytes: &[u8]) -> TestResult {
    let s = f::scope()?;
    let caps = f::limits();
    assert!(matches!(
        store.begin_apply(
            s.group(),
            s.recovery(),
            caps.codec.admission(),
            100,
            1_000_000,
            1_000_000
        ),
        Err(StateTransactionError::Unavailable)
    ));
    assert_eq!(
        perform(store, bytes, 2),
        Err(CorePolicyTransactionError::Transaction(
            StateTransactionError::Unavailable
        ))
    );
    Ok(())
}
fn counted(path: &Path, failure: Option<(bool, usize)>) -> TestResult<(usize, usize)> {
    let (old, new, bytes, response) = initialized(path)?;
    let s = f::scope()?;
    let reads = Arc::new(AtomicUsize::new(0));
    let read_left = Arc::new(AtomicUsize::new(usize::MAX));
    let writes = Arc::new(AtomicUsize::new(0));
    let write_left = Arc::new(AtomicUsize::new(usize::MAX));
    let db = fault_database_counted(
        path,
        Arc::new(AtomicU8::new(0)),
        0,
        Some((Arc::clone(&reads), Arc::clone(&read_left))),
        Some((Arc::clone(&writes), Arc::clone(&write_left))),
    )?;
    let mut store = RedbStateStore::open_database(
        db,
        s.group().clone(),
        s.recovery().clone(),
        f::store_limits(64)?,
    )?;
    reads.store(0, Ordering::SeqCst);
    writes.store(0, Ordering::SeqCst);
    if let Some((write, offset)) = failure {
        if write {
            write_left.store(offset, Ordering::SeqCst);
        } else {
            read_left.store(offset, Ordering::SeqCst);
        }
    }
    let result = perform(&mut store, &bytes, 2);
    let counts = (reads.load(Ordering::SeqCst), writes.load(Ordering::SeqCst));
    let injected = failure.is_none_or(|(write, _)| {
        if write {
            write_left.load(Ordering::SeqCst) == usize::MAX
        } else {
            read_left.load(Ordering::SeqCst) == usize::MAX
        }
    });
    read_left.store(usize::MAX, Ordering::SeqCst);
    write_left.store(usize::MAX, Ordering::SeqCst);
    assert!(
        injected,
        "reset operation missed its calibrated delegated call"
    );
    let uncertain = match result {
        Ok(RecordBatchTransactionOutcome::Retained { outcome, .. }) if failure.is_none() => {
            assert_eq!(
                (
                    outcome.status(),
                    outcome.response_version(),
                    outcome.response()
                ),
                (RetainedStatus::Accepted, 2, response.as_slice())
            );
            false
        }
        Err(CorePolicyTransactionError::Transaction(StateTransactionError::Unavailable))
            if failure.is_some() =>
        {
            fenced(&mut store, &bytes)?;
            false
        }
        Err(CorePolicyTransactionError::Transaction(StateTransactionError::Uncertain))
            if failure.is_some() =>
        {
            fenced(&mut store, &bytes)?;
            true
        }
        other => return Err(format!("unexpected interop delegated-cut result: {other:?}").into()),
    };
    drop(store);
    let actual = f::cohort(path)?;
    if failure.is_none() {
        assert_eq!(actual, new);
    } else if uncertain {
        assert!(actual == old || actual == new, "mixed Core/ETF cohort");
    } else {
        assert_eq!(actual, old);
    }
    drop(f::open(path, 64)?);
    Ok(counts)
}
fn reconcile(
    path: &Path,
    bytes: &[u8],
    response: &[u8],
    mut expected: f::Cohort,
    committed: bool,
) -> TestResult {
    let at = if committed { 3 } else { 2 };
    if committed {
        f::finish(
            &mut expected,
            &f::scope()?,
            bytes,
            f::log(at),
            (f::clock(25)?, f::clock(25)?),
            None,
        )?;
    }
    let mut store = f::open(path, 64)?;
    let RecordBatchTransactionOutcome::Retained { outcome, .. } = perform(&mut store, bytes, at)?
    else {
        return Err("Core/ETF replay missing".into());
    };
    assert_eq!(
        (
            outcome.original_position(),
            outcome.original_time(),
            outcome.expires_at()
        ),
        (
            f::log(2),
            TimestampMillis::new(25),
            TimestampMillis::new(1025)
        )
    );
    assert_eq!(
        (outcome.response_version(), outcome.response()),
        (2, response)
    );
    drop(store);
    assert_eq!(f::cohort(path)?, expected);
    drop(f::open(path, 64)?);
    Ok(())
}
fn killed(path: &Path, after: bool) -> TestResult {
    let mut child = Command::new(std::env::current_exe()?).arg("--exact")
        .arg("original_policy::interop::faults::interop_exact_owner_end_and_real_fault_cuts_preserve_full_cohort")
        .arg("--nocapture").arg("--test-threads=1").env("WF_INTEROP_CHILD_PATH", path)
        .env("WF_INTEROP_CHILD_AFTER", if after { "1" } else { "0" })
        .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::inherit()).spawn()?;
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
    joined.map_err(|_| "child reader panicked")??;
    Ok(())
}
fn child(path: &Path, after: bool) -> TestResult {
    let fault = Arc::new(AtomicU8::new(0));
    let mut store = wrapped(path, Arc::clone(&fault))?;
    if !after {
        fault.store(6, Ordering::SeqCst);
    }
    assert!(
        matches!(perform(&mut store, &frame()?, 2)?, RecordBatchTransactionOutcome::Retained { outcome, .. }
        if outcome.response_version() == 2 && outcome.status() == RetainedStatus::Accepted)
    );
    if after {
        parked("after-return")?;
    }
    Err("child escaped acknowledged park".into())
}
pub(super) fn snapshot_cuts(
    dir: &Path,
    wire: &[u8],
    new: &f::Cohort,
    initial: &[u8],
) -> TestResult {
    use std::io::Cursor;
    use wf_ports::snapshot::{
        SnapshotError, SnapshotLimits, StateSnapshotStore, StateSnapshotTransitionStore,
    };
    use wf_storage::snapshot::RedbSnapshotStage;
    let scope = f::scope()?;
    for source_fault in [true, false] {
        let suffix = if source_fault { "source" } else { "active" };
        let path = dir.join(format!("snapshot-cut-{suffix}"));
        drop(f::create(
            &path,
            &[(b"job".to_vec(), initial.to_vec())],
            64,
        )?);
        let old = f::cohort(&path)?;
        let stage_path = dir.join(format!("snapshot-stage-{suffix}"));
        drop(redb::Builder::new().create(&stage_path)?);
        let stage_fault = Arc::new(AtomicU8::new(0));
        let target_fault = Arc::new(AtomicU8::new(0));
        let candidate = RedbSnapshotStage::receive_database(
            fault_database(&stage_path, Arc::clone(&stage_fault), 0)?,
            scope.group().clone(),
            scope.recovery().clone(),
            &mut Cursor::new(wire),
            f::store_limits(64)?,
            SnapshotLimits::default(),
        )?;
        let mut active = RedbStateStore::open_database(
            fault_database(&path, Arc::clone(&target_fault), 0)?,
            scope.group().clone(),
            scope.recovery().clone(),
            f::store_limits(64)?,
        )?;
        if source_fault {
            stage_fault.store(3, Ordering::SeqCst);
        } else {
            target_fault.store(4, Ordering::SeqCst);
        }
        let error = active
            .install_snapshot(candidate, SnapshotLimits::default())
            .err()
            .ok_or("fault acknowledged snapshot installation")?;
        if source_fault {
            assert_eq!(
                error,
                SnapshotError::Stage(StateTransactionError::Unavailable)
            );
            assert_eq!(stage_fault.load(Ordering::SeqCst), 0);
            assert!(active.supports_snapshot_transitions().is_ok());
        } else {
            assert_eq!(
                error,
                SnapshotError::State(StateTransactionError::Uncertain)
            );
            assert_eq!(target_fault.load(Ordering::SeqCst), 0);
            assert_eq!(
                active.supports_snapshot_transitions(),
                Err(SnapshotError::State(StateTransactionError::Unavailable))
            );
        }
        drop(active);
        let actual = f::cohort(&path)?;
        if source_fault {
            assert_eq!(actual, old);
        } else {
            assert!(
                actual == old || actual == *new,
                "mixed received response2 cohort"
            );
        }
        assert_eq!(f::cohort(&stage_path)?, *new);
        drop(f::open(&path, 64)?);
    }
    Ok(())
}
#[test]
fn interop_exact_owner_end_and_real_fault_cuts_preserve_full_cohort() -> TestResult {
    if let Some(path) = std::env::var_os("WF_INTEROP_CHILD_PATH") {
        return child(
            Path::new(&path),
            std::env::var("WF_INTEROP_CHILD_AFTER")? == "1",
        );
    }
    let dir = f::directory("interop-core-faults")?;
    consumed::assert_prefix(&dir)?;
    let (reads, writes) = counted(&dir.join("calibrate"), None)?;
    assert!((1..=4096).contains(&reads), "finite read coverage exceeded");
    assert!(
        (1..=4096).contains(&writes),
        "finite write coverage exceeded"
    );
    for offset in 0..reads {
        counted(&dir.join(format!("read-{offset}")), Some((false, offset)))?;
    }
    for offset in 0..writes {
        counted(&dir.join(format!("write-{offset}")), Some((true, offset)))?;
    }
    let path = dir.join("uncertain");
    let (old, new, bytes, response) = initialized(&path)?;
    let fault = Arc::new(AtomicU8::new(0));
    let mut store = wrapped(&path, Arc::clone(&fault))?;
    fault.store(4, Ordering::SeqCst);
    assert_eq!(
        perform(&mut store, &bytes, 2),
        Err(CorePolicyTransactionError::Transaction(
            StateTransactionError::Uncertain
        ))
    );
    assert_eq!(fault.load(Ordering::SeqCst), 0);
    fenced(&mut store, &bytes)?;
    drop(store);
    let actual = f::cohort(&path)?;
    assert!(actual == old || actual == new);
    reconcile(&path, &bytes, &response, new.clone(), actual == new)?;
    for after in [false, true] {
        let path = dir.join(if after { "after" } else { "before" });
        let (old, new, bytes, response) = initialized(&path)?;
        killed(&path, after)?;
        let actual = f::cohort(&path)?;
        if after {
            assert_eq!(actual, new);
        } else {
            assert!(actual == old || actual == new);
        }
        let committed = actual == new;
        reconcile(&path, &bytes, &response, new, committed)?;
    }
    std::fs::remove_dir_all(dir)?;
    Ok(())
}
