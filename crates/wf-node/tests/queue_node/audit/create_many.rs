//! `FLOW.CREATE_MANY` beyond the queue fast path (audit gap G2): Elixir's
//! create-many tests in `flow_test/sections` (6a5f2bc34).

use wf_application::commands::flow::Term;
use wf_node::composition::QueueNode;

use super::{events, field, get, int, map, ok, refusal, run, text};
use crate::{TestResult, directory, open};

fn node(tag: &str) -> TestResult<QueueNode> {
    open(&directory(tag)?, 4)
}

fn many(node: &QueueNode, entries: &[(&str, Term)]) -> TestResult<super::Reply> {
    run(node, "FLOW.CREATE_MANY", entries)
}

fn item(entries: &[(&str, Term)]) -> Term {
    map(entries)
}

fn created(node: &QueueNode, id: &str, partition: &str) -> TestResult<Term> {
    get(node, id, &[("partition_key", text(partition))])
}

fn names(node: &QueueNode, id: &str, partition: &str) -> TestResult<Vec<Term>> {
    Ok(events(node, id, &[("partition_key", text(partition))])?
        .iter()
        .map(|fields| field(fields, "event"))
        .collect())
}

#[test]
fn flow_create_many_creates_one_partition_batch_atomically() -> TestResult {
    let node = node("audit-many-atomic")?;
    let reply = many(
        &node,
        &[
            ("partition_key", text("t")),
            ("type", text("bulk")),
            ("run_at_ms", int(1_000)),
            ("now_ms", int(1_000)),
            (
                "items",
                Term::List(vec![
                    item(&[("id", text("a")), ("payload", text("payload:a"))]),
                    item(&[("id", text("b")), ("payload", text("payload:b"))]),
                ]),
            ),
        ],
    )??;
    assert_eq!(reply, text("OK"));
    for id in ["a", "b"] {
        assert_eq!(field(&created(&node, id, "t")?, "partition_key"), text("t"));
    }
    Ok(())
}

#[test]
fn flow_create_many_lets_item_attrs_override_common_attrs() -> TestResult {
    let node = node("audit-many-override")?;
    many(
        &node,
        &[
            ("partition_key", text("t")),
            ("type", text("bulk")),
            ("payload", text("payload:common")),
            ("correlation_id", text("corr:common")),
            ("run_at_ms", int(1_000)),
            ("now_ms", int(1_000)),
            (
                "items",
                Term::List(vec![
                    item(&[("id", text("a"))]),
                    item(&[
                        ("id", text("b")),
                        ("payload", text("payload:item")),
                        ("correlation_id", text("corr:item")),
                    ]),
                ]),
            ),
        ],
    )??;
    let full = |id: &str| {
        get(
            &node,
            id,
            &[("partition_key", text("t")), ("full", Term::Bool(true))],
        )
    };
    let (a, b) = (full("a")?, full("b")?);
    assert_eq!(field(&a, "correlation_id"), text("corr:common"));
    assert_eq!(field(&a, "payload"), text("payload:common"));
    assert_eq!(field(&b, "correlation_id"), text("corr:item"));
    assert_eq!(field(&b, "payload"), text("payload:item"));
    assert_ne!(field(&a, "payload_ref"), field(&b, "payload_ref"));
    Ok(())
}

#[test]
fn flow_create_many_auto_partitions_missing_partition_and_rolls_back_duplicate_batches()
-> TestResult {
    let node = node("audit-many-rollback")?;
    many(
        &node,
        &[
            ("type", text("bulk")),
            ("items", Term::List(vec![item(&[("id", text("n"))])])),
        ],
    )??;
    let auto = get(&node, "n", &[])?;
    assert!(
        matches!(field(&auto, "partition_key"), Term::Bin(ref name) if name.starts_with(b"__flow_auto__:"))
    );
    ok(
        &node,
        "FLOW.CREATE",
        &[
            ("id", text("x")),
            ("type", text("bulk")),
            ("partition_key", text("t")),
            ("run_at_ms", int(1_000)),
        ],
    )?;
    let batch = |ids: &[&str]| {
        many(
            &node,
            &[
                ("partition_key", text("t")),
                ("type", text("bulk")),
                ("run_at_ms", int(1_000)),
                (
                    "items",
                    Term::List(ids.iter().map(|id| item(&[("id", text(id))])).collect()),
                ),
            ],
        )
    };
    assert_eq!(refusal(batch(&["x", "n"])?)?, "ERR flow already exists");
    assert_eq!(created(&node, "n", "t")?, Term::Nil);
    assert_eq!(names(&node, "n", "t")?, Vec::<Term>::new());
    assert_eq!(
        refusal(batch(&["n", "n"])?)?,
        "ERR flow duplicate id in batch"
    );
    assert_eq!(created(&node, "n", "t")?, Term::Nil);
    Ok(())
}

#[test]
fn flow_create_many_independent_keeps_successful_items_when_one_create_fails() -> TestResult {
    let node = node("audit-many-independent")?;
    ok(
        &node,
        "FLOW.CREATE",
        &[
            ("id", text("x")),
            ("type", text("bulk")),
            ("partition_key", text("t")),
        ],
    )?;
    let reply = many(
        &node,
        &[
            ("partition_key", text("t")),
            ("type", text("bulk")),
            ("independent", Term::Bool(true)),
            (
                "items",
                Term::List(vec![item(&[("id", text("x"))]), item(&[("id", text("n"))])]),
            ),
        ],
    )??;
    assert_eq!(
        reply,
        Term::List(vec![
            Term::List(vec![text("error"), text("ERR flow already exists")]),
            text("ok"),
        ])
    );
    assert_eq!(field(&created(&node, "n", "t")?, "state"), text("queued"));
    Ok(())
}

fn options<'a>(extra: &[(&'a str, Term)]) -> Vec<(&'a str, Term)> {
    let mut all = vec![
        ("type", text("bulk")),
        ("partition_key", text("t")),
        ("run_at_ms", int(1_000)),
        ("now_ms", int(1_000)),
    ];
    all.extend(extra.iter().cloned());
    all
}

#[test]
fn flow_create_many_idempotent_retry_returns_existing_records_without_duplicate_writes()
-> TestResult {
    let node = node("audit-many-idempotent")?;
    ok(&node, "FLOW.CREATE", &options(&[("id", text("x"))]))?;
    let reply = many(
        &node,
        &options(&[
            ("idempotent", Term::Bool(true)),
            (
                "items",
                Term::List(vec![
                    item(&[("id", text("x"))]),
                    item(&[("id", text("n")), ("payload", text("payload:n"))]),
                ]),
            ),
        ]),
    )??;
    assert_eq!(reply, text("OK"));
    assert_eq!(names(&node, "x", "t")?, vec![text("created")]);
    assert_eq!(field(&created(&node, "n", "t")?, "id"), text("n"));
    Ok(())
}

#[test]
fn flow_create_many_idempotency_conflict_rolls_back_same_shard_group() -> TestResult {
    let node = node("audit-many-conflict")?;
    ok(
        &node,
        "FLOW.CREATE",
        &[
            ("id", text("x")),
            ("type", text("bulk")),
            ("partition_key", text("t")),
            ("payload", text("payload:old")),
            ("run_at_ms", int(1_000)),
        ],
    )?;
    let reply = many(
        &node,
        &[
            ("partition_key", text("t")),
            ("type", text("bulk")),
            ("run_at_ms", int(1_000)),
            ("idempotent", Term::Bool(true)),
            (
                "items",
                Term::List(vec![
                    item(&[("id", text("x")), ("payload", text("payload:new"))]),
                    item(&[("id", text("n")), ("payload", text("payload:n"))]),
                ]),
            ),
        ],
    )?;
    assert_eq!(refusal(reply)?, "ERR flow idempotency conflict");
    assert_eq!(created(&node, "n", "t")?, Term::Nil);
    assert_eq!(names(&node, "n", "t")?, Vec::<Term>::new());
    Ok(())
}

/// Two partitions on one shard and one on another, for a 4-shard node.
fn mixed_partitions() -> (String, String, String) {
    use wf_application::commands::queue::Partition;
    use wf_application::commands::queue::shard_for_slot;
    let shard = |name: &str| shard_for_slot(Partition::Named(name.as_bytes().to_vec()).slot(), 4);
    let names: Vec<String> = (0..200).map(|index| format!("mixed-{index}")).collect();
    let first = names.first().cloned().unwrap_or_default();
    let same = names
        .iter()
        .skip(1)
        .find(|name| shard(name) == shard(&first))
        .cloned()
        .unwrap_or_default();
    let other = names
        .iter()
        .find(|name| shard(name) != shard(&first))
        .cloned()
        .unwrap_or_default();
    (first, same, other)
}

#[test]
fn flow_create_many_spans_shards_and_rolls_back_failing_shard_group() -> TestResult {
    let node = node("audit-many-mixed")?;
    let (same_a, same_b, other) = mixed_partitions();
    ok(
        &node,
        "FLOW.CREATE",
        &[
            ("id", text("x")),
            ("type", text("bulk")),
            ("partition_key", text(&same_a)),
            ("run_at_ms", int(1_000)),
        ],
    )?;
    let reply = many(
        &node,
        &[
            ("type", text("bulk")),
            ("run_at_ms", int(1_000)),
            ("now_ms", int(1_000)),
            (
                "items",
                Term::List(vec![
                    item(&[("id", text("x")), ("partition_key", text(&same_a))]),
                    item(&[("id", text("s")), ("partition_key", text(&same_b))]),
                    item(&[("id", text("o")), ("partition_key", text(&other))]),
                ]),
            ),
        ],
    )??;
    let exists = Term::List(vec![text("error"), text("ERR flow already exists")]);
    assert_eq!(reply, Term::List(vec![exists.clone(), exists, text("ok")]));
    assert_eq!(created(&node, "s", &same_b)?, Term::Nil);
    assert_eq!(names(&node, "s", &same_b)?, Vec::<Term>::new());
    assert_eq!(names(&node, "o", &other)?, vec![text("created")]);
    Ok(())
}
