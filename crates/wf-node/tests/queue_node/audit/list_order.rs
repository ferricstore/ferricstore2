//! `flow_test/sections/flow_list_auto_partitions_merges_hot_ids_before_
//! hydration.exs`, as the SDK's `FLOW.QUERY` text reaches it: a list in
//! one partition orders before its count, and a projected list leaves the
//! heavy fields out. (Its lists across auto buckets are Elixir's in-process
//! `Flow.list`; a query names its partition.)

use wf_application::commands::flow::Term;
use wf_node::composition::QueueNode;

use super::{field, int, map, ok, text};
use crate::{TestResult, directory, open};

const BUCKET: &str = "__flow_auto__:0";

fn create(
    node: &QueueNode,
    id: &str,
    flow_type: &str,
    now: i64,
    extra: &[(&str, Term)],
) -> TestResult {
    let mut entries = vec![
        ("id", text(id)),
        ("type", text(flow_type)),
        ("partition_key", text(BUCKET)),
        ("now_ms", int(now)),
        ("run_at_ms", int(now)),
    ];
    entries.extend(extra.iter().cloned());
    ok(node, "FLOW.CREATE", &entries)?;
    Ok(())
}

fn records(node: &QueueNode, flow_type: &str, tail: &str) -> TestResult<Vec<Term>> {
    let query = format!(
        "FROM runs WHERE partition_key = @partition_key AND type = @type AND state = @state {tail}"
    );
    let params = map(&[
        ("partition_key", text(BUCKET)),
        ("type", text(flow_type)),
        ("state", text("queued")),
    ]);
    let result = ok(
        node,
        "FLOW.QUERY",
        &[
            ("version", text("FQL1")),
            ("query", text(&query)),
            ("params", params),
        ],
    )?;
    match field(&result, "records") {
        Term::List(records) => Ok(records),
        other => Err(format!("records expected, got {other:?}").into()),
    }
}

#[test]
fn a_list_in_one_partition_orders_before_its_count() -> TestResult {
    // Elixir: "Flow list applies reverse ordering before count on an
    // explicit partition".
    let node = open(&directory("audit-list-reverse")?, 4)?;
    create(&node, "reverse-0", "partition-list-reverse", 1_100, &[])?;
    create(&node, "reverse-1", "partition-list-reverse", 1_101, &[])?;
    let newest = records(
        &node,
        "partition-list-reverse",
        "ORDER BY updated_at_ms DESC LIMIT 1 RETURN RECORDS",
    )?;
    let ids: Vec<Term> = newest.iter().map(|record| field(record, "id")).collect();
    assert_eq!(ids, [text("reverse-1")]);
    Ok(())
}

#[test]
fn a_projected_list_leaves_heavy_fields_out() -> TestResult {
    // Elixir: "Flow list return meta trims heavy fields through embedded
    // API" (its `return: :meta` as a projection).
    let node = open(&directory("audit-list-meta")?, 4)?;
    create(
        &node,
        "meta-flow",
        "auto-list-meta",
        2_500,
        &[
            ("parent_flow_id", text("meta-parent")),
            ("root_flow_id", text("meta-root")),
            ("correlation_id", text("meta-correlation")),
            ("payload", text(&"x".repeat(1_024))),
        ],
    )?;
    let projected = records(
        &node,
        "auto-list-meta",
        "ORDER BY updated_at_ms ASC LIMIT 10 RETURN RECORDS (run_id, type, state)",
    )?;
    let [record] = projected.as_slice() else {
        return Err(format!("one record expected, got {projected:?}").into());
    };
    let Term::Map(entries) = record else {
        return Err(format!("a record map expected, got {record:?}").into());
    };
    let mut names: Vec<&[u8]> = entries.iter().map(|(name, _)| name.as_slice()).collect();
    names.sort_unstable();
    for heavy in [
        "payload",
        "parent_flow_id",
        "root_flow_id",
        "correlation_id",
    ] {
        assert!(!names.contains(&heavy.as_bytes()), "{heavy} in {record:?}");
    }
    assert_eq!(field(record, "type"), text("auto-list-meta"));
    assert_eq!(field(record, "state"), text("queued"));
    Ok(())
}
