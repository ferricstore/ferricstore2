//! Real delegated sync uncertainty, uncached reads, and acknowledged process kills.

use super::super::faults::{parked, wrapped, wrapped_with_cache};
use super::{TestResult, f, fixtures};
use std::{
    io::{self, Read, Write},
    path::Path,
    process::{Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicU8, Ordering},
        mpsc,
    },
    thread,
    time::Duration,
};
use wf_domain::workflow::admission::AdmissionLimits;
use wf_domain::workflow::control::EntryStamp;
use wf_ports::consensus::RaftMembershipLimits;
use wf_ports::state::{
    GroupApplyStore, GroupApplyTransaction, GroupControlChanges, GroupControlTransaction,
    StateTransactionError,
};
use wf_storage::transaction::{RecordBatchTransactionOutcome, apply_record_batch_transaction};

fn seed(path: &Path) -> TestResult {
    let mut store = f::create(path)?;
    let frame = f::frame(b"seed", 1000, vec![(b"i", f::create_op())])?;
    let result = apply_record_batch_transaction(
        &mut store,
        &f::scope()?,
        frame.as_bytes(),
        f::log(0),
        f::driver_limits(),
    )?;
    let RecordBatchTransactionOutcome::Retained { control, outcome } = result else {
        return Err("seed did not retain".into());
    };
    assert_eq!(control.position().get(), 0);
    assert_eq!(outcome.response(), [0]);
    assert_eq!(
        outcome.status(),
        wf_domain::workflow::request::RetainedStatus::Accepted
    );
    Ok(())
}

#[test]
fn control_sync_uncertainty_keeps_whole_cohorts_and_fences() -> TestResult {
    let dir = f::directory("control-sync")?;
    for member in [false, true] {
        let path = dir.join(if member { "member.redb" } else { "blank.redb" });
        seed(&path)?;
        let before_rows = f::raw_rows(&path)?;
        let before_meta = fixtures::raw_meta(&path)?;
        let frame = fixtures::frame(member, fixtures::stamp(20, 3, 19)?)?;
        let mut state = f::open(&path)?;
        let prior = fixtures::current(&mut state)?;
        drop(state);
        let next = fixtures::next(prior, &frame, f::log(1), fixtures::stamp(20, 3, 19)?)?;
        let mut after_rows = before_rows.clone();
        fixtures::set_control(&mut after_rows, next)?;
        let mut after_meta = before_meta.clone();
        if member {
            after_meta.insert(
                fixtures::MEMBER_KEY.to_vec(),
                fixtures::row(Some(f::log(1)), Some(frame.as_bytes()))?,
            );
        }
        let fault = Arc::new(AtomicU8::new(0));
        state = wrapped(&path, Arc::clone(&fault))?;
        let s = f::scope()?;
        let tx = state.begin_apply(
            s.group(),
            s.recovery(),
            AdmissionLimits::default(),
            100,
            1024,
            65536,
        )?;
        let changes = GroupControlChanges::new(
            s.group(),
            s.recovery(),
            tx.control()?,
            f::log(1),
            frame.as_bytes(),
            RaftMembershipLimits::default(),
        )?;
        assert_eq!(changes.expected(), prior);
        fault.store(4, Ordering::SeqCst);
        let actual = tx
            .commit_control(&changes)
            .err()
            .ok_or("faulted control unexpectedly succeeded")?;
        assert_eq!(
            actual,
            StateTransactionError::Uncertain,
            "member={member}, fault={}",
            fault.load(Ordering::SeqCst)
        );
        assert_eq!(
            fault.load(Ordering::SeqCst),
            0,
            "reached actual delegated sync"
        );
        assert!(matches!(
            state.begin_apply(
                s.group(),
                s.recovery(),
                AdmissionLimits::default(),
                100,
                8192,
                65536
            ),
            Err(StateTransactionError::Unavailable)
        ));
        drop(state);
        let rows = f::raw_rows(&path)?;
        let meta = fixtures::raw_meta(&path)?;
        assert!(
            (rows == before_rows && meta == before_meta)
                || (rows == after_rows && meta == after_meta),
            "mixed membership/control cohort"
        );
        state = f::open(&path)?;
        assert_eq!(fixtures::apply(&mut state, &frame, f::log(1))?, next);
        drop(state);
        fixtures::assert_pair(&path, &after_rows, &after_meta)?;
    }
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn same_control_guard_fences_data_after_real_read_failure() -> TestResult {
    let dir = f::directory("control-read-fence")?;
    let path = dir.join("state.redb");
    seed(&path)?;
    let fault = Arc::new(AtomicU8::new(0));
    let mut state = wrapped_with_cache(&path, Arc::clone(&fault), 0)?;
    let s = f::scope()?;
    let body = fixtures::frame(false, EntryStamp::Unstamped)?;
    let tx = state.begin_apply(
        s.group(),
        s.recovery(),
        AdmissionLimits::default(),
        100,
        8192,
        65536,
    )?;
    let changes = GroupControlChanges::new(
        s.group(),
        s.recovery(),
        tx.control()?,
        f::log(1),
        body.as_bytes(),
        RaftMembershipLimits::default(),
    )?;
    fault.store(3, Ordering::SeqCst);
    assert!(matches!(
        tx.applied_membership(RaftMembershipLimits::default()),
        Err(StateTransactionError::Unavailable)
    ));
    assert_eq!(
        fault.load(Ordering::SeqCst),
        0,
        "uncached getter reached actual backend read"
    );
    assert_eq!(tx.owner_limits(), (8192, 65536));
    assert_eq!(tx.control(), Err(StateTransactionError::Unavailable));
    assert!(matches!(
        tx.applied_membership(RaftMembershipLimits::default()),
        Err(StateTransactionError::Unavailable)
    ));
    assert_eq!(
        tx.commit_control(&changes),
        Err(StateTransactionError::Unavailable)
    );
    assert!(matches!(
        state.begin_apply(
            s.group(),
            s.recovery(),
            AdmissionLimits::default(),
            100,
            8192,
            65536
        ),
        Err(StateTransactionError::Unavailable)
    ));
    drop(state);
    drop(f::open(&path)?);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn control_entry_kill_boundaries() -> TestResult {
    if let Some(path) = std::env::var_os("WF_CONTROL_CHILD_PATH") {
        return child(Path::new(&path));
    }
    let dir = f::directory("control-kills")?;
    for member in [false, true] {
        for after in [false, true] {
            let path = dir.join(format!("{member}-{after}.redb"));
            seed(&path)?;
            let before_rows = f::raw_rows(&path)?;
            let before_meta = fixtures::raw_meta(&path)?;
            let frame = fixtures::frame(member, fixtures::stamp(20, 3, 19)?)?;
            let mut state = f::open(&path)?;
            let prior = fixtures::current(&mut state)?;
            drop(state);
            let mut after_rows = before_rows.clone();
            fixtures::set_control(
                &mut after_rows,
                fixtures::next(prior, &frame, f::log(1), fixtures::stamp(20, 3, 19)?)?,
            )?;
            let mut after_meta = before_meta.clone();
            if member {
                after_meta.insert(
                    fixtures::MEMBER_KEY.to_vec(),
                    fixtures::row(Some(f::log(1)), Some(frame.as_bytes()))?,
                );
            }
            kill(&path, member, after)?;
            let rows = f::raw_rows(&path)?;
            let meta = fixtures::raw_meta(&path)?;
            if after {
                assert_eq!(rows, after_rows);
                assert_eq!(meta, after_meta);
            } else {
                assert!(
                    (rows == before_rows && meta == before_meta)
                        || (rows == after_rows && meta == after_meta),
                    "partial killed cohort"
                );
            }
            drop(f::open(&path)?);
        }
    }
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

fn child(path: &Path) -> TestResult {
    let member = std::env::var("WF_CONTROL_CHILD_MEMBER")? == "1";
    let after = std::env::var("WF_CONTROL_CHILD_AFTER")? == "1";
    let fault = Arc::new(AtomicU8::new(0));
    let mut state = wrapped(path, Arc::clone(&fault))?;
    let frame = fixtures::frame(member, fixtures::stamp(20, 3, 19)?)?;
    let s = f::scope()?;
    let tx = state.begin_apply(
        s.group(),
        s.recovery(),
        AdmissionLimits::default(),
        100,
        1024,
        65536,
    )?;
    let changes = GroupControlChanges::new(
        s.group(),
        s.recovery(),
        tx.control()?,
        f::log(1),
        frame.as_bytes(),
        RaftMembershipLimits::default(),
    )?;
    if !after {
        fault.store(6, Ordering::SeqCst);
    }
    tx.commit_control(&changes)?;
    if !after {
        return Err("before-sync child unexpectedly returned".into());
    }
    parked("after-return")?;
    Ok(())
}

fn read_signal(reader: &mut impl Read, token: &[u8]) -> io::Result<()> {
    for _ in 0..32 {
        let mut line = [0; 256];
        let mut n = 0;
        loop {
            if n == line.len() {
                return Err(io::Error::other("oversized control child line"));
            }
            reader.read_exact(&mut line[n..=n])?;
            n += 1;
            if line[n - 1] == b'\n' {
                break;
            }
        }
        if line[..n].windows(token.len()).any(|part| part == token) {
            return Ok(());
        }
    }
    Err(io::Error::other("missing control child boundary"))
}

fn kill(path: &Path, member: bool, after: bool) -> TestResult {
    let mut child = Command::new(std::env::current_exe()?)
        .args([
            "--exact",
            "control::faults::control_entry_kill_boundaries",
            "--nocapture",
            "--test-threads=1",
        ])
        .env("WF_CONTROL_CHILD_PATH", path)
        .env("WF_CONTROL_CHILD_MEMBER", if member { "1" } else { "0" })
        .env("WF_CONTROL_CHILD_AFTER", if after { "1" } else { "0" })
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()?;
    let mut stdout = child.stdout.take().ok_or("child stdout")?;
    let mut stdin = child.stdin.take().ok_or("child stdin")?;
    let (sender, receiver) = mpsc::sync_channel(1);
    let reader = thread::spawn(move || {
        let stage = if after { "after-return" } else { "before-sync" };
        let first = read_signal(&mut stdout, format!("WFSTATE READY {stage}\n").as_bytes());
        let ok = first.is_ok();
        if sender.send(first).is_err() {
            return;
        }
        if ok {
            let _ = sender.send(read_signal(
                &mut stdout,
                format!("WFSTATE WAIT {stage}\n").as_bytes(),
            ));
        }
    });
    for phase in 0..2 {
        match receiver.recv_timeout(Duration::from_secs(20)) {
            Ok(Ok(())) => {}
            result => {
                child.kill()?;
                child.wait()?;
                reader.join().map_err(|_| "signal reader panicked")?;
                return Err(format!("control handshake: {result:?}").into());
            }
        }
        if phase == 0 {
            stdin.write_all(b"A")?;
            stdin.flush()?;
        }
    }
    assert!(child.try_wait()?.is_none());
    child.kill()?;
    assert!(!child.wait()?.success());
    drop(stdin);
    reader.join().map_err(|_| "signal reader panicked")?;
    Ok(())
}
