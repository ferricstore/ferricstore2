//! A node killed with SIGKILL keeps every Flow whose create it acknowledged:
//! replies leave only after the log sync. The second run also completes
//! Flows and takes snapshots, so kills land during snapshots (ADR-013).

use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Command, Stdio};

use wf_node::composition::QueueNodeConfig;
use wf_storage::log::GroupLogLimits;

use super::{TestResult, claim, complete, completion, create, directory, open_with};

const CHILD_PATH: &str = "WF_QUEUE_NODE_CHILD_PATH";
const SNAPSHOT_CHILD_PATH: &str = "WF_QUEUE_NODE_SNAPSHOT_CHILD_PATH";
const ACKS_BEFORE_KILL: usize = 3_000;

fn config(snapshots: bool) -> QueueNodeConfig {
    if !snapshots {
        return QueueNodeConfig {
            shards: 8,
            ..QueueNodeConfig::default()
        };
    }
    QueueNodeConfig {
        shards: 8,
        log: GroupLogLimits {
            max_segment_bytes: 4_096,
            ..QueueNodeConfig::default().log
        },
        snapshot_segments: 2,
        ..QueueNodeConfig::default()
    }
}

/// Child body: creates batches of Flows forever and prints each id after
/// its acknowledgement; with snapshots it also completes them and takes a
/// snapshot every fifth batch.
fn child(path: &Path, snapshots: bool) -> TestResult {
    let node = open_with(path, config(snapshots))?;
    let mut stdout = std::io::stdout().lock();
    for batch in 0_u64.. {
        let ids: Vec<String> = (0..50).map(|index| format!("c{batch}-{index}")).collect();
        let id_refs: Vec<&str> = ids.iter().map(String::as_str).collect();
        let outcomes = create(&node, &id_refs)?;
        for (id, outcome) in ids.iter().zip(outcomes) {
            if outcome.is_ok() {
                writeln!(stdout, "ACK {id}")?;
            }
        }
        stdout.flush()?;
        if snapshots {
            let jobs = claim(&node, 25)?;
            complete(&node, jobs.iter().map(completion).collect())?;
            if batch % 5 == 4 {
                node.snapshot()?;
            }
        }
    }
    Ok(())
}

#[test]
fn crash_child() -> TestResult {
    match std::env::var_os(CHILD_PATH) {
        Some(path) => child(Path::new(&path), false),
        None => Ok(()),
    }
}

#[test]
fn snapshot_crash_child() -> TestResult {
    match std::env::var_os(SNAPSHOT_CHILD_PATH) {
        Some(path) => child(Path::new(&path), true),
        None => Ok(()),
    }
}

fn kill_and_check(tag: &str, child_test: &str, variable: &str, snapshots: bool) -> TestResult {
    let directory = directory(tag)?;
    let mut process = Command::new(std::env::current_exe()?)
        .args(["--exact", child_test, "--nocapture", "--test-threads=1"])
        .env(variable, &directory)
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()?;
    let stdout = process.stdout.take().ok_or("child stdout")?;
    let mut acknowledged = Vec::new();
    for line in BufReader::new(stdout).lines() {
        if let Some(id) = line?.strip_prefix("ACK ") {
            acknowledged.push(id.to_owned());
        }
        if acknowledged.len() >= ACKS_BEFORE_KILL {
            break;
        }
    }
    process.kill()?;
    process.wait()?;
    assert_eq!(acknowledged.len(), ACKS_BEFORE_KILL, "child stopped early");
    let node = open_with(&directory, config(snapshots))?;
    let id_refs: Vec<&str> = acknowledged.iter().map(String::as_str).collect();
    let again = create(&node, &id_refs)?;
    let lost = again.iter().filter(|outcome| outcome.is_ok()).count();
    assert_eq!(
        lost, 0,
        "{lost} acknowledged creates were missing after restart"
    );
    drop(node);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

#[test]
fn sigkill_keeps_every_acknowledged_create() -> TestResult {
    kill_and_check("crash", "crash::crash_child", CHILD_PATH, false)
}

#[test]
fn sigkill_during_snapshots_keeps_every_acknowledged_create() -> TestResult {
    kill_and_check(
        "crash-snapshot",
        "crash::snapshot_crash_child",
        SNAPSHOT_CHILD_PATH,
        true,
    )
}
