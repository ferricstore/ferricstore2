use std::{
    path::Path,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

use super::{
    CommittedRecoveryError as Error, CommittedRecoveryLimits as Limits, TestResult, fixtures,
    limits, recover_committed,
};
use wf_ports::log::{RaftLogError, RaftLogStore};

#[test]
fn committed_recovery_kill_after_durable_prefix_refusal_and_success() -> TestResult {
    if let Some(dir) = std::env::var_os("WF_RECOVERY_CHILD_DIR") {
        return child(Path::new(&dir));
    }
    for phase in ["prefix", "success"] {
        let reference = fixtures::directory("kill-reference")?;
        let (log, mut state) = fixtures::create_pair(&reference)?;
        let first = fixtures::frame(5, b"a", 1, vec![(b"i", fixtures::create_op())])?;
        let second = fixtures::frame(5, b"b", 2, vec![(b"j", fixtures::create_op())])?;
        drop(fixtures::apply_normal(&mut state, 0, first.as_bytes(), 5)?);
        if phase == "success" {
            drop(fixtures::apply_normal(&mut state, 1, second.as_bytes(), 5)?);
        }
        drop((log, state));
        let rows = fixtures::state_rows(&reference)?;
        let meta = fixtures::state_meta(&reference)?;
        let dir = fixtures::directory(phase)?;
        let mut process = Command::new(std::env::current_exe()?)
            .args([
                "--exact",
                "faults::committed_recovery_kill_after_durable_prefix_refusal_and_success",
                "--nocapture",
            ])
            .env("WF_RECOVERY_CHILD_DIR", &dir)
            .env("WF_RECOVERY_CHILD_PHASE", phase)
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .spawn()?;
        let start = Instant::now();
        loop {
            if dir.join("ready").exists() && std::fs::read(dir.join("ready"))? == phase.as_bytes() {
                break;
            }
            if let Some(status) = process.try_wait()? {
                return Err(format!("child exited at {phase}: {status}").into());
            }
            if start.elapsed() > Duration::from_secs(20) {
                process.kill()?;
                process.wait()?;
                return Err(format!("recovery handshake timeout at {phase}").into());
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(process.try_wait()?.is_none());
        process.kill()?;
        assert!(!process.wait()?.success());
        drop(fixtures::open_pair(&dir)?);
        assert_eq!(fixtures::state_rows(&dir)?, rows);
        assert_eq!(fixtures::state_meta(&dir)?, meta);
        let logs = fixtures::log_rows(&dir)?;
        let log_meta = fixtures::log_meta(&dir)?;
        assert_eq!(logs.len(), 2);
        let (log, state) = fixtures::open_pair(&dir)?;
        let s = fixtures::scope()?;
        let (mut log, mut state) =
            recover_committed(log, state, s.group(), s.recovery(), limits()?)?;
        assert_eq!(log.state()?.committed(), Some(fixtures::at(1)));
        assert_eq!(
            fixtures::applied(&mut state)?
                .0
                .last_entry()
                .map(wf_domain::workflow::control::EntryIdentity::log),
            Some(fixtures::at(1))
        );
        drop((log, state));
        if phase == "prefix" {
            let (log, mut state) = fixtures::open_pair(&reference)?;
            drop(fixtures::apply_normal(&mut state, 1, second.as_bytes(), 5)?);
            drop((log, state));
        }
        assert_eq!(
            fixtures::state_rows(&dir)?,
            fixtures::state_rows(&reference)?
        );
        assert_eq!(
            fixtures::state_meta(&dir)?,
            fixtures::state_meta(&reference)?
        );
        assert_eq!(fixtures::log_rows(&dir)?, logs);
        assert_eq!(fixtures::log_meta(&dir)?, log_meta);
        std::fs::remove_dir_all(dir)?;
        std::fs::remove_dir_all(reference)?;
    }
    Ok(())
}

fn child(dir: &Path) -> TestResult {
    use std::io::Write as _;
    let phase = std::env::var("WF_RECOVERY_CHILD_PHASE")?;
    let (mut log, state) = fixtures::create_pair(dir)?;
    let first = fixtures::frame(5, b"a", 1, vec![(b"i", fixtures::create_op())])?;
    let second = fixtures::frame(5, b"b", 2, vec![(b"j", fixtures::create_op())])?;
    fixtures::append(&mut log, 0, first.as_bytes())?;
    fixtures::append(&mut log, 1, second.as_bytes())?;
    log.save_committed(fixtures::at(1))?;
    let base = limits()?;
    let cap = Limits::new(
        base.membership_limits(),
        (
            base.maximum_state_read_steps(),
            base.maximum_applied_state_bytes(),
            base.maximum_control_apply_bytes(),
            base.maximum_temporary_bytes(),
        ),
        base.normal_limits(),
        (
            2,
            if phase == "prefix" {
                first.as_bytes().len()
            } else {
                first.as_bytes().len() + second.as_bytes().len()
            },
        ),
        base.maximum_owned_entry_bytes(),
        base.workspace_bytes(),
    )?;
    let s = fixtures::scope()?;
    let result = recover_committed(log, state, s.group(), s.recovery(), cap);
    let owners = if phase == "prefix" {
        let Err(error) = result else {
            return Err("prefix refusal unexpectedly succeeded".into());
        };
        assert_eq!(error, Error::Log(RaftLogError::Limit));
        None
    } else {
        Some(result?)
    };
    let mut signal = std::fs::File::create(dir.join("ready"))?;
    signal.write_all(phase.as_bytes())?;
    signal.sync_all()?;
    // The parent kills only after the completed call; no mid-engine claim.
    std::hint::black_box(&owners);
    loop {
        std::thread::park();
    }
}
