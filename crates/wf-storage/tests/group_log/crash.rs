//! A process killed with SIGKILL while appending loses no acknowledged record,
//! including when the kill lands during segment rotation.

use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::thread;

use wf_storage::log::{GroupLog, GroupLogLimits};

use super::{TestResult, directory};

const CHILD_PATH: &str = "WF_GROUP_LOG_CHILD_PATH";
const CHILD_SEGMENT_BYTES: &str = "WF_GROUP_LOG_CHILD_SEGMENT_BYTES";
const APPENDERS: usize = 4;
const ACKS_BEFORE_KILL: usize = 2_000;

fn limits(segment_bytes: Option<u64>) -> GroupLogLimits {
    GroupLogLimits {
        max_segment_bytes: segment_bytes.unwrap_or(GroupLogLimits::default().max_segment_bytes),
        ..GroupLogLimits::default()
    }
}

/// Child body: appenders write `a{thread}-{n}` records forever and print each
/// one after its sync. Only runs when the parent sets the path variable.
fn child(path: &Path, limits: GroupLogLimits) -> TestResult {
    let (log, _) = GroupLog::open(path, limits)?;
    let log = Arc::new(log);
    let (sender, receiver) = std::sync::mpsc::channel::<String>();
    for appender in 0..APPENDERS {
        let log = Arc::clone(&log);
        let sender = sender.clone();
        thread::spawn(move || -> Result<(), String> {
            for index in 0_u64.. {
                let body = format!("a{appender}-{index}");
                let sequence = log
                    .append(body.clone().into_bytes())
                    .map_err(|e| e.to_string())?;
                log.wait_durable(sequence).map_err(|e| e.to_string())?;
                sender.send(body).map_err(|e| e.to_string())?;
            }
            Ok(())
        });
    }
    drop(sender);
    let mut stdout = std::io::stdout().lock();
    for body in receiver {
        writeln!(stdout, "ACK {body}")?;
        stdout.flush()?;
    }
    Ok(())
}

#[test]
fn crash_child() -> TestResult {
    let Some(path) = std::env::var_os(CHILD_PATH) else {
        return Ok(());
    };
    let segment_bytes = std::env::var(CHILD_SEGMENT_BYTES)
        .ok()
        .map(|bytes| bytes.parse())
        .transpose()?;
    child(Path::new(&path), limits(segment_bytes))
}

/// Kills a child after it acknowledged enough records, then checks that a
/// reopen replays every acknowledged record exactly once.
fn kill_and_recover(tag: &str, segment_bytes: Option<u64>) -> TestResult<Vec<u64>> {
    let directory = directory(tag)?;
    let path = directory.join("log");
    let mut command = Command::new(std::env::current_exe()?);
    command
        .args([
            "--exact",
            "crash::crash_child",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(CHILD_PATH, &path)
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit());
    if let Some(bytes) = segment_bytes {
        command.env(CHILD_SEGMENT_BYTES, bytes.to_string());
    }
    let mut process = command.spawn()?;
    let stdout = process.stdout.take().ok_or("child stdout")?;
    let mut acknowledged = Vec::new();
    for line in BufReader::new(stdout).lines() {
        if let Some(body) = line?.strip_prefix("ACK ") {
            acknowledged.push(body.as_bytes().to_vec());
        }
        if acknowledged.len() >= ACKS_BEFORE_KILL {
            break;
        }
    }
    process.kill()?;
    process.wait()?;
    assert_eq!(acknowledged.len(), ACKS_BEFORE_KILL, "child stopped early");
    let (_log, recovered) = GroupLog::open(&path, limits(segment_bytes))?;
    let kept: std::collections::BTreeSet<_> = recovered.records.iter().collect();
    let lost = acknowledged
        .iter()
        .filter(|body| !kept.contains(body))
        .count();
    assert_eq!(lost, 0, "{lost} acknowledged records lost");
    assert_eq!(
        kept.len(),
        recovered.records.len(),
        "a record was replayed twice"
    );
    std::fs::remove_dir_all(directory)?;
    Ok(recovered.segments)
}

#[test]
fn sigkill_loses_no_acknowledged_record() -> TestResult {
    kill_and_recover("crash", None)?;
    Ok(())
}

#[test]
fn sigkill_during_rotations_loses_no_acknowledged_record() -> TestResult {
    let segments = kill_and_recover("crash-rotate", Some(1_024))?;
    assert!(
        segments.len() > 10,
        "only {} segments: rotation was not exercised",
        segments.len()
    );
    Ok(())
}
