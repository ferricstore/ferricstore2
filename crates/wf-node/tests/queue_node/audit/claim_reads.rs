//! `flow_test/sections/flow_spawn_children_wait_any_resolves_on_first_
//! successful_child_across_s.exs`, its claims and lifecycle reads: a claim
//! leases due Flows once, claims across every partition find new work and
//! nothing early, and the reads Elixir's retired list commands gave, as the
//! SDK's `FLOW.QUERY` text and `FLOW.INFO`. (Its write-skipping checks are
//! Elixir's own.)

use wf_application::commands::flow::Term;
use wf_node::composition::QueueNode;

use super::{claimed, field, int, map, ok, text};
use crate::{TestResult, directory, open};

fn create(node: &QueueNode, id: &str, flow_type: &str, extra: &[(&str, Term)]) -> TestResult {
    let mut entries = vec![
        ("id", text(id)),
        ("type", text(flow_type)),
        ("state", text("queued")),
    ];
    entries.retain(|(key, _)| !extra.iter().any(|(given, _)| given == key));
    entries.extend(extra.iter().cloned());
    ok(node, "FLOW.CREATE", &entries)?;
    Ok(())
}

fn claim_at(node: &QueueNode, entries: &[(&str, Term)]) -> TestResult<Vec<Term>> {
    let mut all = vec![("lease_ms", int(30_000)), ("limit", int(10))];
    all.retain(|(key, _)| !entries.iter().any(|(given, _)| given == key));
    all.extend(entries.iter().cloned());
    claimed(node, &all)
}

fn ids(records: &[Term]) -> Vec<Term> {
    let mut ids: Vec<Term> = records.iter().map(|record| field(record, "id")).collect();
    ids.sort_by_key(|id| format!("{id:?}"));
    ids
}

#[test]
fn a_claim_leases_due_flows_once() -> TestResult {
    // Elixir: "flow_claim_due atomically leases due flows and removes them
    // from due set" and "flow_claim_due leases large batches without
    // duplicates and drains due members".
    let node = open(&directory("audit-claimreads-lease")?, 4)?;
    create(
        &node,
        "one",
        "email",
        &[("payload", text("payload:one")), ("run_at_ms", int(1_000))],
    )?;
    let email = |worker: &str| {
        [
            ("type", text("email")),
            ("state", text("queued")),
            ("worker", text(worker)),
            ("now_ms", int(1_000)),
        ]
    };
    let records = claim_at(&node, &email("worker-a"))?;
    let [record] = records.as_slice() else {
        return Err("one claim".into());
    };
    assert_eq!(field(record, "id"), text("one"));
    assert_eq!(field(record, "state"), text("running"));
    assert_eq!(field(record, "lease_owner"), text("worker-a"));
    assert!(matches!(field(record, "lease_token"), Term::Bin(_)));
    assert_eq!(field(record, "fencing_token"), int(1));
    assert_eq!(field(record, "version"), int(2));
    assert_eq!(claim_at(&node, &email("worker-b"))?, Vec::<Term>::new());

    let all: Vec<String> = (1..=100).map(|index| format!("large:{index}")).collect();
    for id in &all {
        let options = [
            ("payload", text(&format!("payload:{id}"))),
            ("run_at_ms", int(1_000)),
            ("now_ms", int(1_000)),
        ];
        create(&node, id, "large", &options)?;
    }
    let large = |worker: &str| {
        [
            ("type", text("large")),
            ("state", text("queued")),
            ("worker", text(worker)),
            ("limit", int(100)),
            ("now_ms", int(1_000)),
        ]
    };
    let records = claim_at(&node, &large("worker-large"))?;
    let mut expected: Vec<Term> = all.iter().map(|id| text(id)).collect();
    expected.sort_by_key(|id| format!("{id:?}"));
    assert_eq!(ids(&records), expected);
    assert!(
        records
            .iter()
            .all(|record| field(record, "state") == text("running"))
    );
    assert!(
        records
            .iter()
            .all(|record| field(record, "lease_owner") == text("worker-large"))
    );
    assert_eq!(
        claim_at(&node, &large("worker-large-2"))?,
        Vec::<Term>::new()
    );
    Ok(())
}

#[test]
fn claims_across_partitions_find_new_work_and_nothing_early() -> TestResult {
    // Elixir: "all-partition claims avoid Raft writes for an empty type
    // without hiding newly due work", "all-partition empty precheck respects
    // future deadlines and encoded type boundaries", "... falls back when its
    // bounded catalog page is full" and "... does not hide hibernated work"
    // (their write counts are Elixir's).
    let node = open(&directory("audit-claimreads-any")?, 4)?;
    let unrelated = [
        ("partition_key", text("other")),
        ("run_at_ms", int(1_000)),
        ("now_ms", int(1_000)),
    ];
    create(&node, "unrelated", "empty-any-other", &unrelated)?;
    let any = |flow_type: &str, state: &str, now: i64| {
        [
            ("type", text(flow_type)),
            ("partition_key", text("any")),
            ("state", text(state)),
            ("worker", text("idle-check")),
            ("limit", int(8)),
            ("now_ms", int(now)),
            ("reclaim_expired", Term::Bool(false)),
        ]
    };
    for _ in 0..3 {
        assert_eq!(
            claim_at(&node, &any("empty-any", "queued", 1_000))?,
            Vec::<Term>::new()
        );
    }
    let fresh = [
        ("partition_key", text("new-partition")),
        ("run_at_ms", int(1_000)),
        ("now_ms", int(1_000)),
    ];
    create(&node, "newly-due", "empty-any", &fresh)?;
    assert_eq!(
        ids(&claim_at(&node, &any("empty-any", "queued", 1_000))?),
        [text("newly-due")]
    );

    // Names with the characters keys use as separators.
    let odd = "any:deadline:p0}";
    let future = [
        ("state", text("wait:ing")),
        ("partition_key", text("p:one")),
        ("run_at_ms", int(1_100)),
        ("now_ms", int(1_000)),
    ];
    create(&node, "future-any", odd, &future)?;
    assert_eq!(
        claim_at(&node, &any(odd, "wait:ing", 1_000))?,
        Vec::<Term>::new()
    );
    assert_eq!(
        ids(&claim_at(&node, &any(odd, "wait:ing", 1_100))?),
        [text("future-any")]
    );
    Ok(())
}

/// The SDK's query for `flow_type` in `state`, in partition `ops`.
fn listed(node: &QueueNode, flow_type: &str, state: &str) -> TestResult<Vec<Term>> {
    let fql = "FROM runs WHERE partition_key = @partition_key AND type = @type AND state = @state ORDER BY updated_at_ms ASC LIMIT 10 RETURN RECORDS";
    let params = map(&[
        ("partition_key", text("ops")),
        ("type", text(flow_type)),
        ("state", text(state)),
    ]);
    let result = ok(
        node,
        "FLOW.QUERY",
        &[
            ("version", text("FQL1")),
            ("query", text(fql)),
            ("params", params),
        ],
    )?;
    let Term::List(records) = field(&result, "records") else {
        return Err(format!("no records in {result:?}").into());
    };
    Ok(records.iter().map(|record| field(record, "id")).collect())
}

#[test]
fn lifecycle_reads_list_count_and_find_stuck_flows() -> TestResult {
    // Elixir: "flow_list, flow_info, and flow_stuck read lifecycle indexes"
    // and "flow_info counts pending terminal records before LMDB writer
    // flush".
    let node = open(&directory("audit-claimreads-reads")?, 4)?;
    let at = |due: i64| {
        [
            ("partition_key", text("ops")),
            ("run_at_ms", int(due)),
            ("now_ms", int(900)),
        ]
    };
    create(&node, "due", "ops", &at(2_000))?;
    create(&node, "running", "ops", &at(1_000))?;
    create(&node, "done", "ops", &at(1_000))?;
    let records = claim_at(
        &node,
        &[
            ("type", text("ops")),
            ("partition_key", text("ops")),
            ("worker", text("worker-ops")),
            ("lease_ms", int(50)),
            ("limit", int(2)),
            ("now_ms", int(1_000)),
        ],
    )?;
    let [first, second] = records.as_slice() else {
        return Err("two claims".into());
    };
    let (running, done) = if field(first, "id") == text("done") {
        (second, first)
    } else {
        (first, second)
    };
    ok(
        &node,
        "FLOW.COMPLETE",
        &[
            ("id", field(done, "id")),
            ("partition_key", text("ops")),
            ("lease_token", field(done, "lease_token")),
            ("fencing_token", field(done, "fencing_token")),
            ("now_ms", int(1_001)),
        ],
    )?;
    assert_eq!(listed(&node, "ops", "queued")?, [text("due")]);
    assert_eq!(listed(&node, "ops", "running")?, [field(running, "id")]);
    assert_eq!(listed(&node, "ops", "completed")?, [field(done, "id")]);
    let info = ok(
        &node,
        "FLOW.INFO",
        &[("type", text("ops")), ("partition_key", text("ops"))],
    )?;
    assert_eq!(field(&info, "queued"), int(1));
    assert_eq!(field(&info, "running"), int(1));
    assert_eq!(field(&info, "completed"), int(1));
    assert_eq!(field(&info, "inflight"), int(1));
    let stuck = "FROM runs WHERE partition_key = @partition_key AND type = @type AND state = @state AND lease_deadline_ms BETWEEN @lease_from_ms AND @lease_to_ms ORDER BY lease_deadline_ms ASC LIMIT 10 RETURN RECORDS";
    let params = map(&[
        ("partition_key", text("ops")),
        ("type", text("ops")),
        ("state", text("running")),
        ("lease_from_ms", int(0)),
        ("lease_to_ms", int(1_051)),
    ]);
    let result = ok(
        &node,
        "FLOW.QUERY",
        &[
            ("version", text("FQL1")),
            ("query", text(stuck)),
            ("params", params),
        ],
    )?;
    let Term::List(stuck) = field(&result, "records") else {
        return Err(format!("no records in {result:?}").into());
    };
    let stuck_ids: Vec<Term> = stuck.iter().map(|record| field(record, "id")).collect();
    assert_eq!(stuck_ids, [field(running, "id")]);
    Ok(())
}
