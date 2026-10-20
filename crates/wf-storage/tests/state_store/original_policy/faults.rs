//! Real delegated calls and acknowledged process boundaries preserve whole cohorts.
use super::super::faults::{
    fault_database, fault_database_counted, parked, signal_reader, terminate,
};
use super::{TestResult, fixtures as f};
use std::{
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
use wf_domain::policy::complete::{PolicySchemaCharge, RawFlowPolicyValue as Raw};
use wf_domain::workflow::request::{RetainedStatus, gate::PointGateView};
use wf_ports::log::{
    OriginalPolicySetPointInspection, inspect_original_policy_set_point_frame_bounded,
    prepare_original_policy_set_point_frame_bounded,
};
use wf_ports::state::{GroupApplyStore, GroupApplyTransaction, StateTransactionError};
use wf_storage::state::RedbStateStore;
use wf_storage::transaction::{
    OriginalPolicyTransactionError, RecordBatchTransactionOutcome,
    apply_original_policy_set_transaction,
};
use wf_types::identity::{PartitionId, RecordId, RequestId};
use wf_types::version::TimestampMillis;

fn frame() -> TestResult<Vec<u8>> {
    let config = [(Raw::Atom("mode"), Raw::Atom("fifo"))];
    let states = [(Raw::Binary(b"q"), Raw::Map(&config))];
    f::public_frame(
        &f::scope()?,
        &RequestId::new(b"publish".to_vec())?,
        25,
        b"job",
        Raw::Map(&[(Raw::Atom("states"), Raw::Map(&states))]),
        Raw::List(&[]),
        &mut PolicySchemaCharge::new(f::limits().schema),
    )
}

fn response() -> TestResult<Vec<u8>> {
    let config = f::etf_map(&[(f::atom(b"mode")?, f::atom(b"fifo")?)])?;
    let states = f::etf_map(&[(f::binary(b"q")?, config)])?;
    f::etf_policy(1, b"job", &[(b"states", &states)])
}

fn initialized(path: &Path) -> TestResult<(f::Cohort, f::Cohort, Vec<u8>, Vec<u8>)> {
    let s = f::scope()?;
    let mut store = f::create(path, &[], 64)?;
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
    let frame = frame()?;
    let response = response()?;
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
        f::frame_intent(&frame, 1)?,
        RetainedStatus::Accepted,
    )?;
    f::finish(
        &mut new,
        &s,
        &frame,
        f::log(1),
        (f::clock(25)?, f::clock(25)?),
        Some((&metadata, 1, &response)),
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
    frame: &[u8],
    at: u64,
) -> Result<RecordBatchTransactionOutcome, OriginalPolicyTransactionError> {
    let caps = f::limits();
    let s = f::scope().expect("fixed nonempty scope");
    apply_original_policy_set_transaction(
        store,
        &s,
        frame,
        f::log(at),
        caps,
        &mut PolicySchemaCharge::new(caps.schema),
    )
}

fn fenced(store: &mut RedbStateStore, frame: &[u8]) -> TestResult {
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
        perform(store, frame, 1),
        Err(OriginalPolicyTransactionError::Transaction(
            StateTransactionError::Unavailable
        ))
    );
    Ok(())
}

fn read_faults(dir: &Path) -> TestResult {
    for after_gate in [false, true] {
        let path = dir.join(if after_gate { "cold-body" } else { "point" });
        let (old, _, frame, _) = initialized(&path)?;
        let fault = Arc::new(AtomicU8::new(0));
        let mut store = wrapped(&path, Arc::clone(&fault))?;
        if after_gate {
            sealed_cold_read(&mut store, &frame, &fault)?;
        } else {
            fault.store(3, Ordering::SeqCst);
            assert_eq!(
                perform(&mut store, &frame, 1),
                Err(OriginalPolicyTransactionError::Transaction(
                    StateTransactionError::Unavailable
                ))
            );
            assert_eq!(
                fault.load(Ordering::SeqCst),
                0,
                "actual delegated point/control read"
            );
        }
        fenced(&mut store, &frame)?;
        drop(store);
        assert_eq!(f::cohort(&path)?, old);
        drop(f::open(&path, 64)?);
    }
    Ok(())
}

fn sealed_cold_read(store: &mut RedbStateStore, frame: &[u8], fault: &AtomicU8) -> TestResult {
    let s = f::scope()?;
    let caps = f::limits();
    let mut charge = PolicySchemaCharge::new(caps.schema);
    let allowance = caps.workspace_bytes
        - caps.maximum_read_bytes
        - caps.maximum_staging_bytes
        - caps.maximum_adapter_bytes;
    let verified = prepare_original_policy_set_point_frame_bounded(
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
    let request = RequestId::new(b"publish".to_vec())?;
    let original = tx.outcome(s.tenant(), &request)?;
    assert!(matches!(
        inspect_original_policy_set_point_frame_bounded(
            PointGateView {
                scope: &s,
                control,
                retained_nonempty,
                outcome: original.as_ref()
            },
            verified,
            f::log(1),
            caps.schema,
            caps.codec.admission(),
            &mut charge,
            allowance
        )?,
        OriginalPolicySetPointInspection::Fresh(_)
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
fn real_read_write_and_uncertain_faults_preserve_original_cohorts() -> TestResult {
    let dir = f::directory("original-faults")?;
    read_faults(&dir)?;
    let path = dir.join("commit");
    let (old, new, frame, response) = initialized(&path)?;
    let fault = Arc::new(AtomicU8::new(0));
    let mut store = wrapped(&path, Arc::clone(&fault))?;
    // Mode4 fails after an actual delegated commit-path sync, skipping growth
    // syncs. This is neither a final-flush label nor a power-loss simulation.
    fault.store(4, Ordering::SeqCst);
    assert_eq!(
        perform(&mut store, &frame, 1),
        Err(OriginalPolicyTransactionError::Transaction(
            StateTransactionError::Uncertain
        ))
    );
    assert_eq!(fault.load(Ordering::SeqCst), 0);
    fenced(&mut store, &frame)?;
    drop(store);
    let actual = f::cohort(&path)?;
    assert!(actual == old || actual == new, "mixed original publication");
    drop(f::open(&path, 64)?);
    reconcile(&path, &frame, &response, new, actual != old)?;
    counted_faults(&dir)?;
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

fn counted_faults(dir: &Path) -> TestResult {
    let (reads, writes) = counted_case(&dir.join("calibrate"), None)?;
    assert!(
        (1..=4096).contains(&reads),
        "finite read coverage cap exceeded"
    );
    assert!(
        (1..=4096).contains(&writes),
        "finite write coverage cap exceeded"
    );
    // Counts select delegated calls only, never semantic phases/business facts.
    for offset in 0..reads {
        counted_case(&dir.join(format!("read-{offset}")), Some((false, offset)))?;
    }
    for offset in 0..writes {
        counted_case(&dir.join(format!("write-{offset}")), Some((true, offset)))?;
    }
    Ok(())
}

fn counted_case(path: &Path, failure: Option<(bool, usize)>) -> TestResult<(usize, usize)> {
    let (old, new, frame, response) = initialized(path)?;
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
    // Open/preparation are outside the identical measured operation window.
    reads.store(0, Ordering::SeqCst);
    writes.store(0, Ordering::SeqCst);
    if let Some((write, offset)) = failure {
        if write {
            write_left.store(offset, Ordering::SeqCst);
        } else {
            read_left.store(offset, Ordering::SeqCst);
        }
    }
    let result = perform(&mut store, &frame, 1);
    let counts = (reads.load(Ordering::SeqCst), writes.load(Ordering::SeqCst));
    let injected = failure.is_none_or(|(write, _)| {
        if write {
            write_left.load(Ordering::SeqCst) == usize::MAX
        } else {
            read_left.load(Ordering::SeqCst) == usize::MAX
        }
    });
    // Freeze N before result/store Drop; later traffic cannot consume a fault.
    read_left.store(usize::MAX, Ordering::SeqCst);
    write_left.store(usize::MAX, Ordering::SeqCst);
    assert!(
        injected,
        "identically reset operation did not reach calibrated call"
    );
    let uncertain = match result {
        Ok(RecordBatchTransactionOutcome::Retained { outcome, .. }) if failure.is_none() => {
            assert_eq!(
                (outcome.status(), outcome.response()),
                (RetainedStatus::Accepted, response.as_slice())
            );
            false
        }
        Err(OriginalPolicyTransactionError::Transaction(StateTransactionError::Unavailable))
            if failure.is_some() =>
        {
            fenced(&mut store, &frame)?;
            false
        }
        Err(OriginalPolicyTransactionError::Transaction(StateTransactionError::Uncertain))
            if failure.is_some() =>
        {
            fenced(&mut store, &frame)?;
            true
        }
        other => return Err(format!("unexpected counted operation result: {other:?}").into()),
    };
    drop(store);
    let actual = f::cohort(path)?;
    if failure.is_none() {
        assert_eq!(actual, new);
    } else if uncertain {
        assert!(actual == old || actual == new, "mixed uncertain cohort");
    } else {
        assert_eq!(actual, old, "read/unavailable failure published rows");
    }
    drop(f::open(path, 64)?);
    Ok(counts)
}

fn reconcile(
    path: &Path,
    frame: &[u8],
    response: &[u8],
    mut expected: f::Cohort,
    committed: bool,
) -> TestResult {
    let at = if committed { 2 } else { 1 };
    let s = f::scope()?;
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
    let mut store = f::open(path, 64)?;
    let RecordBatchTransactionOutcome::Retained { outcome, .. } = perform(&mut store, frame, at)?
    else {
        return Err("reconciliation did not retain original reply".into());
    };
    assert_eq!(outcome.response(), response);
    assert_eq!(outcome.original_position(), f::log(1));
    assert_eq!(outcome.original_time(), TimestampMillis::new(25));
    assert_eq!(outcome.expires_at(), TimestampMillis::new(1025));
    drop(store);
    assert_eq!(f::cohort(path)?, expected);
    drop(f::open(path, 64)?);
    Ok(())
}

fn killed_child(path: &Path, after: bool) -> TestResult {
    let mut child = Command::new(std::env::current_exe()?)
        .arg("--exact")
        .arg("original_policy::faults::acknowledged_original_publication_kill_reopens_whole_cohort")
        .arg("--nocapture")
        .arg("--test-threads=1")
        .env("WF_ORIGINAL_POLICY_CHILD_PATH", path)
        .env(
            "WF_ORIGINAL_POLICY_CHILD_AFTER",
            if after { "1" } else { "0" },
        )
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
    let fault = Arc::new(AtomicU8::new(0));
    let mut store = wrapped(path, Arc::clone(&fault))?;
    if !after {
        fault.store(6, Ordering::SeqCst);
    }
    assert!(
        matches!(perform(&mut store,&frame()?,1)?,RecordBatchTransactionOutcome::Retained{outcome,..}
        if outcome.status()==RetainedStatus::Accepted)
    );
    if after {
        parked("after-return")?;
    }
    Err("child returned without acknowledged park".into())
}

#[test]
fn acknowledged_original_publication_kill_reopens_whole_cohort() -> TestResult {
    if let Some(path) = std::env::var_os("WF_ORIGINAL_POLICY_CHILD_PATH") {
        return child(
            Path::new(&path),
            std::env::var("WF_ORIGINAL_POLICY_CHILD_AFTER")? == "1",
        );
    }
    let dir = f::directory("original-kill")?;
    for after in [false, true] {
        let path = dir.join(if after { "after" } else { "before" });
        let (old, new, frame, response) = initialized(&path)?;
        killed_child(&path, after)?;
        let actual = f::cohort(&path)?;
        if after {
            assert_eq!(actual, new);
        } else {
            assert!(actual == old || actual == new, "mixed killed generation");
        }
        drop(f::open(&path, 64)?);
        let committed = actual == new;
        reconcile(&path, &frame, &response, new, committed)?;
    }
    std::fs::remove_dir_all(dir)?;
    Ok(())
}
