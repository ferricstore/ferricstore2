//! `flow_attributes_test.exs`, the read side. Elixir's tests list and search
//! through its in-process API; its `FLOW.LIST` and `FLOW.SEARCH` commands are
//! retired, and clients list and search with `FLOW.QUERY`. These ports send
//! the queries the Python SDK's `list` and `search` build.

use std::collections::BTreeSet;

use wf_application::commands::flow::Term;
use wf_node::composition::QueueNode;

use super::attributes::{P, claim_one, create, lease, plus};
use super::{field, get, int, map, ok, text, texts};
use crate::{TestResult, directory, open};

/// One `FLOW.QUERY` with the SDK's parameters; the ids it returns, in order.
fn query(node: &QueueNode, fql: &str, params: &[(&str, Term)]) -> TestResult<Vec<String>> {
    let mut all = vec![("partition_key", text(P))];
    all.extend(params.iter().cloned());
    let result = ok(
        node,
        "FLOW.QUERY",
        &[
            ("version", text("FQL1")),
            ("query", text(fql)),
            ("params", map(&all)),
        ],
    )?;
    let Term::List(records) = field(&result, "records") else {
        return Err(format!("no records in {result:?}").into());
    };
    Ok(records
        .iter()
        .filter_map(|record| match field(record, "id") {
            Term::Bin(id) => Some(String::from_utf8_lossy(&id).into_owned()),
            _ => None,
        })
        .collect())
}

const SCOPE: &str = "FROM runs WHERE partition_key = @partition_key";
const ORDER: &str = "ORDER BY updated_at_ms ASC";

/// The SDK's `list(type, state, attributes={name: ...})`.
fn list(
    node: &QueueNode,
    flow_type: &str,
    state: &str,
    name: &str,
    value: Term,
) -> TestResult<Vec<String>> {
    let fql = format!(
        "{SCOPE} AND type = @type AND state = @state AND attribute['{name}'] = @attribute_0 {ORDER} LIMIT 10 RETURN RECORDS"
    );
    query(
        node,
        &fql,
        &[
            ("type", text(flow_type)),
            ("state", text(state)),
            ("attribute_0", value),
        ],
    )
}

/// The SDK's `search(type=..., state=..., attributes={"tenant": "acme"})`;
/// `None` leaves the type or state out.
fn search(
    node: &QueueNode,
    flow_type: Option<&str>,
    state: Option<&str>,
    count: i64,
) -> TestResult<Vec<String>> {
    let mut fql = SCOPE.to_owned();
    let mut params = Vec::new();
    if let Some(flow_type) = flow_type {
        fql.push_str(" AND type = @type");
        params.push(("type", text(flow_type)));
    }
    if let Some(state) = state {
        fql.push_str(" AND state = @state");
        params.push(("state", text(state)));
    }
    fql.push_str(" AND attribute['tenant'] = @attribute_0 ");
    fql.push_str(ORDER);
    fql.push_str(&[" LIMIT ", &count.to_string(), " RETURN RECORDS"].concat());
    params.push(("attribute_0", text("acme")));
    query(node, &fql, &params)
}

fn set(ids: &[&str]) -> BTreeSet<String> {
    ids.iter().map(|id| (*id).to_owned()).collect()
}

fn found(ids: Vec<String>) -> BTreeSet<String> {
    ids.into_iter().collect()
}

fn index(node: &QueueNode, flow_type: &str, names: &[&str]) -> TestResult {
    ok(
        node,
        "FLOW.POLICY.SET",
        &[
            ("type", text(flow_type)),
            ("indexed_attributes", texts(names)),
        ],
    )?;
    Ok(())
}

fn tenant(value: &str) -> Term {
    map(&[("tenant", text(value))])
}

#[test]
fn queries_and_stats_filter_by_attributes() -> TestResult {
    // Elixir: "list and stats filter by projected attributes".
    let node = open(&directory("audit-attrq-list")?, 4)?;
    let spring = |who: &str| map(&[("tenant", text(who)), ("campaign", text("spring"))]);
    create(&node, "acme", "attrs-query", Some(spring("acme")), 1_000)?;
    create(&node, "other", "attrs-query", Some(spring("other")), 1_000)?;
    assert_eq!(
        list(&node, "attrs-query", "queued", "tenant", text("acme"))?,
        ["acme"]
    );
    let stats = ok(
        &node,
        "FLOW.STATS",
        &[
            ("type", text("attrs-query")),
            ("state", text("queued")),
            ("partition_key", text(P)),
            ("attributes", tenant("acme")),
        ],
    )?;
    assert_eq!(field(&stats, "count"), int(1));
    assert_eq!(field(&stats, "attributes"), tenant("acme"));
    Ok(())
}

/// Creates, claims and completes `id`, merging `phase` at completion.
fn completed(node: &QueueNode, id: &str, flow_type: &str, now: i64) -> TestResult<Term> {
    create(
        node,
        id,
        flow_type,
        Some(map(&[("tenant", text("acme")), ("phase", text("queued"))])),
        now,
    )?;
    let claim = claim_one(node, flow_type, "queued", now + 1)?;
    let done = [
        ("attributes", map(&[("phase", text("done"))])),
        ("now_ms", int(now + 2)),
    ];
    ok(node, "FLOW.COMPLETE", &plus(lease(&claim), &done))?;
    get(node, id, &[("partition_key", text(P))])
}

#[test]
fn completed_flows_stay_queryable_by_attribute_until_retention_cleanup() -> TestResult {
    // Elixir: "terminal records keep attributes queryable through async
    // projection" and "retention cleanup removes terminal attribute
    // indexes".
    let node = open(&directory("audit-attrq-terminal")?, 4)?;
    index(&node, "attrs-terminal", &["tenant"])?;
    let record = completed(&node, "done", "attrs-terminal", 1_000)?;
    assert_eq!(
        list(&node, "attrs-terminal", "completed", "phase", text("done"))?,
        ["done"]
    );
    assert_eq!(
        list(&node, "attrs-terminal", "completed", "tenant", text("acme"))?,
        ["done"]
    );
    assert_eq!(search(&node, Some("attrs-terminal"), None, 10)?, ["done"]);
    assert_eq!(search(&node, None, Some("completed"), 10)?, ["done"]);
    assert_eq!(search(&node, None, None, 10)?, ["done"]);
    let Term::Int(until) = field(&record, "terminal_retention_until_ms") else {
        return Err(format!("no retention deadline in {record:?}").into());
    };
    let cleaned = ok(
        &node,
        "FLOW.RETENTION_CLEANUP",
        &[("limit", int(10)), ("now_ms", int(until + 1))],
    )?;
    assert!(matches!(field(&cleaned, "flows"), Term::Int(flows) if flows >= 1));
    assert_eq!(
        get(&node, "done", &[("partition_key", text(P))])?,
        Term::Nil
    );
    let empty: Vec<String> = Vec::new();
    assert_eq!(
        list(&node, "attrs-terminal", "completed", "tenant", text("acme"))?,
        empty
    );
    assert_eq!(search(&node, Some("attrs-terminal"), None, 10)?, empty);
    assert_eq!(search(&node, None, Some("completed"), 10)?, empty);
    assert_eq!(search(&node, None, None, 10)?, empty);
    Ok(())
}

#[test]
fn attribute_queries_and_discovery_survive_a_restart() -> TestResult {
    // Elixir: "restart rebuild preserves attribute search correctness".
    let path = directory("audit-attrq-restart")?;
    let both = map(&[("tenant", text("acme")), ("region", text("us"))]);
    {
        let node = open(&path, 4)?;
        index(&node, "attrs-restart", &["tenant", "region"])?;
        create(&node, "r", "attrs-restart", Some(both), 1_000)?;
        assert_eq!(
            list(&node, "attrs-restart", "queued", "tenant", text("acme"))?,
            ["r"]
        );
    }
    let node = open(&path, 4)?;
    assert_eq!(
        list(&node, "attrs-restart", "queued", "region", text("us"))?,
        ["r"]
    );
    assert_eq!(search(&node, None, None, 10)?, ["r"]);
    let scope = [
        ("type", text("attrs-restart")),
        ("state", text("queued")),
        ("partition_key", text(P)),
    ];
    let Term::List(names) = ok(&node, "FLOW.ATTRIBUTES", &scope)? else {
        return Err("attributes are not a list".into());
    };
    let one_named = |name: &str| map(&[("count", int(1)), ("name", text(name))]);
    assert!(names.contains(&one_named("tenant")), "{names:?}");
    assert!(names.contains(&one_named("region")), "{names:?}");
    let mut values_scope = scope.to_vec();
    values_scope.push(("attribute", text("tenant")));
    assert_eq!(
        ok(&node, "FLOW.ATTRIBUTE_VALUES", &values_scope)?,
        Term::List(vec![map(&[("count", int(1)), ("value", text("acme"))])])
    );
    Ok(())
}

#[test]
fn an_explicit_null_attribute_round_trips_and_differs_from_a_missing_one() -> TestResult {
    // Elixir: "explicit null attributes round trip without matching missing
    // attributes". The SDK cannot send a null parameter; FQL asks `IS NULL`.
    let node = open(&directory("audit-attrq-null")?, 4)?;
    create(
        &node,
        "null",
        "attrs-null",
        Some(map(&[("nullable", Term::Nil)])),
        1_000,
    )?;
    create(&node, "missing", "attrs-null", None, 1_000)?;
    let record = get(&node, "null", &[("partition_key", text(P))])?;
    assert_eq!(
        field(&record, "attributes"),
        map(&[("nullable", Term::Nil)])
    );
    let fql = format!(
        "{SCOPE} AND type = @type AND state = @state AND attribute['nullable'] IS NULL {ORDER} LIMIT 10 RETURN RECORDS"
    );
    let ids = query(
        &node,
        &fql,
        &[("type", text("attrs-null")), ("state", text("queued"))],
    )?;
    assert_eq!(ids, ["null"]);
    Ok(())
}

#[test]
fn attribute_search_spans_states_and_types() -> TestResult {
    // Elixir: "indexed attributes support broad cross-state and cross-type
    // search", "indexed attributes support cross-type state search without
    // partition-scope under-return" and "non-indexed attributes remain
    // exact-queryable" (the broad-search refusal belongs to the retired
    // FLOW.SEARCH: FLOW.QUERY filters on any attribute).
    let node = open(&directory("audit-attrq-broad")?, 4)?;
    index(&node, "type-a", &["tenant"])?;
    index(&node, "type-b", &["tenant"])?;
    for (id, flow_type, state, who, now) in [
        ("a1", "type-a", "queued", "acme", 1_000),
        ("a2", "type-a", "review", "acme", 1_100),
        ("b1", "type-b", "queued", "acme", 1_200),
        ("other", "type-b", "queued", "other", 1_300),
    ] {
        let entries = [
            ("id", text(id)),
            ("type", text(flow_type)),
            ("state", text(state)),
            ("partition_key", text(P)),
            ("attributes", tenant(who)),
            ("run_at_ms", int(now)),
            ("now_ms", int(now)),
        ];
        ok(&node, "FLOW.CREATE", &entries)?;
    }
    assert_eq!(
        found(search(&node, None, None, 10)?),
        set(&["a1", "a2", "b1"])
    );
    assert_eq!(
        found(search(&node, Some("type-a"), None, 10)?),
        set(&["a1", "a2"])
    );
    assert_eq!(search(&node, None, Some("queued"), 1)?, ["a1"]);
    assert_eq!(search(&node, None, Some("review"), 1)?, ["a2"]);
    create(
        &node,
        "c",
        "type-a",
        Some(map(&[("customer", text("c1"))])),
        1_400,
    )?;
    assert_eq!(
        list(&node, "type-a", "queued", "customer", text("c1"))?,
        ["c"]
    );
    Ok(())
}

#[test]
fn attribute_search_honors_time_windows_order_and_count() -> TestResult {
    // Elixir: "indexed attribute search honors time reverse and count
    // windows" and "multi-attribute search plans from selective candidate
    // before applying count".
    let node = open(&directory("audit-attrq-window")?, 4)?;
    index(&node, "window", &["tenant", "region"])?;
    create(&node, "old", "window", Some(tenant("acme")), 1_000)?;
    create(&node, "new", "window", Some(tenant("acme")), 2_000)?;
    let window = |from: i64, to: i64| {
        let fql = format!(
            "{SCOPE} AND type = @type AND attribute['tenant'] = @attribute_0 AND updated_at_ms BETWEEN @from_ms AND @to_ms {ORDER} LIMIT 10 RETURN RECORDS"
        );
        query(
            &node,
            &fql,
            &[
                ("type", text("window")),
                ("attribute_0", text("acme")),
                ("from_ms", int(from)),
                ("to_ms", int(to)),
            ],
        )
    };
    assert_eq!(window(1_500, 9_007_199_254_740_991)?, ["new"]);
    assert_eq!(window(0, 1_500)?, ["old"]);
    let newest = format!(
        "{SCOPE} AND type = @type AND attribute['tenant'] = @attribute_0 ORDER BY updated_at_ms DESC LIMIT 1 RETURN RECORDS"
    );
    let params = [("type", text("window")), ("attribute_0", text("acme"))];
    assert_eq!(query(&node, &newest, &params)?, ["new"]);

    for index in 1..=8 {
        let attributes = map(&[("tenant", text("acme")), ("region", text("eu"))]);
        create(
            &node,
            &format!("broad-{index}"),
            "selective",
            Some(attributes),
            1_000 + index,
        )?;
    }
    let winner = map(&[("tenant", text("acme")), ("region", text("us"))]);
    create(&node, "winner", "selective", Some(winner), 2_000)?;
    let fql = format!(
        "{SCOPE} AND type = @type AND attribute['region'] = @attribute_0 AND attribute['tenant'] = @attribute_1 {ORDER} LIMIT 1 RETURN RECORDS"
    );
    let params = [
        ("type", text("selective")),
        ("attribute_0", text("us")),
        ("attribute_1", text("acme")),
    ];
    assert_eq!(query(&node, &fql, &params)?, ["winner"]);
    Ok(())
}

#[test]
fn auto_bucket_discovery_leaves_out_expired_terminal_flows() -> TestResult {
    // Elixir: "auto partition attribute discovery excludes expired terminal
    // projections".
    let node = open(&directory("audit-attrq-auto")?, 4)?;
    index(&node, "auto-expired", &["tenant"])?;
    let stale = i64::try_from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_millis(),
    )? - 60_000;
    ok(
        &node,
        "FLOW.CREATE",
        &[
            ("id", text("x")),
            ("type", text("auto-expired")),
            ("state", text("queued")),
            ("attributes", tenant("acme")),
            ("retention_ttl_ms", int(1)),
            ("run_at_ms", int(stale)),
            ("now_ms", int(stale)),
        ],
    )?;
    let created = get(&node, "x", &[])?;
    let partition = field(&created, "partition_key");
    let records = super::claimed(
        &node,
        &[
            ("type", text("auto-expired")),
            ("states", texts(&["queued"])),
            ("partition_key", partition.clone()),
            ("worker", text("w1")),
            ("limit", int(1)),
            ("now_ms", int(stale + 1)),
        ],
    )?;
    let [claim] = records.as_slice() else {
        return Err("one claim".into());
    };
    ok(
        &node,
        "FLOW.COMPLETE",
        &[
            ("id", text("x")),
            ("partition_key", partition),
            ("lease_token", field(claim, "lease_token")),
            ("fencing_token", field(claim, "fencing_token")),
            ("now_ms", int(stale + 2)),
        ],
    )?;
    // Its retention (1 ms) has ended.
    std::thread::sleep(std::time::Duration::from_millis(5));
    let discovered = ok(
        &node,
        "FLOW.ATTRIBUTES",
        &[
            ("type", text("auto-expired")),
            ("state", text("completed")),
            ("partition_key", text("auto")),
        ],
    )?;
    assert_eq!(discovered, Term::List(Vec::new()));
    Ok(())
}
