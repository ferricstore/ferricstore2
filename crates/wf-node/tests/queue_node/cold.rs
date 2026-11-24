//! Finished Flows on disk (ADR-016): a Flow that finishes moves to a cold
//! file at the end of its round and is read from there; snapshots keep its
//! stub, a crash before one rewrites it from the log, a rewind brings it
//! back, and retention removes it and, once its file is empty and sealed,
//! the file.

use std::path::Path;

use wf_application::commands::flow::{self, FlowService, Term};
use wf_application::commands::queue::ServiceError;
use wf_node::composition::{QueueNode, QueueNodeConfig};
use wf_storage::log::GroupLogLimits;

use super::{TestResult, call, claim, complete, completion, create, directory, open, open_with};

fn run(
    node: &QueueNode,
    name: &str,
    entries: &[(&str, Term)],
) -> TestResult<Result<Term, ServiceError>> {
    let options = Term::Map(
        entries
            .iter()
            .map(|(key, value)| (key.as_bytes().to_vec(), value.clone()))
            .collect(),
    );
    let command = flow::parse(name, &options)?;
    call(|reply| node.execute(command, reply))
}

fn get(node: &QueueNode, id: &str) -> TestResult<Term> {
    Ok(run(
        node,
        "FLOW.GET",
        &[("id", Term::text(id)), ("full", Term::Bool(true))],
    )??)
}

fn field(term: &Term, key: &str) -> Term {
    term.get(key.as_bytes()).cloned().unwrap_or(Term::Nil)
}

fn cold_files(path: &Path) -> TestResult<Vec<String>> {
    let mut files: Vec<String> = std::fs::read_dir(path.join("cold"))?
        .map(|entry| Ok(entry?.file_name().to_string_lossy().into_owned()))
        .collect::<TestResult<Vec<_>>>()?;
    files.sort();
    Ok(files)
}

/// Creates `count` Flows, claims and completes them with a result.
fn finish(node: &QueueNode, prefix: &str, count: usize) -> TestResult<Vec<String>> {
    let ids: Vec<String> = (0..count).map(|index| format!("{prefix}{index}")).collect();
    let refs: Vec<&str> = ids.iter().map(String::as_str).collect();
    assert!(create(node, &refs)?.iter().all(Result::is_ok));
    let claimed = claim(node, count)?;
    assert_eq!(claimed.len(), count);
    let completions = claimed
        .iter()
        .map(|job| {
            let mut done = completion(job);
            done.result = Some(format!("result-{}", String::from_utf8_lossy(&job.id)).into_bytes());
            done
        })
        .collect();
    assert!(complete(node, completions)?.iter().all(Result::is_ok));
    Ok(ids)
}

fn assert_finished(node: &QueueNode, ids: &[String]) -> TestResult {
    for id in ids {
        let record = get(node, id)?;
        assert_eq!(field(&record, "state"), Term::text("completed"), "{id}");
        assert_eq!(
            field(&record, "result"),
            Term::text(&format!("result-{id}")),
            "{id}"
        );
        let Term::List(events) = run(node, "FLOW.HISTORY", &[("id", Term::text(id))])?? else {
            return Err("history is not a list".into());
        };
        assert_eq!(events.len(), 3, "created, claimed, completed: {id}");
    }
    Ok(())
}

#[test]
fn finished_flows_are_read_from_cold_files_across_a_snapshot_and_restart() -> TestResult {
    let path = directory("cold-snapshot")?;
    let config = QueueNodeConfig {
        shards: 4,
        log: GroupLogLimits {
            max_segment_bytes: 4_096,
            ..QueueNodeConfig::default().log
        },
        ..QueueNodeConfig::default()
    };
    let ids = {
        let node = open_with(&path, config)?;
        let ids = finish(&node, "c", 40)?;
        assert_finished(&node, &ids)?;
        assert!(
            !cold_files(&path)?.is_empty(),
            "finished Flows went to disk"
        );
        node.snapshot()?;
        ids
    };
    let node = open_with(
        &path,
        QueueNodeConfig {
            shards: 3,
            ..config
        },
    )?;
    assert_finished(&node, &ids)?;
    Ok(())
}

#[test]
fn a_crash_before_a_snapshot_rewrites_finished_flows_from_the_log() -> TestResult {
    let path = directory("cold-crash")?;
    let ids = {
        let node = open(&path, 4)?;
        let ids = finish(&node, "k", 30)?;
        assert_finished(&node, &ids)?;
        ids
    };
    let before = cold_files(&path)?;
    let node = open(&path, 4)?;
    assert_finished(&node, &ids)?;
    let after = cold_files(&path)?;
    assert!(
        after.iter().all(|file| !before.contains(file)),
        "files no stub names are removed: {before:?} then {after:?}"
    );
    Ok(())
}

#[test]
fn a_rewind_brings_a_finished_flow_back_from_disk() -> TestResult {
    let path = directory("cold-rewind")?;
    let node = open(&path, 4)?;
    let ids = finish(&node, "r", 1)?;
    let id = ids.first().ok_or("no flow")?;
    let Term::List(events) = run(&node, "FLOW.HISTORY", &[("id", Term::text(id))])?? else {
        return Err("history is not a list".into());
    };
    let Some(Term::List(first)) = events.first() else {
        return Err("no created event".into());
    };
    let created = first.first().cloned().unwrap_or(Term::Nil);
    run(
        &node,
        "FLOW.REWIND",
        &[("id", Term::text(id)), ("to_event", created)],
    )??;
    assert_eq!(field(&get(&node, id)?, "state"), Term::text("queued"));
    let again = claim(&node, 1)?;
    assert_eq!(again.len(), 1, "the rewound Flow is live again");
    Ok(())
}

#[test]
fn retention_removes_finished_flows_and_then_their_cold_files() -> TestResult {
    let path = directory("cold-retention")?;
    let config = QueueNodeConfig {
        shards: 2,
        retention_ms: 1,
        ..QueueNodeConfig::default()
    };
    {
        let node = open_with(&path, config)?;
        let ids = finish(&node, "x", 10)?;
        std::thread::sleep(std::time::Duration::from_millis(600));
        for id in &ids {
            assert_eq!(get(&node, id)?, Term::Nil, "{id} removed");
        }
        node.snapshot()?;
    }
    // The writers were open; after a restart nothing names their files.
    let node = open_with(&path, config)?;
    drop(node);
    assert!(cold_files(&path)?.is_empty(), "{:?}", cold_files(&path)?);
    Ok(())
}

fn history(node: &QueueNode, id: &str) -> TestResult<Term> {
    Ok(run(node, "FLOW.HISTORY", &[("id", Term::text(id))])??)
}

fn rewind_to_created(node: &QueueNode, id: &str) -> TestResult {
    let Term::List(events) = history(node, id)? else {
        return Err("history is not a list".into());
    };
    let Some(Term::List(first)) = events.first() else {
        return Err("no created event".into());
    };
    let created = first.first().cloned().unwrap_or(Term::Nil);
    run(
        node,
        "FLOW.REWIND",
        &[("id", Term::text(id)), ("to_event", created)],
    )??;
    Ok(())
}

#[test]
fn a_cold_file_the_last_snapshot_names_outlives_its_last_stub() -> TestResult {
    // A rewind takes the only stub of a sealed cold file; the last
    // snapshot still names that file, so after a crash replay applies the
    // rewind to the image there.
    let path = directory("cold-snapshot-names")?;
    let id = {
        let node = open(&path, 4)?;
        finish(&node, "g", 1)?.into_iter().next().ok_or("no flow")?
    };
    {
        // Replay moves the Flow to a file the snapshot names.
        let node = open(&path, 4)?;
        node.snapshot()?;
    }
    let before = {
        // A restart seals every file.
        let node = open(&path, 4)?;
        rewind_to_created(&node, &id)?;
        history(&node, &id)?
    };
    let node = open(&path, 4)?;
    assert_eq!(history(&node, &id)?, before);
    assert_eq!(field(&get(&node, &id)?, "state"), Term::text("queued"));
    assert!(
        !cold_files(&path)?.is_empty(),
        "the snapshot still names it"
    );
    node.snapshot()?;
    assert!(
        cold_files(&path)?.is_empty(),
        "a newer snapshot no longer names it: {:?}",
        cold_files(&path)?
    );
    Ok(())
}
