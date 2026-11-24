//! A Flow's log cost per change stays flat as it ages (perf note P2): a
//! change logs the Flow's after-image, which must not carry every value
//! version the Flow kept, or a Flow that replaces its payload on each
//! change logs quadratically many payload bytes.

use wf_application::commands::flow::{self, FlowService, Term};
use wf_node::composition::{QueueNode, QueueNodeConfig};

use super::{TestResult, call, directory, open, open_with};

fn map(entries: &[(&str, Term)]) -> Term {
    Term::Map(
        entries
            .iter()
            .map(|(key, value)| (key.as_bytes().to_vec(), value.clone()))
            .collect(),
    )
}

fn run(node: &QueueNode, name: &str, entries: &[(&str, Term)]) -> TestResult<Term> {
    let command = flow::parse(name, &map(entries))?;
    Ok(call(|reply| node.execute(command, reply))??)
}

fn log_bytes(path: &std::path::Path) -> TestResult<u64> {
    let mut total = 0;
    for entry in std::fs::read_dir(path.join("log"))? {
        total += entry?.metadata()?.len();
    }
    Ok(total)
}

fn create(node: &QueueNode) -> TestResult<Term> {
    run(
        node,
        "FLOW.CREATE",
        &[
            ("id", Term::text("grow")),
            ("type", Term::text("grow")),
            ("state", Term::text("a")),
            ("partition_key", Term::text("p")),
            ("payload", Term::text("payload-0")),
            ("run_at_ms", Term::Int(9_000_000)),
            ("now_ms", Term::Int(1_000)),
        ],
    )
}

/// Moves the Flow from `a` to `b` or back, replacing its payload.
fn replace(node: &QueueNode, change: i64, payload: Vec<u8>) -> TestResult<Term> {
    let (from, to) = if change % 2 == 0 {
        ("a", "b")
    } else {
        ("b", "a")
    };
    run(
        node,
        "FLOW.TRANSITION",
        &[
            ("id", Term::text("grow")),
            ("partition_key", Term::text("p")),
            ("from_state", Term::text(from)),
            ("to_state", Term::text(to)),
            ("fencing_token", Term::Int(0)),
            ("payload", Term::Bin(payload)),
            ("run_at_ms", Term::Int(9_000_000)),
            ("now_ms", Term::Int(1_001 + change)),
        ],
    )
}

fn payload_ref(node: &QueueNode) -> TestResult<Term> {
    let flow = run(
        node,
        "FLOW.GET",
        &[
            ("id", Term::text("grow")),
            ("partition_key", Term::text("p")),
        ],
    )?;
    Ok(flow.get(b"payload_ref").cloned().unwrap_or(Term::Nil))
}

#[test]
fn every_kept_version_reads_after_the_log_replays() -> TestResult {
    // A logged change carries only the versions it kept; replay keeps the
    // ones earlier changes logged.
    let path = directory("log-growth-replay")?;
    let config = || QueueNodeConfig {
        shards: 1,
        snapshot_segments: 0,
        ..QueueNodeConfig::default()
    };
    let node = open_with(&path, config())?;
    create(&node)?;
    let mut refs = vec![payload_ref(&node)?];
    let mut payloads = vec![Term::text("payload-0")];
    for change in 0..6_i64 {
        let payload = format!("payload-{}", change + 1);
        replace(&node, change, payload.clone().into_bytes())?;
        refs.push(payload_ref(&node)?);
        payloads.push(Term::text(&payload));
    }
    let read = |node: &QueueNode| {
        run(
            node,
            "FLOW.VALUE.MGET",
            &[("refs", Term::List(refs.clone()))],
        )
    };
    assert_eq!(read(&node)?, Term::List(payloads.clone()));
    drop(node);
    let node = open_with(&path, config())?;
    assert_eq!(read(&node)?, Term::List(payloads));
    drop(node);
    std::fs::remove_dir_all(path)?;
    Ok(())
}

#[test]
fn a_change_logs_about_one_payload_however_many_versions_were_kept() -> TestResult {
    let path = directory("log-growth")?;
    let node = open(&path, 1)?;
    create(&node)?;
    let mut deltas = Vec::new();
    for change in 0..60_i64 {
        let before = log_bytes(&path)?;
        let byte = b'x'.wrapping_add(u8::try_from(change % 20)?);
        replace(&node, change, vec![byte; 1_024])?;
        deltas.push(log_bytes(&path)?.saturating_sub(before));
    }
    let first: u64 = deltas[1..11].iter().sum();
    let last: u64 = deltas[50..60].iter().sum();
    assert!(
        last < first.saturating_mul(2),
        "the last ten changes logged {last} bytes, the first ten {first}"
    );
    drop(node);
    std::fs::remove_dir_all(path)?;
    Ok(())
}

#[test]
fn kept_versions_survive_a_snapshot_and_a_move_to_disk() -> TestResult {
    // Only logged after-images carry the newly kept versions; a snapshot
    // and a cold file hold every one.
    let path = directory("log-growth-snapshot")?;
    let config = || QueueNodeConfig {
        shards: 1,
        snapshot_segments: 0,
        ..QueueNodeConfig::default()
    };
    let node = open_with(&path, config())?;
    create(&node)?;
    let mut refs = vec![payload_ref(&node)?];
    let mut payloads = vec![Term::text("payload-0")];
    let mut change = |node: &QueueNode, at: i64| -> TestResult {
        let payload = format!("payload-{}", at + 1);
        replace(node, at, payload.clone().into_bytes())?;
        refs.push(payload_ref(node)?);
        payloads.push(Term::text(&payload));
        Ok(())
    };
    for at in 0..3 {
        change(&node, at)?;
    }
    assert_eq!(node.snapshot()?, 1);
    change(&node, 3)?;
    drop(node);
    let read = |node: &QueueNode| {
        run(
            node,
            "FLOW.VALUE.MGET",
            &[("refs", Term::List(refs.clone()))],
        )
    };
    let node = open_with(&path, config())?;
    assert_eq!(read(&node)?, Term::List(payloads.clone()));
    run(
        &node,
        "FLOW.CANCEL",
        &[
            ("id", Term::text("grow")),
            ("partition_key", Term::text("p")),
            ("fencing_token", Term::Int(0)),
            ("now_ms", Term::Int(2_000)),
        ],
    )?;
    // Reopened, the node moves the finished Flow to disk before serving,
    // so the read comes from its cold file.
    drop(node);
    let node = open_with(&path, config())?;
    assert_eq!(read(&node)?, Term::List(payloads));
    drop(node);
    std::fs::remove_dir_all(path)?;
    Ok(())
}
