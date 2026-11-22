//! `flow_test/sections/flow_fail_many_atomically_fails_one_partition_batch.exs`,
//! its batches: `FAIL_MANY`, `CANCEL_MANY` and `RETRY_MANY` finish a
//! partition's Flows together or not at all; in a batch whose items name
//! their own partitions each partition is decided together and apart from
//! the others; an independent batch answers per item. (Elixir's unit is the
//! shard a client cannot see; the node's is the partition. Its pre-read
//! hooks are Elixir's.)

use wf_application::commands::flow::Term;
use wf_node::composition::QueueNode;

use super::{claimed, field, get, int, map, ok, refusal, run, text};
use crate::{TestResult, directory, open};

const P: &str = "tenant-terminal";
const STALE: &str = "ERR stale flow lease";

fn in_partition(partition: &str) -> [(&'static str, Term); 1] {
    [("partition_key", text(partition))]
}

/// Creates `ids` of `flow_type`, queued in `partition` and due at 1000.
fn create_many(node: &QueueNode, partition: &str, flow_type: &str, ids: &[&str]) -> TestResult {
    ok(
        node,
        "FLOW.CREATE_MANY",
        &[
            ("partition_key", text(partition)),
            ("type", text(flow_type)),
            ("state", text("queued")),
            ("run_at_ms", int(1_000)),
            ("now_ms", int(900)),
            ("items", Term::List(ids.iter().map(|id| text(id)).collect())),
        ],
    )?;
    Ok(())
}

/// Claims every due `flow_type` Flow in `partition`, sorted by id.
fn claim_sorted(node: &QueueNode, partition: &str, flow_type: &str) -> TestResult<Vec<Term>> {
    let mut records = claimed(
        node,
        &[
            ("type", text(flow_type)),
            ("partition_key", text(partition)),
            ("worker", text("worker-terminal")),
            ("limit", int(10)),
            ("now_ms", int(1_000)),
        ],
    )?;
    records.sort_by_key(|record| match field(record, "id") {
        Term::Bin(id) => id,
        _ => Vec::new(),
    });
    Ok(records)
}

/// Lease items for `records`; `stale` gets its fence raised by one, and
/// `with_partition` names each record's partition in its item.
fn lease_items(records: &[Term], stale: Option<&str>, with_partition: bool) -> TestResult<Term> {
    let mut items = Vec::new();
    for record in records {
        let Term::Int(mut fence) = field(record, "fencing_token") else {
            return Err("no fence".into());
        };
        if stale.is_some_and(|id| field(record, "id") == text(id)) {
            fence = fence.saturating_add(1);
        }
        let mut entries = vec![
            ("id", field(record, "id")),
            ("lease_token", field(record, "lease_token")),
            ("fencing_token", int(fence)),
        ];
        if with_partition {
            entries.push(("partition_key", field(record, "partition_key")));
        }
        items.push(map(&entries));
    }
    Ok(Term::List(items))
}

fn state_and_version(node: &QueueNode, partition: &str, id: &str) -> TestResult<(Term, Term)> {
    let record = get(node, id, &in_partition(partition))?;
    Ok((field(&record, "state"), field(&record, "version")))
}

fn info(node: &QueueNode, partition: &str, flow_type: &str, count: &str) -> TestResult<Term> {
    let info = ok(
        node,
        "FLOW.INFO",
        &[
            ("type", text(flow_type)),
            ("partition_key", text(partition)),
        ],
    )?;
    Ok(field(&info, count))
}

/// The record's `error_ref` is a reference, not the text it stands for.
fn holds_a_reference(node: &QueueNode, id: &str, written: &str) -> TestResult {
    let error_ref = field(&get(node, id, &in_partition(P))?, "error_ref");
    assert!(matches!(&error_ref, Term::Bin(_)), "{error_ref:?}");
    assert_ne!(error_ref, text(written));
    Ok(())
}

#[test]
fn a_failure_batch_fails_a_partition_together_or_not_at_all() -> TestResult {
    // Elixir: "flow_fail_many atomically fails one-partition batch" and
    // "flow_fail_many rolls back when any item fails guard" (its stale
    // item sorts first).
    let node = open(&directory("audit-terminal-fail")?, 4)?;
    create_many(&node, P, "bulk-fail", &["fail-a", "fail-b"])?;
    let records = claim_sorted(&node, P, "bulk-fail")?;
    ok(
        &node,
        "FLOW.FAIL_MANY",
        &[
            ("partition_key", text(P)),
            ("items", lease_items(&records, None, false)?),
            ("error", text("error-batch")),
            ("now_ms", int(2_000)),
        ],
    )?;
    for id in ["fail-a", "fail-b"] {
        assert_eq!(state_and_version(&node, P, id)?.0, text("failed"));
        holds_a_reference(&node, id, "error-batch")?;
    }
    assert_eq!(info(&node, P, "bulk-fail", "failed")?, int(2));

    create_many(&node, P, "bulk-fail-rollback", &["fail-bad", "fail-good"])?;
    let records = claim_sorted(&node, P, "bulk-fail-rollback")?;
    let stale = [
        ("partition_key", text(P)),
        ("items", lease_items(&records, Some("fail-bad"), false)?),
        ("now_ms", int(2_000)),
    ];
    assert_eq!(refusal(run(&node, "FLOW.FAIL_MANY", &stale)?)?, STALE);
    for id in ["fail-bad", "fail-good"] {
        assert_eq!(state_and_version(&node, P, id)?, (text("running"), int(2)));
    }
    Ok(())
}

#[test]
fn a_cancel_batch_cancels_a_partition_together_or_not_at_all() -> TestResult {
    // Elixir: "flow_cancel_many atomically cancels one-partition queued
    // batch" and "flow_cancel_many rolls back when any item fails guard".
    let node = open(&directory("audit-terminal-cancel")?, 4)?;
    let fenced = |items: &[(&str, i64)]| {
        Term::List(
            items
                .iter()
                .map(|(id, fence)| map(&[("id", text(id)), ("fencing_token", int(*fence))]))
                .collect(),
        )
    };
    create_many(&node, P, "bulk-cancel", &["cancel-a", "cancel-b"])?;
    ok(
        &node,
        "FLOW.CANCEL_MANY",
        &[
            ("partition_key", text(P)),
            ("items", fenced(&[("cancel-a", 0), ("cancel-b", 0)])),
            ("reason", text("cancel-batch")),
            ("now_ms", int(2_000)),
        ],
    )?;
    for id in ["cancel-a", "cancel-b"] {
        assert_eq!(state_and_version(&node, P, id)?.0, text("cancelled"));
        holds_a_reference(&node, id, "cancel-batch")?;
    }
    assert_eq!(info(&node, P, "bulk-cancel", "cancelled")?, int(2));

    create_many(
        &node,
        P,
        "bulk-cancel-rollback",
        &["cancel-good", "cancel-bad"],
    )?;
    let stale = [
        ("partition_key", text(P)),
        ("items", fenced(&[("cancel-good", 0), ("cancel-bad", 1)])),
        ("now_ms", int(2_000)),
    ];
    assert_eq!(refusal(run(&node, "FLOW.CANCEL_MANY", &stale)?)?, STALE);
    for id in ["cancel-good", "cancel-bad"] {
        assert_eq!(state_and_version(&node, P, id)?, (text("queued"), int(1)));
    }
    Ok(())
}

/// Elixir's mixed batch for `command`: a stale item and a good one share
/// partition `A`; partition `B` holds one good item. `claim` leases them
/// first; the batch names no partition of its own.
fn mixed_batch(
    node: &QueueNode,
    command: &str,
    claim: bool,
    extra: &[(&str, Term)],
) -> TestResult<Term> {
    let flow_type = format!("mixed-{command}");
    let (a, b) = (format!("{flow_type}-a"), format!("{flow_type}-b"));
    create_many(node, &a, &flow_type, &["bad", "same"])?;
    create_many(node, &b, &flow_type, &["other"])?;
    let items = if claim {
        let mut records = claim_sorted(node, &a, &flow_type)?;
        records.extend(claim_sorted(node, &b, &flow_type)?);
        lease_items(&records, Some("bad"), true)?
    } else {
        let item = |id: &str, partition: &str, fence: i64| {
            map(&[
                ("id", text(id)),
                ("partition_key", text(partition)),
                ("fencing_token", int(fence)),
            ])
        };
        Term::List(vec![
            item("bad", &a, 1),
            item("same", &a, 0),
            item("other", &b, 0),
        ])
    };
    let mut entries = vec![("items", items), ("now_ms", int(2_000))];
    entries.extend(extra.iter().cloned());
    ok(node, command, &entries)
}

#[test]
fn a_mixed_batch_decides_each_partition_together() -> TestResult {
    // Elixir: "flow_retry_many spans shards and rolls back failing shard
    // group", "flow_fail_many spans ..." and "flow_cancel_many spans ...":
    // the stale item's partition stays as it was, the other partition
    // moves.
    let node = open(&directory("audit-terminal-mixed")?, 4)?;
    let stale = Term::List(vec![text("error"), text(STALE)]);
    for (command, claim, before, after) in [
        ("FLOW.RETRY_MANY", true, "running", "queued"),
        ("FLOW.FAIL_MANY", true, "running", "failed"),
        ("FLOW.CANCEL_MANY", false, "queued", "cancelled"),
    ] {
        let extra = [("run_at_ms", int(2_000))];
        let extra: &[(&str, Term)] = if command == "FLOW.RETRY_MANY" {
            &extra
        } else {
            &[]
        };
        let reply = mixed_batch(&node, command, claim, extra)?;
        assert_eq!(
            reply,
            Term::List(vec![stale.clone(), stale.clone(), text("ok")]),
            "{command}"
        );
        let flow_type = format!("mixed-{command}");
        for id in ["bad", "same"] {
            let partition = format!("{flow_type}-a");
            assert_eq!(
                state_and_version(&node, &partition, id)?.0,
                text(before),
                "{command} {id}"
            );
        }
        let partition = format!("{flow_type}-b");
        assert_eq!(
            state_and_version(&node, &partition, "other")?.0,
            text(after),
            "{command}"
        );
    }
    Ok(())
}

#[test]
fn a_mixed_batch_that_succeeds_finishes_every_partition() -> TestResult {
    // Elixir: "terminal local-only mixed complete_many skips cross-shard
    // pre-read" (its pre-read hook is Elixir's).
    let node = open(&directory("audit-terminal-mixed-ok")?, 4)?;
    let mut records = Vec::new();
    for partition in ["local-a", "local-b"] {
        create_many(
            &node,
            partition,
            "local-only",
            &[&format!("done-{partition}")],
        )?;
        records.extend(claim_sorted(&node, partition, "local-only")?);
    }
    let reply = ok(
        &node,
        "FLOW.COMPLETE_MANY",
        &[
            ("items", lease_items(&records, None, true)?),
            ("now_ms", int(2_000)),
        ],
    )?;
    assert_eq!(reply, text("OK"));
    for partition in ["local-a", "local-b"] {
        let id = format!("done-{partition}");
        assert_eq!(
            state_and_version(&node, partition, &id)?.0,
            text("completed")
        );
    }
    Ok(())
}

#[test]
fn independent_terminal_batches_keep_their_successes() -> TestResult {
    // Elixir: "terminal many independent keeps successful items when one
    // item fails" and "terminal many independent does not pre-read records
    // for cross-shard planning" (its pre-read hook is Elixir's).
    let node = open(&directory("audit-terminal-independent")?, 4)?;
    let answer = Term::List(vec![
        Term::List(vec![text("error"), text(STALE)]),
        text("ok"),
    ]);
    for (command, after) in [
        ("FLOW.COMPLETE_MANY", "completed"),
        ("FLOW.RETRY_MANY", "queued"),
        ("FLOW.FAIL_MANY", "failed"),
    ] {
        let flow_type = format!("independent-{command}");
        create_many(&node, &flow_type, &flow_type, &["a-bad", "b-good"])?;
        let records = claim_sorted(&node, &flow_type, &flow_type)?;
        let mut entries = vec![
            ("partition_key", text(&flow_type)),
            ("items", lease_items(&records, Some("a-bad"), false)?),
            ("now_ms", int(2_000)),
            ("independent", Term::Bool(true)),
        ];
        if command == "FLOW.RETRY_MANY" {
            entries.push(("run_at_ms", int(3_000)));
        }
        let reply = ok(&node, command, &entries)?;
        assert_eq!(reply, answer, "{command}");
        let partition = in_partition(&flow_type);
        let state = |id: &str| get(&node, id, &partition).map(|record| field(&record, "state"));
        assert_eq!(state("a-bad")?, text("running"), "{command}");
        assert_eq!(state("b-good")?, text(after), "{command}");
    }
    create_many(
        &node,
        P,
        "independent-cancel",
        &["cancel-bad", "cancel-good"],
    )?;
    let item = |id: &str, fence: i64| map(&[("id", text(id)), ("fencing_token", int(fence))]);
    let reply = ok(
        &node,
        "FLOW.CANCEL_MANY",
        &[
            ("partition_key", text(P)),
            (
                "items",
                Term::List(vec![item("cancel-bad", 1), item("cancel-good", 0)]),
            ),
            ("now_ms", int(2_000)),
            ("independent", Term::Bool(true)),
        ],
    )?;
    assert_eq!(reply, answer);
    assert_eq!(state_and_version(&node, P, "cancel-bad")?.0, text("queued"));
    assert_eq!(
        state_and_version(&node, P, "cancel-good")?.0,
        text("cancelled")
    );
    Ok(())
}
