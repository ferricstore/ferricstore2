//! Replaced value versions of a running Flow on disk (ADR-016 phase D):
//! once a Flow keeps more replaced bytes than the node's memory budget for
//! them, its kept versions move to the cold files at the end of a round,
//! and every read of them (by reference, in history, by a rewind) still
//! finds them, across a snapshot, a crash and the Flow's own move to disk.

use std::path::Path;
use std::time::{Duration, Instant};

use wf_application::commands::flow::{self, FlowService, Term};
use wf_node::composition::{QueueNode, QueueNodeConfig};

use super::{TestResult, call, directory, open_with};

/// Every replaced version above 64 bytes goes to disk.
fn config() -> QueueNodeConfig {
    QueueNodeConfig {
        shards: 2,
        snapshot_segments: 0,
        kept_memory_bytes: 0,
        ..QueueNodeConfig::default()
    }
}

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

fn field(term: &Term, key: &str) -> Term {
    term.get(key.as_bytes()).cloned().unwrap_or(Term::Nil)
}

fn cold_bytes(path: &Path) -> TestResult<u64> {
    let mut total = 0;
    for entry in std::fs::read_dir(path.join("cold"))? {
        total += entry?.metadata()?.len();
    }
    Ok(total)
}

fn cold_files(path: &Path) -> TestResult<Vec<std::ffi::OsString>> {
    let mut names = Vec::new();
    for entry in std::fs::read_dir(path.join("cold"))? {
        names.push(entry?.file_name());
    }
    Ok(names)
}

/// Bytes in the cold files not among `before`.
fn new_cold_bytes(path: &Path, before: &[std::ffi::OsString]) -> TestResult<u64> {
    let mut total = 0;
    for entry in std::fs::read_dir(path.join("cold"))? {
        let entry = entry?;
        if !before.contains(&entry.file_name()) {
            total += entry.metadata()?.len();
        }
    }
    Ok(total)
}

/// Every snapshot file's bytes.
fn snapshot_bytes(path: &Path) -> TestResult<Vec<u8>> {
    let mut bytes = Vec::new();
    for entry in std::fs::read_dir(path.join("snapshots"))? {
        bytes.extend(std::fs::read(entry?.path())?);
    }
    Ok(bytes)
}

/// Waits until the cold files hold at least `bytes`, then for the shard
/// to take the writer's answer.
fn wait_for_cold(path: &Path, bytes: u64) -> TestResult {
    let deadline = Instant::now() + Duration::from_secs(10);
    while cold_bytes(path)? < bytes {
        if Instant::now() > deadline {
            return Err(format!("cold files hold {} of {bytes} bytes", cold_bytes(path)?).into());
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    // The shard reads the writer's answers at its next round or idle tick.
    std::thread::sleep(Duration::from_millis(600));
    Ok(())
}

fn payload(index: usize) -> Vec<u8> {
    format!("payload-{index:04}-").into_bytes().repeat(64)
}

fn create(node: &QueueNode) -> TestResult<Term> {
    run(
        node,
        "FLOW.CREATE",
        &[
            ("id", Term::text("kept")),
            ("type", Term::text("kept")),
            ("state", Term::text("a")),
            ("partition_key", Term::text("p")),
            ("payload", Term::Bin(payload(0))),
            ("run_at_ms", Term::Int(9_000_000)),
            ("now_ms", Term::Int(1_000)),
        ],
    )
}

/// Moves the Flow from `a` to `b` or back, replacing its payload.
fn replace(node: &QueueNode, change: usize) -> TestResult<Term> {
    let (from, to) = if change.is_multiple_of(2) {
        ("a", "b")
    } else {
        ("b", "a")
    };
    run(
        node,
        "FLOW.TRANSITION",
        &[
            ("id", Term::text("kept")),
            ("partition_key", Term::text("p")),
            ("from_state", Term::text(from)),
            ("to_state", Term::text(to)),
            ("fencing_token", Term::Int(0)),
            ("payload", Term::Bin(payload(change + 1))),
            ("run_at_ms", Term::Int(9_000_000)),
            ("now_ms", Term::Int(1_001 + i64::try_from(change)?)),
        ],
    )
}

fn get(node: &QueueNode) -> TestResult<Term> {
    run(
        node,
        "FLOW.GET",
        &[
            ("id", Term::text("kept")),
            ("partition_key", Term::text("p")),
            ("full", Term::Bool(true)),
        ],
    )
}

/// The Flow's references and payloads after it was created and replaced
/// its payload `changes` times.
struct Kept {
    refs: Vec<Term>,
    payloads: Vec<Term>,
}

impl Kept {
    fn new(node: &QueueNode) -> TestResult<Self> {
        create(node)?;
        let mut kept = Self {
            refs: Vec::new(),
            payloads: Vec::new(),
        };
        kept.note(node, 0)?;
        Ok(kept)
    }

    fn note(&mut self, node: &QueueNode, index: usize) -> TestResult {
        self.refs.push(field(&get(node)?, "payload_ref"));
        self.payloads.push(Term::Bin(payload(index)));
        Ok(())
    }

    fn replace(&mut self, node: &QueueNode, changes: std::ops::Range<usize>) -> TestResult {
        for change in changes {
            replace(node, change)?;
            self.note(node, change + 1)?;
        }
        Ok(())
    }

    /// Every version reads by reference and in history.
    fn assert_read(&self, node: &QueueNode) -> TestResult {
        let read = run(
            node,
            "FLOW.VALUE.MGET",
            &[("refs", Term::List(self.refs.clone()))],
        )?;
        assert_eq!(read, Term::List(self.payloads.clone()));
        let Term::List(events) = run(
            node,
            "FLOW.HISTORY",
            &[
                ("id", Term::text("kept")),
                ("partition_key", Term::text("p")),
                ("values", Term::Bool(true)),
            ],
        )?
        else {
            return Err("history is not a list".into());
        };
        let shown: Vec<Term> = events
            .iter()
            .filter_map(|event| match event {
                Term::List(pair) => pair.get(1).map(|fields| field(fields, "payload")),
                _ => None,
            })
            .take(self.payloads.len())
            .collect();
        assert_eq!(shown, self.payloads, "history shows each change's payload");
        Ok(())
    }

    fn replaced_bytes(&self) -> u64 {
        let replaced = self.payloads.len().saturating_sub(1);
        self.payloads
            .iter()
            .take(replaced)
            .map(|payload| match payload {
                Term::Bin(bytes) => bytes.len() as u64,
                _ => 0,
            })
            .sum()
    }
}

#[test]
fn a_running_flow_s_replaced_values_go_to_disk_and_still_read() -> TestResult {
    let path = directory("kept-on-disk")?;
    let node = open_with(&path, config())?;
    let mut kept = Kept::new(&node)?;
    kept.replace(&node, 0..10)?;
    wait_for_cold(&path, kept.replaced_bytes())?;
    kept.assert_read(&node)?;
    // A rewind restores a payload read back from disk.
    let Term::List(events) = run(
        &node,
        "FLOW.HISTORY",
        &[
            ("id", Term::text("kept")),
            ("partition_key", Term::text("p")),
        ],
    )?
    else {
        return Err("history is not a list".into());
    };
    let Some(Term::List(third)) = events.get(2) else {
        return Err("no third event".into());
    };
    let event = third.first().cloned().unwrap_or(Term::Nil);
    run(
        &node,
        "FLOW.REWIND",
        &[
            ("id", Term::text("kept")),
            ("partition_key", Term::text("p")),
            ("to_event", event),
        ],
    )?;
    assert_eq!(field(&get(&node)?, "payload"), Term::Bin(payload(2)));
    drop(node);
    std::fs::remove_dir_all(path)?;
    Ok(())
}

/// The default budget is 16 KiB a Flow: 19 kept 832-byte payloads
/// (15,808 bytes) stay in memory, the 20th (16,640) sends them to disk.
#[test]
fn kept_versions_stay_in_memory_up_to_the_default_budget() -> TestResult {
    let path = directory("kept-in-memory")?;
    let node = open_with(
        &path,
        QueueNodeConfig {
            kept_memory_bytes: QueueNodeConfig::default().kept_memory_bytes,
            ..config()
        },
    )?;
    let mut kept = Kept::new(&node)?;
    kept.replace(&node, 0..19)?;
    assert_eq!(kept.replaced_bytes(), 15_808);
    std::thread::sleep(Duration::from_millis(600));
    assert_eq!(cold_bytes(&path)?, 0, "nothing went to disk");
    kept.assert_read(&node)?;
    kept.replace(&node, 19..20)?;
    wait_for_cold(&path, kept.replaced_bytes())?;
    kept.assert_read(&node)?;
    drop(node);
    std::fs::remove_dir_all(path)?;
    Ok(())
}

#[test]
fn kept_versions_on_disk_survive_a_snapshot_a_crash_and_the_flow_s_move_to_disk() -> TestResult {
    let path = directory("kept-on-disk-restart")?;
    let mut kept = {
        let node = open_with(&path, config())?;
        let mut kept = Kept::new(&node)?;
        kept.replace(&node, 0..10)?;
        kept
    };
    {
        // Replay keeps the versions in memory; the restart drops the files
        // no snapshot names and moves the versions to disk again, and the
        // snapshot names where they are, without their bytes.
        let before = cold_files(&path)?;
        let node = open_with(&path, config())?;
        let deadline = Instant::now() + Duration::from_secs(10);
        while new_cold_bytes(&path, &before)? < kept.replaced_bytes() {
            assert!(
                Instant::now() < deadline,
                "the replayed versions went to disk again"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
        std::thread::sleep(Duration::from_millis(600));
        kept.assert_read(&node)?;
        node.snapshot()?;
        let snapshot = snapshot_bytes(&path)?;
        for (index, replaced) in kept.payloads.iter().enumerate().take(10) {
            let Term::Bin(replaced) = replaced else {
                return Err("payloads are bytes".into());
            };
            assert!(
                !snapshot
                    .windows(replaced.len())
                    .any(|window| window == replaced),
                "the snapshot carries replaced payload {index}"
            );
        }
        kept.replace(&node, 10..11)?;
        kept.assert_read(&node)?;
    }
    {
        // From the snapshot and the change after it, whose logged image
        // carries the version it kept: the restart moves that one to disk
        // again. Then the Flow ends and moves to its own cold file, reading
        // its versions from disk.
        let before = cold_files(&path)?;
        let node = open_with(&path, config())?;
        let deadline = Instant::now() + Duration::from_secs(10);
        while new_cold_bytes(&path, &before)? < kept.replaced_bytes() / 11 {
            assert!(
                Instant::now() < deadline,
                "the replayed version went to disk again"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
        kept.assert_read(&node)?;
        run(
            &node,
            "FLOW.CANCEL",
            &[
                ("id", Term::text("kept")),
                ("partition_key", Term::text("p")),
                ("fencing_token", Term::Int(0)),
                ("now_ms", Term::Int(2_000)),
            ],
        )?;
        assert_eq!(field(&get(&node)?, "state"), Term::text("cancelled"));
        kept.assert_read(&node)?;
    }
    // The snapshot still names the files the versions were in.
    let node = open_with(&path, config())?;
    assert_eq!(field(&get(&node)?, "state"), Term::text("cancelled"));
    kept.assert_read(&node)?;
    // A newer snapshot names only the Flow's own cold file: the files its
    // versions were in go.
    node.snapshot()?;
    let files = std::fs::read_dir(path.join("cold"))?.count();
    assert_eq!(files, 1, "only the finished Flow's file is left");
    kept.assert_read(&node)?;
    drop(node);
    std::fs::remove_dir_all(path)?;
    Ok(())
}

#[test]
fn small_kept_versions_stay_in_memory() -> TestResult {
    // A version of under 64 bytes costs about as much on disk as in memory.
    let path = directory("kept-small")?;
    let node = open_with(&path, config())?;
    create(&node)?;
    for change in 0..10 {
        let (from, to) = if change % 2 == 0 {
            ("a", "b")
        } else {
            ("b", "a")
        };
        run(
            &node,
            "FLOW.TRANSITION",
            &[
                ("id", Term::text("kept")),
                ("partition_key", Term::text("p")),
                ("from_state", Term::text(from)),
                ("to_state", Term::text(to)),
                ("fencing_token", Term::Int(0)),
                ("payload", Term::text(&format!("small-{change}"))),
                ("run_at_ms", Term::Int(9_000_000)),
                ("now_ms", Term::Int(1_001 + change)),
            ],
        )?;
    }
    std::thread::sleep(Duration::from_millis(600));
    // Only the first payload, replaced once, is large enough.
    assert_eq!(
        cold_bytes(&path)?,
        u64::try_from(payload(0).len())? + 8,
        "one record: its header and bytes"
    );
    drop(node);
    std::fs::remove_dir_all(path)?;
    Ok(())
}

/// Versions handed to the writer are on their way only until it answers:
/// a Flow created again under a removed Flow's id keeps versions under the
/// same names, and they go to disk too.
#[test]
fn a_flow_created_again_under_a_removed_one_s_id_moves_its_versions_to_disk() -> TestResult {
    let path = directory("kept-again")?;
    let node = open_with(
        &path,
        QueueNodeConfig {
            retention_ms: 200,
            ..config()
        },
    )?;
    let mut first = Kept::new(&node)?;
    first.replace(&node, 0..3)?;
    wait_for_cold(&path, first.replaced_bytes())?;
    run(
        &node,
        "FLOW.CANCEL",
        &[
            ("id", Term::text("kept")),
            ("partition_key", Term::text("p")),
            ("fencing_token", Term::Int(0)),
            ("now_ms", Term::Int(2_000)),
        ],
    )?;
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut again = loop {
        if let Ok(again) = Kept::new(&node) {
            break again;
        }
        assert!(
            Instant::now() < deadline,
            "the cancelled Flow was never removed"
        );
        std::thread::sleep(Duration::from_millis(50));
    };
    std::thread::sleep(Duration::from_millis(600));
    let before = cold_bytes(&path)?;
    again.replace(&node, 0..3)?;
    wait_for_cold(&path, before + again.replaced_bytes())?;
    again.assert_read(&node)?;
    drop(node);
    std::fs::remove_dir_all(path)?;
    Ok(())
}
