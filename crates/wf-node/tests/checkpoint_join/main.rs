//! Checkpoint consistency on real disk; service readiness requires later replay.

#![forbid(unsafe_code)]

use std::{
    path::Path,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

use wf_consensus::recovery::{CheckpointJoinLimits, verify_checkpoint};
use wf_ports::log::{RaftLogEntry, RaftLogStore};

mod fixtures;
mod identity;
mod limits;

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

#[test]
fn checkpoint_join_kill_boundaries() -> TestResult {
    if let Some(dir) = std::env::var_os("WF_JOIN_CHILD_DIR") {
        return child(Path::new(&dir));
    }
    for phase in ["append", "committed", "applied"] {
        let dir = fixtures::directory(phase)?;
        let mut child = Command::new(std::env::current_exe()?)
            .args(["--exact", "checkpoint_join_kill_boundaries", "--nocapture"])
            .env("WF_JOIN_CHILD_DIR", &dir)
            .env("WF_JOIN_CHILD_PHASE", phase)
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .spawn()?;
        let start = Instant::now();
        loop {
            if dir.join("ready").exists() && std::fs::read(dir.join("ready"))? == phase.as_bytes() {
                break;
            }
            if let Some(status) = child.try_wait()? {
                return Err(format!("child exited before {phase}: {status}").into());
            }
            if start.elapsed() > Duration::from_secs(20) {
                child.kill()?;
                child.wait()?;
                return Err(format!("child handshake timeout at {phase}").into());
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(child.try_wait()?.is_none());
        child.kill()?;
        assert!(!child.wait()?.success());
        let (log, mut state) = fixtures::open_pair(&dir)?;
        let expected_control = fixtures::control(&mut state)?;
        let scope = fixtures::scope()?;
        let (mut log, mut state) = verify_checkpoint(
            log,
            state,
            scope.group(),
            scope.recovery(),
            CheckpointJoinLimits::default(),
        )?;
        let meta = log.state()?;
        assert_eq!(meta.last_log(), Some(fixtures::id(1, 7, 0)));
        assert_eq!(
            meta.committed(),
            if phase == "append" {
                None
            } else {
                Some(fixtures::id(1, 7, 0))
            }
        );
        assert_eq!(expected_control.last_entry().is_some(), phase == "applied");
        fixtures::assert_authority(&mut state, expected_control, phase == "applied")?;
        fixtures::assert_log(&mut log, meta, &[&fixtures::frame(b"i", b"q")?])?;
        drop((log, state));
        let (log, state) = fixtures::open_pair(&dir)?;
        drop(verify_checkpoint(
            log,
            state,
            scope.group(),
            scope.recovery(),
            CheckpointJoinLimits::default(),
        )?);
        std::fs::remove_dir_all(dir)?;
    }
    Ok(())
}

fn child(dir: &Path) -> TestResult {
    use std::io::Write as _;
    let phase = std::env::var("WF_JOIN_CHILD_PHASE")?;
    let (mut log, mut state) = fixtures::create_pair(dir)?;
    let frame = fixtures::frame(b"i", b"q")?;
    log.append(vec![RaftLogEntry::new(
        fixtures::id(1, 7, 0),
        frame.as_bytes().to_vec(),
    )])?;
    if phase != "append" {
        log.save_committed(fixtures::id(1, 7, 0))?;
    }
    if phase == "applied" {
        fixtures::apply(&mut state, &frame, fixtures::id(1, 7, 0))?;
    }
    let mut signal = std::fs::File::create(dir.join("ready"))?;
    signal.write_all(phase.as_bytes())?;
    signal.sync_all()?;
    // Keep both actual database handles alive until the parent terminates us.
    loop {
        std::thread::park();
    }
}
