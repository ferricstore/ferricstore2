//! `flow_test/sections/flow_claim_due_automatically_reclaims_expired_
//! leases_ratio.exs`: expired leases reclaimed by ratio, off, after ready
//! work and per partition; transitions and batches that move, refuse or
//! roll back together. A batch is all-or-none per partition here, where
//! Elixir's unit is the shard a client cannot see.

use wf_application::commands::flow::Term;
use wf_node::composition::QueueNode;

use super::{claimed, events, field, get, int, map, ok, refusal, run, text};
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

fn claim_at(node: &QueueNode, flow_type: &str, extra: &[(&str, Term)]) -> TestResult<Vec<Term>> {
    let mut entries = vec![
        ("type", text(flow_type)),
        ("worker", text("worker-a")),
        ("lease_ms", int(50)),
        ("limit", int(1)),
    ];
    entries.retain(|(key, _)| !extra.iter().any(|(given, _)| given == key));
    entries.extend(extra.iter().cloned());
    claimed(node, &entries)
}

fn versions_of(records: &[Term], version: i64) -> Vec<Term> {
    let mut ids: Vec<Term> = records
        .iter()
        .filter(|record| field(record, "version") == int(version))
        .map(|record| field(record, "id"))
        .collect();
    ids.sort_by_key(|id| format!("{id:?}"));
    ids
}

#[test]
fn expired_leases_are_reclaimed_by_ratio_off_or_after_ready_work() -> TestResult {
    // Elixir: "flow_claim_due automatically reclaims expired leases by
    // ratio", "flow_claim_due can disable automatic expired lease reclaim"
    // and "flow_claim_due limit one prefers ready queued work before expired
    // lease reclaim".
    let node = open(&directory("audit-reclaim-ratio")?, 4)?;
    for index in 1..=4 {
        create(
            &node,
            &format!("expired-{index}"),
            "ratio",
            &[("run_at_ms", int(1_000))],
        )?;
    }
    assert_eq!(
        claim_at(&node, "ratio", &[("limit", int(4)), ("now_ms", int(1_000))])?.len(),
        4
    );
    for index in 1..=4 {
        create(
            &node,
            &format!("fresh-{index}"),
            "ratio",
            &[("run_at_ms", int(1_050))],
        )?;
    }
    let records = claim_at(
        &node,
        "ratio",
        &[
            ("worker", text("worker-b")),
            ("limit", int(4)),
            ("now_ms", int(1_050)),
            ("reclaim_ratio", int(50)),
        ],
    )?;
    let reclaimed = versions_of(&records, 3);
    let fresh = versions_of(&records, 2);
    assert_eq!((reclaimed.len(), fresh.len()), (2, 2));
    assert!(
        reclaimed
            .iter()
            .all(|id| matches!(id, Term::Bin(bytes) if bytes.starts_with(b"expired")))
    );
    assert!(
        fresh
            .iter()
            .all(|id| matches!(id, Term::Bin(bytes) if bytes.starts_with(b"fresh")))
    );

    for (flow_type, reclaim) in [("no-reclaim", Some(false)), ("ready-first", None)] {
        create(
            &node,
            &format!("{flow_type}-expired"),
            flow_type,
            &[("run_at_ms", int(1_000))],
        )?;
        claim_at(&node, flow_type, &[("now_ms", int(1_000))])?;
        create(
            &node,
            &format!("{flow_type}-fresh"),
            flow_type,
            &[("run_at_ms", int(1_050))],
        )?;
        let mut options = vec![("worker", text("worker-b")), ("now_ms", int(1_050))];
        options.extend(reclaim.map(|on| ("reclaim_expired", Term::Bool(on))));
        let records = claim_at(&node, flow_type, &options)?;
        let [record] = records.as_slice() else {
            return Err("one claim".into());
        };
        assert_eq!(field(record, "id"), text(&format!("{flow_type}-fresh")));
        assert_eq!(field(record, "version"), int(2));
    }
    let records = claim_at(
        &node,
        "ready-first",
        &[("worker", text("worker-c")), ("now_ms", int(1_050))],
    )?;
    let [record] = records.as_slice() else {
        return Err("one reclaim".into());
    };
    assert_eq!(field(record, "id"), text("ready-first-expired"));
    assert_eq!(field(record, "version"), int(3));
    Ok(())
}

#[test]
fn a_running_claim_reclaims_only_its_partition() -> TestResult {
    // Elixir: "expired running lease reclaim is partition scoped" (its stuck
    // list as the SDK's FLOW.QUERY text).
    let node = open(&directory("audit-reclaim-partition")?, 4)?;
    for partition in ["a", "b"] {
        let options = [
            ("partition_key", text(partition)),
            ("run_at_ms", int(1_000)),
        ];
        create(
            &node,
            &format!("flow-{partition}"),
            "lease-partition",
            &options,
        )?;
    }
    let mut first = Vec::new();
    for partition in ["a", "b"] {
        let options = [
            ("state", text("queued")),
            ("partition_key", text(partition)),
            ("now_ms", int(1_000)),
        ];
        first.extend(claim_at(&node, "lease-partition", &options)?);
    }
    let [first_a, first_b] = first.as_slice() else {
        return Err("two claims".into());
    };
    let options = [
        ("state", text("running")),
        ("partition_key", text("a")),
        ("worker", text("worker-b")),
        ("limit", int(10)),
        ("now_ms", int(1_050)),
    ];
    let records = claim_at(&node, "lease-partition", &options)?;
    let [second_a] = records.as_slice() else {
        return Err("one reclaim in a".into());
    };
    assert_eq!(field(second_a, "id"), text("flow-a"));
    assert_eq!(field(second_a, "lease_owner"), text("worker-b"));
    assert_ne!(
        field(second_a, "lease_token"),
        field(first_a, "lease_token")
    );
    let b = get(&node, "flow-b", &[("partition_key", text("b"))])?;
    assert_eq!(field(&b, "lease_owner"), text("worker-a"));
    assert_eq!(field(&b, "lease_token"), field(first_b, "lease_token"));
    let stuck = |partition: &str| -> TestResult<Vec<Term>> {
        let fql = "FROM runs WHERE partition_key = @partition_key AND type = @type AND state = @state AND lease_deadline_ms BETWEEN @lease_from_ms AND @lease_to_ms ORDER BY lease_deadline_ms ASC LIMIT 10 RETURN RECORDS";
        let params = map(&[
            ("partition_key", text(partition)),
            ("type", text("lease-partition")),
            ("state", text("running")),
            ("lease_from_ms", int(0)),
            ("lease_to_ms", int(1_050)),
        ]);
        let result = ok(
            &node,
            "FLOW.QUERY",
            &[
                ("version", text("FQL1")),
                ("query", text(fql)),
                ("params", params),
            ],
        )?;
        let Term::List(records) = field(&result, "records") else {
            return Err("no records".into());
        };
        Ok(records.iter().map(|record| field(record, "id")).collect())
    };
    assert_eq!(stuck("a")?, Vec::<Term>::new());
    assert_eq!(stuck("b")?, [text("flow-b")]);
    Ok(())
}

#[test]
fn a_transition_moves_state_due_time_and_history_but_never_into_running() -> TestResult {
    // Elixir: "flow_transition atomically moves state, due index, and
    // history" and "flow_transition rejects direct moves into running
    // state".
    let node = open(&directory("audit-reclaim-transition")?, 4)?;
    create(
        &node,
        "t",
        "checkout",
        &[
            ("state", text("payment_pending")),
            ("run_at_ms", int(1_000)),
            ("now_ms", int(900)),
        ],
    )?;
    let moved = [
        ("id", text("t")),
        ("from_state", text("payment_pending")),
        ("to_state", text("email_pending")),
        ("fencing_token", int(0)),
        ("run_at_ms", int(2_000)),
        ("now_ms", int(1_100)),
    ];
    ok(&node, "FLOW.TRANSITION", &moved)?;
    let record = get(&node, "t", &[])?;
    assert_eq!(field(&record, "state"), text("email_pending"));
    assert_eq!(field(&record, "next_run_at_ms"), int(2_000));
    assert_eq!(field(&record, "version"), int(2));
    let at = |state: &str| {
        [
            ("state", text(state)),
            ("lease_ms", int(30_000)),
            ("now_ms", int(2_000)),
        ]
    };
    assert_eq!(
        claim_at(&node, "checkout", &at("payment_pending"))?,
        Vec::<Term>::new()
    );
    assert_eq!(claim_at(&node, "checkout", &at("email_pending"))?.len(), 1);
    let names: Vec<Term> = events(&node, "t", &[])?
        .iter()
        .map(|fields| field(fields, "event"))
        .collect();
    assert_eq!(
        names,
        [text("created"), text("transitioned"), text("claimed")]
    );

    create(
        &node,
        "r",
        "checkout",
        &[("run_at_ms", int(1_000)), ("now_ms", int(900))],
    )?;
    let running = run(
        &node,
        "FLOW.TRANSITION",
        &[
            ("id", text("r")),
            ("from_state", text("queued")),
            ("to_state", text("running")),
            ("fencing_token", int(0)),
            ("now_ms", int(1_000)),
        ],
    )?;
    assert_eq!(
        refusal(running)?,
        "ERR flow running state is only entered by FLOW.CLAIM_DUE"
    );
    assert_eq!(field(&get(&node, "r", &[])?, "state"), text("queued"));
    Ok(())
}
