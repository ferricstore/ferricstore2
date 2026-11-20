//! Snapshots (ADR-013): a restart loads the newest snapshot and replays only
//! what its shards logged after their markers; the log before the snapshot is
//! removed; a damaged snapshot or a missing marker refuses to open.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use wf_application::commands::queue::ClaimedFlow;
use wf_node::composition::{QueueNode, QueueNodeConfig};
use wf_storage::log::GroupLogLimits;

use super::{TestResult, claim, complete, completion, create, directory, open_with};

fn small(shards: u16, snapshot_segments: u64) -> QueueNodeConfig {
    QueueNodeConfig {
        shards,
        log: GroupLogLimits {
            max_segment_bytes: 4_096,
            ..QueueNodeConfig::default().log
        },
        snapshot_segments,
        ..QueueNodeConfig::default()
    }
}

fn ids(prefix: &str, count: usize) -> Vec<String> {
    (0..count).map(|index| format!("{prefix}{index}")).collect()
}

fn refs(ids: &[String]) -> Vec<&str> {
    ids.iter().map(String::as_str).collect()
}

fn files(path: &Path, suffix: &str) -> TestResult<Vec<PathBuf>> {
    let mut found: Vec<PathBuf> = std::fs::read_dir(path)?
        .map(|entry| Ok(entry?.path()))
        .collect::<TestResult<Vec<_>>>()?
        .into_iter()
        .filter(|file| file.to_string_lossy().ends_with(suffix))
        .collect();
    found.sort();
    Ok(found)
}

/// 300 Flows, 100 claimed, 50 of those completed; returns the 50 held.
fn busy_node(node: &QueueNode) -> TestResult<Vec<ClaimedFlow>> {
    let created = ids("s", 300);
    for chunk in refs(&created).chunks(100) {
        assert!(create(node, chunk)?.iter().all(Result::is_ok));
    }
    let claimed = claim(node, 100)?;
    assert_eq!(claimed.len(), 100);
    let (done, held) = claimed.split_at(50);
    assert!(
        complete(node, done.iter().map(completion).collect())?
            .iter()
            .all(Result::is_ok)
    );
    Ok(held.to_vec())
}

/// After `busy_node` plus 10 later creates: every id exists, 210 are due,
/// and the held leases still complete.
fn check_restored(node: &QueueNode, held: &[ClaimedFlow]) -> TestResult {
    let all: Vec<String> = ids("s", 300).into_iter().chain(ids("late", 10)).collect();
    assert!(
        create(node, &refs(&all))?.iter().all(Result::is_err),
        "a Flow was lost"
    );
    assert_eq!(claim(node, 1_000)?.len(), 210);
    let outcomes = complete(node, held.iter().map(completion).collect())?;
    assert!(outcomes.iter().all(Result::is_ok), "{outcomes:?}");
    Ok(())
}

#[test]
fn a_snapshot_replaces_the_log_before_it() -> TestResult {
    let directory = directory("snapshot")?;
    let held = {
        let node = open_with(&directory, small(4, 0))?;
        let held = busy_node(&node)?;
        let before = files(&directory.join("log"), ".wfgl")?;
        assert_eq!(node.snapshot()?, 1);
        let after = files(&directory.join("log"), ".wfgl")?;
        assert!(after.len() < before.len(), "{before:?} -> {after:?}");
        assert!(!after.contains(before.first().ok_or("no segment")?));
        assert_eq!(files(&directory.join("snapshots"), ".snapshot")?.len(), 1);
        assert!(
            create(&node, &refs(&ids("late", 10)))?
                .iter()
                .all(Result::is_ok)
        );
        held
    };
    for shards in [4, 7] {
        let node = open_with(&directory, small(shards, 0))?;
        if shards == 7 {
            check_restored(&node, &held)?;
        } else {
            assert!(create(&node, &["s0"])?.iter().all(Result::is_err));
        }
    }
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

#[test]
fn records_the_snapshot_holds_are_skipped_when_old_segments_survive() -> TestResult {
    let directory = directory("snapshot-crash")?;
    let saved = directory.join("saved");
    std::fs::create_dir(&saved)?;
    let held = {
        let node = open_with(&directory, small(4, 0))?;
        let held = busy_node(&node)?;
        for segment in files(&directory.join("log"), ".wfgl")? {
            std::fs::copy(&segment, saved.join(segment.file_name().ok_or("name")?))?;
        }
        node.snapshot()?;
        assert!(
            create(&node, &refs(&ids("late", 10)))?
                .iter()
                .all(Result::is_ok)
        );
        held
    };
    let mut restored = 0;
    for segment in files(&saved, ".wfgl")? {
        let back = directory
            .join("log")
            .join(segment.file_name().ok_or("name")?);
        if !back.exists() {
            std::fs::copy(&segment, back)?;
            restored += 1;
        }
    }
    assert!(restored > 0, "the snapshot removed no segment");
    let node = open_with(&directory, small(4, 0))?;
    check_restored(&node, &held)?;
    drop(node);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

#[test]
fn a_damaged_snapshot_or_a_missing_marker_refuses_to_open() -> TestResult {
    let directory = directory("snapshot-damaged")?;
    {
        let node = open_with(&directory, small(2, 0))?;
        busy_node(&node)?;
        node.snapshot()?;
    }
    let snapshot = files(&directory.join("snapshots"), ".snapshot")?
        .pop()
        .ok_or("no snapshot")?;
    let good = std::fs::read(&snapshot)?;
    let mut bad = good.clone();
    let middle = bad.len() / 2;
    if let Some(byte) = bad.get_mut(middle) {
        *byte ^= 0x01;
    }
    std::fs::write(&snapshot, &bad)?;
    assert!(QueueNode::open(&directory, small(2, 0)).is_err());
    std::fs::write(&snapshot, &good)?;
    std::fs::remove_dir_all(directory.join("log"))?;
    assert!(QueueNode::open(&directory, small(2, 0)).is_err());
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

#[test]
fn automatic_snapshots_keep_the_log_short() -> TestResult {
    let directory = directory("snapshot-auto")?;
    let node = open_with(&directory, small(4, 2))?;
    let created = ids("auto", 3_000);
    for chunk in refs(&created).chunks(200) {
        assert!(create(&node, chunk)?.iter().all(Result::is_ok));
        let jobs = claim(&node, 200)?;
        assert!(
            complete(&node, jobs.iter().map(completion).collect())?
                .iter()
                .all(Result::is_ok)
        );
    }
    let deadline = Instant::now() + Duration::from_secs(10);
    while files(&directory.join("snapshots"), ".snapshot")?.is_empty() {
        assert!(Instant::now() < deadline, "no automatic snapshot");
        std::thread::sleep(Duration::from_millis(100));
    }
    let oldest = files(&directory.join("log"), ".wfgl")?
        .first()
        .and_then(|segment| {
            segment
                .file_name()?
                .to_str()?
                .get(..20)?
                .parse::<u64>()
                .ok()
        })
        .ok_or("no segment")?;
    assert!(oldest > 0, "no segment was removed");
    drop(node);
    let node = open_with(&directory, small(4, 2))?;
    assert!(create(&node, &refs(&created))?.iter().all(Result::is_err));
    assert_eq!(claim(&node, 10)?, Vec::new());
    drop(node);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}
