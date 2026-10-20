//! Delegated physical reads, uncertain commit, and acknowledged process kills.

use super::super::faults::{parked, signal_reader, terminate, wrapped_with_cache};
use super::{TestResult, fixtures as f};
use std::collections::BTreeMap;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, mpsc};
use std::time::Duration;
use wf_domain::history::EventKind;
use wf_domain::lease::{Lease, LeaseToken};
use wf_domain::workflow::indexed::OrderingMetadata;
use wf_domain::workflow::request::{RetainedStatus, gate::PointGateView};
use wf_domain::workflow::restoration::WorkflowRecordData;
use wf_ports::log::{ClaimPhaseInspection, inspect_claim_phase_point, prepare_claim_phase};
use wf_ports::state::{GroupApplyStore, GroupApplyTransaction, StateTransactionError};
use wf_storage::transaction::{
    ClaimTransactionError, ClaimTransactionOutcome, apply_claim_transaction,
};
use wf_types::identity::{PartitionId, RecordId, RequestId};
use wf_types::version::{Fence, TimestampMillis};

fn initialized(
    path: &Path,
) -> TestResult<(
    [BTreeMap<Vec<u8>, Vec<u8>>; 4],
    [BTreeMap<Vec<u8>, Vec<u8>>; 4],
    Vec<u8>,
    Vec<u8>,
)> {
    let s = f::scope()?;
    let mut store = f::create(path, &s, &[b"p"], Some(f::policy(b"job", &[b"q"], false)?))?;
    let mut first = f::data(b"head", b"job", b"q", 0, Some(0))?;
    first.payload = Some(vec![0xa5; 131_072]);
    let head = f::owner(
        &s,
        b"p",
        first,
        OrderingMetadata {
            priority: 0,
            state_enter_seq: 0,
        },
    )?;
    let follower = f::owner(
        &s,
        b"p",
        f::data(b"follower", b"job", b"q", 0, Some(0))?,
        OrderingMetadata {
            priority: 0,
            state_enter_seq: 1,
        },
    )?;
    f::seed(&mut store, &s, &[head.clone(), follower], 0, 0)?;
    drop(store);
    let old = f::cohort(path)?;
    let mut new = old.clone();
    let after = f::owner(
        &s,
        b"p",
        WorkflowRecordData {
            state: b"running".to_vec(),
            version: 2,
            fence: Fence::new(1),
            updated_at: TimestampMillis::new(25),
            next_run_at: Some(TimestampMillis::new(30025)),
            run_state: Some(b"q".to_vec()),
            lease: Some(Lease {
                worker: b"worker".to_vec(),
                token: LeaseToken::new(b"worker:25:1".to_vec())?,
                deadline: TimestampMillis::new(30025),
            }),
            payload: Some(vec![0xa5; 131_072]),
            latest_kind: EventKind::Claimed,
            latest_version: 2,
            latest_fence: Fence::new(1),
            ..f::data(b"head", b"job", b"q", 0, Some(0))?
        },
        head.ordering,
    )?;
    let frame = f::phase(
        &s,
        b"claim",
        25,
        &f::query(b"job", b"worker", 1),
        (1, 1),
        [51; 32],
    )?;
    let response = f::receipt(std::slice::from_ref(&after), 0, 1, 1)?;
    f::replace_expected(
        &mut new,
        &s,
        &after,
        &f::history(&after),
        &[f::due(&s, &head)?, f::member(&s, &head)],
        &[
            (f::due(&s, &after)?, vec![1]),
            (f::member(&s, &after), Vec::new()),
        ],
    )?;
    let metadata = f::metadata(&s, b"claim", 1, 25, [51; 32], RetainedStatus::Accepted)?;
    f::finish(
        &mut new,
        &s,
        &frame,
        f::log(1),
        (f::clock(25)?, f::clock(25)?),
        Some((&metadata, &response)),
    )?;
    Ok((old, new, frame, response))
}

fn fenced(store: &mut wf_storage::state::RedbStateStore, frame: &[u8]) -> TestResult {
    let s = f::scope()?;
    let caps = f::limits();
    assert!(matches!(
        store.begin_apply(
            s.group(),
            s.recovery(),
            caps.base.codec.admission(),
            100,
            1_000_000,
            1_000_000
        ),
        Err(StateTransactionError::Unavailable)
    ));
    assert_eq!(
        apply_claim_transaction(store, &s, frame, f::log(2), caps),
        Err(ClaimTransactionError::Transaction(
            StateTransactionError::Unavailable
        ))
    );
    Ok(())
}

#[test]
fn actual_read_fault_before_and_after_gate_has_no_local_prefix() -> TestResult {
    let dir = f::directory("claim-read-fault")?;
    let s = f::scope()?;
    for after_gate in [false, true] {
        let path = dir.join(if after_gate { "after" } else { "before" });
        let (old, _, frame, _) = initialized(&path)?;
        let fault = Arc::new(AtomicU8::new(0));
        let mut store = wrapped_with_cache(&path, Arc::clone(&fault), 0)?;
        if after_gate {
            // Real same-writer observations, followed by the actual sealed gate.
            let decoded = prepare_claim_phase(
                &frame,
                f::limits().base.codec,
                f::limits().base.workspace_bytes,
            )?;
            let tx = store.begin_apply(
                s.group(),
                s.recovery(),
                f::limits().base.codec.admission(),
                1000,
                16_000_000,
                16_000_000,
            )?;
            let control = tx.control()?;
            let retained_nonempty = tx.retained_nonempty(s.tenant())?;
            let original = tx.outcome(s.tenant(), &RequestId::new(b"claim".to_vec())?)?;
            let fresh = inspect_claim_phase_point(
                decoded,
                f::log(1),
                PointGateView {
                    scope: &s,
                    control,
                    outcome: original.as_ref(),
                    retained_nonempty,
                },
                f::limits().base.codec,
                f::limits().base.workspace_bytes,
                f::limits().work,
            )?;
            assert!(matches!(fresh, ClaimPhaseInspection::Fresh(_)));
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
                "large cold body reached delegated physical read"
            );
            assert_eq!(tx.control(), Err(StateTransactionError::Unavailable));
        } else {
            fault.store(3, Ordering::SeqCst);
            assert_eq!(
                apply_claim_transaction(&mut store, &s, &frame, f::log(1), f::limits()),
                Err(ClaimTransactionError::Transaction(
                    StateTransactionError::Unavailable
                ))
            );
            assert_eq!(
                fault.load(Ordering::SeqCst),
                0,
                "gate reached delegated physical read"
            );
        }
        fenced(&mut store, &frame)?;
        drop(store);
        assert_eq!(f::cohort(&path)?, old);
        drop(f::open(&path, &s)?);
    }
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

fn reconcile(
    path: &Path,
    frame: &[u8],
    response: &[u8],
    mut expected: [BTreeMap<Vec<u8>, Vec<u8>>; 4],
    committed: bool,
) -> TestResult {
    let s = f::scope()?;
    let index = if committed { 2 } else { 1 };
    if committed {
        f::finish(
            &mut expected,
            &s,
            frame,
            f::log(index),
            (f::clock(25)?, f::clock(25)?),
            None,
        )?;
    }
    let reply = f::apply_check(
        path,
        &s,
        frame,
        f::log(index),
        f::limits(),
        &expected,
        response,
    )?;
    assert_eq!(reply.original_position(), f::log(1));
    assert_eq!(reply.original_time(), TimestampMillis::new(25));
    assert_eq!(reply.expires_at(), TimestampMillis::new(1025));
    Ok(())
}

#[test]
fn actual_commit_uncertainty_fences_and_reopens_complete_cohort() -> TestResult {
    let dir = f::directory("claim-sync-fault")?;
    let path = dir.join("state");
    let s = f::scope()?;
    let (old, new, frame, response) = initialized(&path)?;
    let fault = Arc::new(AtomicU8::new(0));
    let mut store = wrapped_with_cache(&path, Arc::clone(&fault), 0)?;
    // Mode4 skips delegated growth syncs and fails after the first subsequent
    // reached commit-path sync. This is not a final-flush/power-loss claim.
    fault.store(4, Ordering::SeqCst);
    assert_eq!(
        apply_claim_transaction(&mut store, &s, &frame, f::log(1), f::limits()),
        Err(ClaimTransactionError::Transaction(
            StateTransactionError::Uncertain
        ))
    );
    assert_eq!(
        fault.load(Ordering::SeqCst),
        0,
        "actual delegated commit-path sync failed"
    );
    fenced(&mut store, &frame)?;
    drop(store);
    let actual = f::cohort(&path)?;
    assert!(
        actual == old || actual == new,
        "partial core/index/head/history/outcome/control publication"
    );
    drop(f::open(&path, &s)?);
    reconcile(&path, &frame, &response, new.clone(), actual == new)?;
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

fn killed_child(path: &Path, after: bool) -> TestResult {
    let mut child = Command::new(std::env::current_exe()?)
        .arg("--exact")
        .arg("claim::faults::acknowledged_claim_kill_before_sync_and_after_return")
        .arg("--nocapture")
        .arg("--test-threads=1")
        .env("WF_CLAIM_CHILD_PATH", path)
        .env("WF_CLAIM_CHILD_AFTER", if after { "1" } else { "0" })
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
    let frame = f::phase(
        &s,
        b"claim",
        25,
        &f::query(b"job", b"worker", 1),
        (1, 1),
        [51; 32],
    )?;
    assert!(matches!(
        apply_claim_transaction(&mut store, &s, &frame, f::log(1), f::limits())?,
        ClaimTransactionOutcome::Retained { .. }
    ));
    if after {
        parked("after-return")?;
    }
    Err("child returned without requested acknowledged park".into())
}

#[test]
fn acknowledged_claim_kill_before_sync_and_after_return() -> TestResult {
    if let Some(path) = std::env::var_os("WF_CLAIM_CHILD_PATH") {
        return child(
            Path::new(&path),
            std::env::var("WF_CLAIM_CHILD_AFTER")? == "1",
        );
    }
    let dir = f::directory("claim-kill")?;
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
                "killed Claim left a partial physical cohort"
            );
        }
        drop(f::open(&path, &s)?);
        reconcile(&path, &frame, &response, new.clone(), actual == new)?;
    }
    std::fs::remove_dir_all(dir)?;
    Ok(())
}
