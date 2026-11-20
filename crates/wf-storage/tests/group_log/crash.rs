//! A process killed with SIGKILL while appending loses no acknowledged record.

use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::thread;

use wf_storage::log::{GroupLog, GroupLogLimits};

use super::{TestResult, directory};

const CHILD_PATH: &str = "WF_GROUP_LOG_CHILD_PATH";
const APPENDERS: usize = 4;
const ACKS_BEFORE_KILL: usize = 2_000;

/// Child body: appenders write `a{thread}-{n}` records forever and print each
/// one after its sync. Only runs when the parent sets the path variable.
fn child(path: &Path) -> TestResult {
    let (log, _) = GroupLog::open(path, GroupLogLimits::default())?;
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
    match std::env::var_os(CHILD_PATH) {
        Some(path) => child(Path::new(&path)),
        None => Ok(()),
    }
}

#[test]
fn sigkill_loses_no_acknowledged_record() -> TestResult {
    let directory = directory("crash")?;
    let path = directory.join("node.log");
    let mut process = Command::new(std::env::current_exe()?)
        .args([
            "--exact",
            "crash::crash_child",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(CHILD_PATH, &path)
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()?;
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
    let (_log, recovered) = GroupLog::open(&path, GroupLogLimits::default())?;
    let kept: std::collections::BTreeSet<_> = recovered.records.iter().collect();
    let lost: Vec<_> = acknowledged
        .iter()
        .filter(|body| !kept.contains(body))
        .collect();
    assert!(lost.is_empty(), "{} acknowledged records lost", lost.len());
    assert_eq!(
        kept.len(),
        recovered.records.len(),
        "a record was replayed twice"
    );
    std::fs::remove_dir_all(directory)?;
    Ok(())
}
