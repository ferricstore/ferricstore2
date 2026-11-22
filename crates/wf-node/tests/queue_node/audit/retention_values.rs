//! `flow_test/sections/flushdb_clears_flow_lmdb_terminal_projections.exs`,
//! its retention cases: a replaced payload stays readable after its events
//! leave the history and goes with the Flow at retention cleanup; a
//! finished Flow's history ends with its retention; a completion's own
//! retention must be positive; a create takes its type's retention
//! defaults. (`FLUSHDB` is a key-value command the workflow-only node does
//! not serve.)

use std::time::Duration;

use wf_application::commands::flow::Term;
use wf_node::composition::QueueNode;

use super::{claimed, field, get, int, map, ok, refusal, run, text};
use crate::{TestResult, directory, open};

fn claim(node: &QueueNode, flow_type: &str, state: &str, now: i64) -> TestResult<Term> {
    let records = claimed(
        node,
        &[
            ("type", text(flow_type)),
            ("state", text(state)),
            ("worker", text("worker")),
            ("lease_ms", int(30_000)),
            ("limit", int(1)),
            ("now_ms", int(now)),
        ],
    )?;
    match records.as_slice() {
        [record] => Ok(record.clone()),
        _ => Err(format!("one claim expected, got {records:?}").into()),
    }
}

fn wall_ms() -> TestResult<i64> {
    let since = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?;
    Ok(i64::try_from(since.as_millis())?)
}

#[test]
fn a_replaced_payload_reads_until_retention_removes_the_flow() -> TestResult {
    // Elixir: "retention cleanup deletes owned payload refs from trimmed
    // history" (audit gap G37).
    let node = open(&directory("audit-retention-owned-values")?, 4)?;
    let now = wall_ms()?.saturating_add(60_000);
    ok(
        &node,
        "FLOW.CREATE",
        &[
            ("id", text("owned")),
            ("type", text("history-trim-owned-value")),
            ("state", text("queued")),
            ("payload", text("first-payload")),
            ("run_at_ms", int(now)),
            ("retention_ttl_ms", int(10)),
            ("history_hot_max_events", int(0)),
            ("history_max_events", int(1)),
            ("now_ms", int(now)),
        ],
    )?;
    let first_ref = field(&get(&node, "owned", &[])?, "payload_ref");
    let lease = claim(&node, "history-trim-owned-value", "queued", now)?;
    ok(
        &node,
        "FLOW.TRANSITION",
        &[
            ("id", text("owned")),
            ("from_state", text("running")),
            ("to_state", text("ready")),
            ("lease_token", field(&lease, "lease_token")),
            ("fencing_token", field(&lease, "fencing_token")),
            ("payload", text("second-payload")),
            ("run_at_ms", int(now + 100)),
            ("now_ms", int(now + 100)),
        ],
    )?;
    let second_ref = field(&get(&node, "owned", &[])?, "payload_ref");
    assert_ne!(first_ref, second_ref);
    let refs = Term::List(vec![first_ref, second_ref]);
    let read = |refs: &Term| ok(&node, "FLOW.VALUE.MGET", &[("refs", refs.clone())]);
    assert_eq!(
        read(&refs)?,
        Term::List(vec![text("first-payload"), text("second-payload")])
    );
    let ready = claim(&node, "history-trim-owned-value", "ready", now + 100)?;
    ok(
        &node,
        "FLOW.COMPLETE",
        &[
            ("id", text("owned")),
            ("lease_token", field(&ready, "lease_token")),
            ("fencing_token", field(&ready, "fencing_token")),
            ("now_ms", int(now + 1_000)),
        ],
    )?;
    let Term::Int(until) = field(&get(&node, "owned", &[])?, "terminal_retention_until_ms") else {
        return Err("no retention deadline".into());
    };
    let cleaned = ok(
        &node,
        "FLOW.RETENTION_CLEANUP",
        &[("limit", int(10)), ("now_ms", int(until.saturating_add(1)))],
    )?;
    assert_eq!(field(&cleaned, "flows"), int(1));
    assert_eq!(read(&refs)?, Term::List(vec![Term::Nil, Term::Nil]));
    Ok(())
}

#[test]
fn a_finished_flows_history_ends_with_its_retention() -> TestResult {
    // Elixir: "terminal retention expires queryable flow history".
    let node = open(&directory("audit-retention-history")?, 4)?;
    ok(
        &node,
        "FLOW.CREATE",
        &[
            ("id", text("ttl")),
            ("type", text("history-ttl")),
            ("payload", map(&[("input", int(1))])),
            ("run_at_ms", int(1_000)),
            ("retention_ttl_ms", int(100)),
            ("now_ms", int(1_000)),
        ],
    )?;
    let history = || {
        ok(
            &node,
            "FLOW.HISTORY",
            &[("id", text("ttl")), ("count", int(10))],
        )
    };
    let Term::List(events) = history()? else {
        return Err("events expected".into());
    };
    assert_eq!(events.len(), 1);
    let lease = claim(&node, "history-ttl", "queued", 1_000)?;
    ok(
        &node,
        "FLOW.COMPLETE",
        &[
            ("id", text("ttl")),
            ("lease_token", field(&lease, "lease_token")),
            ("fencing_token", field(&lease, "fencing_token")),
        ],
    )?;
    std::thread::sleep(Duration::from_millis(150));
    assert_eq!(get(&node, "ttl", &[])?, Term::Nil);
    assert_eq!(history()?, Term::List(Vec::new()));
    Ok(())
}

#[test]
fn a_completions_own_retention_must_be_positive() -> TestResult {
    // Elixir: "terminal ttl override must be positive".
    let node = open(&directory("audit-retention-ttl-zero")?, 4)?;
    ok(
        &node,
        "FLOW.CREATE",
        &[
            ("id", text("zero")),
            ("type", text("ttl-zero")),
            ("run_at_ms", int(1_000)),
            ("now_ms", int(1_000)),
        ],
    )?;
    let lease = claim(&node, "ttl-zero", "queued", 1_000)?;
    let refused = run(
        &node,
        "FLOW.COMPLETE",
        &[
            ("id", text("zero")),
            ("lease_token", field(&lease, "lease_token")),
            ("fencing_token", field(&lease, "fencing_token")),
            ("ttl_ms", int(0)),
        ],
    )?;
    assert_eq!(
        refusal(refused)?,
        "ERR flow ttl_ms must be a positive integer"
    );
    Ok(())
}

#[test]
fn a_create_takes_its_types_retention_defaults() -> TestResult {
    // Elixir: "flow create inherits retention defaults from policy".
    let node = open(&directory("audit-retention-policy")?, 4)?;
    // Type-wide retention is set through the text form, as the SDK sends
    // it (the typed form has no top-level retention).
    super::policy_lifecycle::text_set(
        &node,
        &[
            text("retention-policy"),
            text("RETENTION_TTL"),
            int(5_000),
            text("HISTORY_MAX_EVENTS"),
            int(9),
        ],
    )?;
    ok(
        &node,
        "FLOW.CREATE",
        &[
            ("id", text("inherits")),
            ("type", text("retention-policy")),
            ("state", text("queued")),
            ("now_ms", int(10)),
        ],
    )?;
    let created = get(&node, "inherits", &[])?;
    assert_eq!(field(&created, "retention_ttl_ms"), int(5_000));
    assert_eq!(field(&created, "history_hot_max_events"), int(0));
    assert_eq!(field(&created, "history_max_events"), int(9));
    assert_eq!(field(&created, "terminal_retention_until_ms"), Term::Nil);
    Ok(())
}
