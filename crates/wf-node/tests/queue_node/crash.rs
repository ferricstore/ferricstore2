//! A node killed with SIGKILL keeps every Flow whose create it acknowledged:
//! replies leave only after the log sync.

use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Command, Stdio};

use super::{TestResult, create, directory, open};

const CHILD_PATH: &str = "WF_QUEUE_NODE_CHILD_PATH";
const ACKS_BEFORE_KILL: usize = 3_000;

/// Child body: creates batches of Flows forever and prints each id after
/// its acknowledgement.
fn child(path: &Path) -> TestResult {
    let node = open(path, 8)?;
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
fn sigkill_keeps_every_acknowledged_create() -> TestResult {
    let directory = directory("crash")?;
    let mut process = Command::new(std::env::current_exe()?)
        .args([
            "--exact",
            "crash::crash_child",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(CHILD_PATH, &directory)
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
    let node = open(&directory, 8)?;
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
