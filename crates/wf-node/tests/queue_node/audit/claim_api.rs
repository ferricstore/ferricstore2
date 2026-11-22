//! `flow/claim_due_api_test.exs`, its client-visible cases: a claim's
//! states times partitions stay within 64, compact job replies, exact
//! lease times, block timeouts and payload hydration stay bounded. (Its
//! waiter keys and key-size checks are Elixir's.)

use wf_application::commands::flow::Term;
use wf_node::composition::QueueNode;

use super::{claimed, int, ok, text, texts};
use crate::{TestResult, directory, open};

const MAX_EXACT: i64 = 9_007_199_254_740_991;

fn claim(node: &QueueNode, extra: &[(&str, Term)]) -> Result<Vec<Term>, String> {
    let mut entries = vec![
        ("type", text("type")),
        ("worker", text("worker")),
        ("partition_key", text("p")),
    ];
    entries.retain(|(key, _)| !extra.iter().any(|(given, _)| given == key));
    if extra.iter().any(|(given, _)| *given == "partition_keys") {
        entries.retain(|(key, _)| *key != "partition_key");
    }
    entries.extend(extra.iter().cloned());
    claimed(node, &entries).map_err(|error| error.to_string())
}

fn named(prefix: &str, count: usize) -> Vec<String> {
    (1..=count)
        .map(|index| format!("{prefix}-{index}"))
        .collect()
}

fn list(names: &[String]) -> Term {
    Term::List(names.iter().map(|name| text(name)).collect())
}

#[test]
fn a_claims_states_times_partitions_stay_within_64() -> TestResult {
    // Elixir: "wait_keys rejects a state filter whose expanded footprint is
    // unbounded", "... a partition filter ..." and "wait_keys bounds the
    // combined state and partition footprint" (Elixir checks every claim).
    let node = open(&directory("audit-claim-footprint")?, 4)?;
    let refused = Err("ERR flow claim filter footprint exceeds maximum 64".to_owned());
    let states = named("state", 65);
    let partitions = named("partition", 65);
    assert_eq!(claim(&node, &[("states", list(&states))]), refused);
    let wide = [
        ("states", texts(&["queued"])),
        ("partition_keys", list(&partitions)),
    ];
    assert_eq!(claim(&node, &wide), refused);
    let nine = [
        ("states", list(&named("state", 9))),
        ("partition_keys", list(&named("partition", 8))),
    ];
    assert_eq!(claim(&node, &nine), refused);
    let eight = [
        ("states", list(&named("state", 8))),
        ("partition_keys", list(&named("partition", 8))),
    ];
    assert_eq!(claim(&node, &eight), Ok(Vec::new()));
    Ok(())
}

#[test]
fn a_compact_claim_answers_id_partition_lease_and_fence() -> TestResult {
    // Elixir: "return_records supports compact jobs".
    let node = open(&directory("audit-claim-compact")?, 4)?;
    ok(
        &node,
        "FLOW.CREATE",
        &[
            ("id", text("id")),
            ("type", text("type")),
            ("partition_key", text("p")),
            ("run_at_ms", int(1_000)),
        ],
    )?;
    let jobs = claim(
        &node,
        &[("return", text("jobs_compact")), ("now_ms", int(1_000))],
    )?;
    let [Term::List(job)] = jobs.as_slice() else {
        return Err(format!("one compact job expected, got {jobs:?}").into());
    };
    let [id, partition, Term::Bin(_), Term::Int(1)] = job.as_slice() else {
        return Err(format!("[id, partition, lease, fence] expected, got {job:?}").into());
    };
    assert_eq!((id, partition), (&text("id"), &text("p")));
    Ok(())
}

#[test]
fn claim_times_and_sizes_stay_within_their_ceilings() -> TestResult {
    // Elixir: "claim_due rejects inexact lease timestamps before routing",
    // "... block timeouts larger than the VM timer limit ..." and "...
    // payload hydration above the configured ceiling ...".
    let node = open(&directory("audit-claim-ceilings")?, 4)?;
    let over = MAX_EXACT.saturating_add(1);
    for (extra, refused) in [
        (
            vec![("lease_ms", int(over))],
            format!("ERR flow lease_ms exceeds maximum {MAX_EXACT}"),
        ),
        (
            vec![("now_ms", int(over))],
            format!("ERR flow now_ms exceeds maximum {MAX_EXACT}"),
        ),
        (
            vec![("now_ms", int(MAX_EXACT)), ("lease_ms", int(1))],
            format!("ERR flow lease_ms deadline exceeds maximum {MAX_EXACT}"),
        ),
        (
            vec![("block_ms", int(4_294_967_296))],
            "ERR flow block_ms exceeds maximum 4294967295".to_owned(),
        ),
        (
            vec![("payload_max_bytes", int(65_537))],
            "ERR flow payload_max_bytes exceeds maximum 65536".to_owned(),
        ),
    ] {
        assert_eq!(claim(&node, &extra), Err(refused.clone()), "{refused}");
    }
    Ok(())
}
