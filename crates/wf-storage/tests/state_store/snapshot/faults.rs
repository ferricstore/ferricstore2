//! Caller I/O faults are separate from real active-engine uncertainty and kills.

use std::{
    io::{self, Cursor, Read, Write},
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
use wf_ports::state::StateTransactionError;
use wf_storage::state::StateStoreLimits;

use super::super::{
    control,
    faults::{fault_database, parked, wrapped},
};
use super::{
    RedbSnapshotStage, SnapshotError, SnapshotLimits, StateSnapshotStore, TestResult, f,
    fixtures as s, limits,
};

pub(super) struct ReadFault(
    pub(super) Cursor<Vec<u8>>,
    pub(super) usize,
    pub(super) bool,
);
pub(super) struct WriteFault(pub(super) Vec<u8>, pub(super) usize, pub(super) bool);

impl Read for ReadFault {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        if out.is_empty() {
            return Ok(0);
        }
        if std::mem::take(&mut self.2) {
            return Err(io::ErrorKind::Interrupted.into());
        }
        let position = usize::try_from(self.0.position())
            .map_err(|_| io::Error::other("input position overflow"))?;
        let remaining = self.1.saturating_sub(position);
        if remaining == 0 {
            return Err(io::Error::other("injected snapshot input failure"));
        }
        let n = remaining.min(out.len());
        self.0.read(&mut out[..n])
    }
}

impl Write for WriteFault {
    fn write(&mut self, input: &[u8]) -> io::Result<usize> {
        if input.is_empty() {
            return Ok(0);
        }
        if std::mem::take(&mut self.2) {
            return Err(io::ErrorKind::Interrupted.into());
        }
        let remaining = self.1.saturating_sub(self.0.len());
        if remaining == 0 {
            return Err(io::Error::other("injected snapshot sink failure"));
        }
        let n = remaining.min(input.len());
        self.0.extend_from_slice(&input[..n]);
        Ok(n)
    }
    fn flush(&mut self) -> io::Result<()> {
        Err(io::Error::other("caller owns flush"))
    }
}

#[test]
fn actual_short_io_interrupted_attempts_and_failure_custody() -> TestResult {
    let dir = f::directory("snapshot-io")?;
    let path = dir.join("state.redb");
    s::seed(&path)?;
    let original = s::capture(&path)?;
    let mut state = f::open(&path)?;
    s::apply(&mut state, b"later", 4, 13)?;
    drop(state);
    let before = s::raw(&path)?;
    let next = s::frame((b"g", b"z"), &s::authority(&before)?)?;
    state = f::open(&path)?;
    let mut sink = WriteFault(Vec::new(), 7, true);
    assert!(matches!(
        state.capture_snapshot(&mut sink, SnapshotLimits::default()),
        Err(SnapshotError::OutputUnavailable)
    ));
    assert_eq!(sink.0, &next[..7]);
    assert!(!sink.2);
    let mut current = Vec::new();
    state
        .current_snapshot(&mut current, SnapshotLimits::default())?
        .ok_or("old snapshot disappeared")?;
    assert_eq!(current, original);
    assert_eq!(control::fixtures::current(&mut state)?.position().get(), 4);
    let mut zero: &mut [u8] = &mut [];
    assert!(matches!(
        state.capture_snapshot(&mut zero, SnapshotLimits::default()),
        Err(SnapshotError::OutputUnavailable)
    ));
    assert_eq!(control::fixtures::current(&mut state)?.position().get(), 4);
    let mut input = ReadFault(Cursor::new(next), 7, true);
    let scope = f::scope()?;
    assert!(matches!(
        RedbSnapshotStage::receive(
            &dir.join("input.redb"),
            scope.group().clone(),
            scope.recovery().clone(),
            &mut input,
            StateStoreLimits::default(),
            SnapshotLimits::default()
        ),
        Err(SnapshotError::InputUnavailable)
    ));
    assert_eq!(input.0.position(), 7);
    assert!(!input.2);
    control::fixtures::current(&mut state)?;
    drop(state);
    s::assert_raw(&path, &before)?;
    stage_source_failure(&dir, input.0.get_ref())?;
    interrupted_limits(&dir)?;
    fresh_custody(&dir)?;
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

fn stage_source_failure(dir: &Path, wire: &[u8]) -> TestResult {
    let path = dir.join("stage-origin-active.redb");
    target(&path)?;
    let mut active = f::open(&path)?;
    active.capture_snapshot(&mut Vec::new(), SnapshotLimits::default())?;
    drop(active);
    let before = s::raw(&path)?;
    let stage_path = dir.join("stage-origin-source.redb");
    drop(redb::Builder::new().create(&stage_path)?);
    let fault = Arc::new(AtomicU8::new(0));
    let database = fault_database(&stage_path, Arc::clone(&fault), 0)?;
    let scope = f::scope()?;
    let incoming_stage = RedbSnapshotStage::receive_database(
        database,
        scope.group().clone(),
        scope.recovery().clone(),
        &mut Cursor::new(wire),
        StateStoreLimits::default(),
        SnapshotLimits::default(),
    )?;
    active = f::open(&path)?;
    let prior = control::fixtures::current(&mut active)?;
    fault.store(3, Ordering::SeqCst);
    assert!(matches!(
        active.install_snapshot(incoming_stage, SnapshotLimits::default()),
        Err(SnapshotError::Stage(StateTransactionError::Unavailable))
    ));
    assert_eq!(
        fault.load(Ordering::SeqCst),
        0,
        "reached the private source engine read"
    );
    assert_eq!(control::fixtures::current(&mut active)?, prior);
    let mut bytes = Vec::new();
    active.capture_snapshot(&mut bytes, SnapshotLimits::default())?;
    assert_eq!(bytes, s::frame((b"g", b"z"), &s::authority(&before)?)?);
    drop(active);
    s::assert_raw(&path, &before)
}

fn interrupted_limits(dir: &Path) -> TestResult {
    let path = dir.join("interrupt.redb");
    let mut state = s::empty(&path)?;
    let literal = s::hex(s::INITIAL)?;
    let exact = limits::profile((3, 325, 118), (326, 1), (2050, f::MIB), 2 * f::MIB)?;
    let mut sink = WriteFault(Vec::new(), usize::MAX, true);
    state.capture_snapshot(&mut sink, exact)?;
    assert_eq!(sink.0, literal);
    assert!(!sink.2);
    let short = limits::profile((3, 325, 118), (325, 1), (2050, f::MIB), 2 * f::MIB)?;
    let mut sink = WriteFault(Vec::new(), usize::MAX, true);
    assert!(matches!(
        state.capture_snapshot(&mut sink, short),
        Err(SnapshotError::Limit)
    ));
    assert_eq!(sink.0.len(), 324);
    control::fixtures::current(&mut state)?;
    drop(state);
    let scope = f::scope()?;
    for (calls, success) in [(327, true), (326, false)] {
        let cap = limits::profile((3, 325, 118), (calls, 1), (2050, f::MIB), 2 * f::MIB)?;
        let mut input = ReadFault(Cursor::new(literal.clone()), usize::MAX, true);
        let result = RedbSnapshotStage::receive(
            &dir.join(format!("input-{calls}.redb")),
            scope.group().clone(),
            scope.recovery().clone(),
            &mut input,
            StateStoreLimits::default(),
            cap,
        );
        if success {
            drop(result?.into_store()?);
        } else {
            assert!(matches!(result, Err(SnapshotError::Limit)));
        }
        assert_eq!(input.0.position(), 325);
        assert!(!input.2);
    }
    Ok(())
}

fn fresh_custody(dir: &Path) -> TestResult {
    let scope = f::scope()?;
    let literal = s::hex(s::INITIAL)?;
    let existing = dir.join("existing.bin");
    std::fs::write(&existing, b"keep original bytes")?;
    let mut input = Cursor::new(&literal);
    assert!(
        RedbSnapshotStage::receive(
            &existing,
            scope.group().clone(),
            scope.recovery().clone(),
            &mut input,
            StateStoreLimits::default(),
            SnapshotLimits::default()
        )
        .is_err()
    );
    assert_eq!(input.position(), 0);
    assert_eq!(std::fs::read(&existing)?, b"keep original bytes");
    let empty = dir.join("existing-empty.redb");
    drop(redb::Builder::new().create(&empty)?);
    let original = std::fs::read(&empty)?;
    let mut input = Cursor::new(&literal);
    assert!(
        RedbSnapshotStage::receive(
            &empty,
            scope.group().clone(),
            scope.recovery().clone(),
            &mut input,
            StateStoreLimits::default(),
            SnapshotLimits::default()
        )
        .is_err()
    );
    assert_eq!(input.position(), 0);
    assert_eq!(std::fs::read(&empty)?, original);
    let opaque = dir.join("opaque-empty.redb");
    let db = redb::Builder::new().create(&opaque)?;
    let incoming_stage = RedbSnapshotStage::receive_database(
        db,
        scope.group().clone(),
        scope.recovery().clone(),
        &mut Cursor::new(&literal),
        StateStoreLimits::default(),
        SnapshotLimits::default(),
    )?;
    drop(incoming_stage.into_store()?);
    let mut state = f::open(&opaque)?;
    let mut restored = Vec::new();
    state
        .current_snapshot(&mut restored, SnapshotLimits::default())?
        .ok_or("restored snapshot absent")?;
    assert_eq!(restored, literal);
    drop(state);
    let path = dir.join("nonempty.redb");
    drop(s::empty(&path)?);
    let before = s::raw(&path)?;
    let db = redb::Builder::new().open(&path)?;
    let mut input = Cursor::new(&literal);
    assert!(matches!(
        RedbSnapshotStage::receive_database(
            db,
            scope.group().clone(),
            scope.recovery().clone(),
            &mut input,
            StateStoreLimits::default(),
            SnapshotLimits::default()
        ),
        Err(SnapshotError::Stage(
            StateTransactionError::NamespaceConflict
        ))
    ));
    assert_eq!(input.position(), 0);
    s::assert_raw(&path, &before)?;
    let missing = dir.join("missing").join("stage.redb");
    assert!(
        RedbSnapshotStage::receive(
            &missing,
            scope.group().clone(),
            scope.recovery().clone(),
            &mut Cursor::new(literal),
            StateStoreLimits::default(),
            SnapshotLimits::default()
        )
        .is_err()
    );
    assert!(!missing.parent().ok_or("missing parent")?.exists());
    Ok(())
}

fn target(path: &Path) -> TestResult {
    let mut state = s::create(path)?;
    s::apply(&mut state, b"old-only", 0, 1)?;
    state.capture_snapshot(&mut Vec::new(), SnapshotLimits::default())?;
    s::apply(&mut state, b"uncaptured", 1, 2)?;
    Ok(())
}

#[test]
fn delegated_active_sync_uncertainty_fences_and_reconciles_whole_cohorts() -> TestResult {
    let dir = f::directory("snapshot-sync")?;
    let source = dir.join("source.redb");
    let source_data = s::authority(&s::seed(&source)?)?;
    let wire = s::capture(&source)?;
    for install in [false, true] {
        let path = dir.join(format!("{install}.redb"));
        target(&path)?;
        let before = s::raw(&path)?;
        let after = if install {
            s::installed(&source_data)?
        } else {
            s::installed(&s::authority(&before)?)?
        };
        let incoming_stage = if install {
            Some(s::stage(&dir.join("stage.redb"), &wire)?)
        } else {
            None
        };
        let fault = Arc::new(AtomicU8::new(0));
        let mut state = wrapped(&path, Arc::clone(&fault))?;
        fault.store(4, Ordering::SeqCst);
        let result = if let Some(incoming_stage) = incoming_stage {
            state.install_snapshot(incoming_stage, SnapshotLimits::default())
        } else {
            state.capture_snapshot(&mut Vec::new(), SnapshotLimits::default())
        };
        assert!(
            matches!(
                result,
                Err(SnapshotError::State(StateTransactionError::Uncertain))
            ),
            "install={install}"
        );
        assert_eq!(
            fault.load(Ordering::SeqCst),
            0,
            "actual delegated sync consumed the fault"
        );
        let mut output = Vec::new();
        assert!(matches!(
            state.current_snapshot(&mut output, SnapshotLimits::default()),
            Err(SnapshotError::State(StateTransactionError::Unavailable))
        ));
        assert_eq!(output, [] as [u8; 0]);
        assert!(control::fixtures::current(&mut state).is_err());
        drop(state);
        let actual = s::raw(&path)?;
        assert!(
            actual == before || actual == after,
            "mixed uncertain cohort, install={install}"
        );
        state = f::open(&path)?;
        if install {
            state.install_snapshot(
                s::stage(&dir.join("retry-stage.redb"), &wire)?,
                SnapshotLimits::default(),
            )?;
        } else {
            state.capture_snapshot(&mut Vec::new(), SnapshotLimits::default())?;
        }
        drop(state);
        s::assert_raw(&path, &after)?;
    }
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn snapshot_process_kill_boundaries() -> TestResult {
    if let Some(path) = std::env::var_os("WF_SNAPSHOT_CHILD_PATH") {
        return child(Path::new(&path));
    }
    let dir = f::directory("snapshot-kills")?;
    let source = dir.join("source.redb");
    let source_data = s::authority(&s::seed(&source)?)?;
    let wire = s::capture(&source)?;
    let image = dir.join("source.snapshot");
    std::fs::write(&image, &wire)?;
    for install in [false, true] {
        for after_return in [false, true] {
            let path = dir.join(format!("{install}-{after_return}.redb"));
            target(&path)?;
            let before = s::raw(&path)?;
            let after = if install {
                s::installed(&source_data)?
            } else {
                s::installed(&s::authority(&before)?)?
            };
            kill(&path, &image, install, after_return)?;
            let actual = s::raw(&path)?;
            if after_return {
                assert_eq!(actual, after);
            } else {
                assert!(actual == before || actual == after, "partial killed cohort");
            }
            let mut state = f::open(&path)?;
            if install {
                let stage_path = dir.join(format!("retry-{after_return}.redb"));
                state.install_snapshot(s::stage(&stage_path, &wire)?, SnapshotLimits::default())?;
            } else {
                state.capture_snapshot(&mut Vec::new(), SnapshotLimits::default())?;
            }
            drop(state);
            s::assert_raw(&path, &after)?;
        }
    }
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

fn child(path: &Path) -> TestResult {
    let install = std::env::var("WF_SNAPSHOT_CHILD_INSTALL")? == "1";
    let after = std::env::var("WF_SNAPSHOT_CHILD_AFTER")? == "1";
    let incoming_stage = if install {
        let image =
            std::fs::read(std::env::var_os("WF_SNAPSHOT_CHILD_IMAGE").ok_or("no child image")?)?;
        Some(s::stage(&path.with_extension("stage.redb"), &image)?)
    } else {
        None
    };
    let fault = Arc::new(AtomicU8::new(0));
    let mut state = wrapped(path, Arc::clone(&fault))?;
    if !after {
        fault.store(6, Ordering::SeqCst);
    }
    if let Some(incoming_stage) = incoming_stage {
        state.install_snapshot(incoming_stage, SnapshotLimits::default())?;
    } else {
        state.capture_snapshot(&mut Vec::new(), SnapshotLimits::default())?;
    }
    if !after {
        return Err("before-sync child unexpectedly returned".into());
    }
    parked("after-return")?;
    Ok(())
}

fn signal(reader: &mut impl Read, token: &[u8]) -> io::Result<()> {
    for _ in 0..32 {
        let mut line = [0; 256];
        let mut n = 0;
        loop {
            if n == line.len() {
                return Err(io::Error::other("oversized snapshot signal"));
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
    Err(io::Error::other("missing snapshot boundary"))
}

fn kill(path: &Path, image: &Path, install: bool, after: bool) -> TestResult {
    let mut child = Command::new(std::env::current_exe()?)
        .args([
            "--exact",
            "snapshot::faults::snapshot_process_kill_boundaries",
            "--nocapture",
            "--test-threads=1",
        ])
        .env("WF_SNAPSHOT_CHILD_PATH", path)
        .env("WF_SNAPSHOT_CHILD_IMAGE", image)
        .env("WF_SNAPSHOT_CHILD_INSTALL", if install { "1" } else { "0" })
        .env("WF_SNAPSHOT_CHILD_AFTER", if after { "1" } else { "0" })
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()?;
    let mut stdout = child.stdout.take().ok_or("child stdout")?;
    let mut stdin = child.stdin.take().ok_or("child stdin")?;
    let (sender, receiver) = mpsc::sync_channel(1);
    let reader = thread::spawn(move || {
        let phase = if after { "after-return" } else { "before-sync" };
        let first = signal(&mut stdout, format!("WFSTATE READY {phase}\n").as_bytes());
        let ready = first.is_ok();
        if sender.send(first).is_err() {
            return;
        }
        if ready {
            // A receiver that already timed out has nothing left to observe.
            let _unobserved = sender.send(signal(
                &mut stdout,
                format!("WFSTATE WAIT {phase}\n").as_bytes(),
            ));
        }
    });
    for phase in 0..2 {
        match receiver.recv_timeout(Duration::from_secs(20)) {
            Ok(Ok(())) => {}
            result => {
                child.kill()?;
                child.wait()?;
                reader.join().map_err(|_| "snapshot reader panicked")?;
                return Err(format!("snapshot handshake: {result:?}").into());
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
    reader.join().map_err(|_| "snapshot reader panicked")?;
    Ok(())
}
