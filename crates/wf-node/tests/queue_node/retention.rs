//! Completed Flows are kept for the retention period, then removed; the
//! removal is logged, so a restart agrees and the id can be created again
//! (ADR-013).

use std::path::Path;
use std::time::{Duration, Instant};

use wf_node::composition::{QueueNode, QueueNodeConfig};

use super::{TestResult, claim, complete, completion, create, directory, open, open_with};

fn short_retention(path: &Path) -> TestResult<QueueNode> {
    open_with(
        path,
        QueueNodeConfig {
            shards: 4,
            retention_ms: 200,
            ..QueueNodeConfig::default()
        },
    )
}

fn finish(node: &QueueNode, ids: &[&str]) -> TestResult {
    assert!(create(node, ids)?.iter().all(Result::is_ok));
    let jobs = claim(node, ids.len())?;
    assert_eq!(jobs.len(), ids.len());
    assert!(
        complete(node, jobs.iter().map(completion).collect())?
            .iter()
            .all(Result::is_ok)
    );
    Ok(())
}

/// Creates `id` until it succeeds, which needs its old Flow removed.
fn recreate_within(node: &QueueNode, id: &str, patience: Duration) -> TestResult<Duration> {
    let started = Instant::now();
    loop {
        if create(node, &[id])?.iter().all(Result::is_ok) {
            return Ok(started.elapsed());
        }
        if started.elapsed() > patience {
            return Err(format!("{id} was never removed").into());
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[test]
fn a_completed_flow_is_removed_after_retention() -> TestResult {
    let directory = directory("retention")?;
    let node = short_retention(&directory)?;
    finish(&node, &["a"])?;
    assert!(
        create(&node, &["a"])?.iter().all(Result::is_err),
        "a completed Flow was removed before its retention"
    );
    let waited = recreate_within(&node, "a", Duration::from_secs(5))?;
    assert!(waited >= Duration::from_millis(100), "{waited:?}");
    drop(node);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

#[test]
fn a_restart_replays_removals() -> TestResult {
    let directory = directory("retention-restart")?;
    {
        let node = short_retention(&directory)?;
        finish(&node, &["again", "gone"])?;
        recreate_within(&node, "again", Duration::from_secs(5))?;
        recreate_within(&node, "gone", Duration::from_secs(5))?;
        let jobs = claim(&node, 5)?;
        assert_eq!(jobs.len(), 2);
        assert!(
            complete(&node, vec![completion(jobs.first().ok_or("no job")?)])?
                .iter()
                .all(Result::is_ok)
        );
    }
    let node = open(&directory, 3)?;
    let outcomes = create(&node, &["again", "gone"])?;
    assert!(
        outcomes.iter().all(Result::is_err),
        "the second Flows were lost: {outcomes:?}"
    );
    drop(node);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

#[test]
fn unfinished_and_retained_flows_stay() -> TestResult {
    let directory = directory("retention-stay")?;
    {
        let node = short_retention(&directory)?;
        assert!(create(&node, &["open"])?.iter().all(Result::is_ok));
        std::thread::sleep(Duration::from_millis(700));
        assert_eq!(claim(&node, 5)?.len(), 1, "an open Flow was removed");
    }
    let node = open(&directory, 4)?;
    finish(&node, &["kept"])?;
    drop(node);
    let node = open(&directory, 4)?;
    std::thread::sleep(Duration::from_millis(500));
    assert!(
        create(&node, &["kept", "open"])?.iter().all(Result::is_err),
        "a Flow within the default retention was removed"
    );
    drop(node);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}
