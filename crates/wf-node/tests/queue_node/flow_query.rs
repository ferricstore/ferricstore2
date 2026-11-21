//! `FLOW.QUERY` and `FLOW.QUERY.INDEXES` (ADR-014 step 12) through the
//! node: the partition's shard reads its catalog, so collections, counts,
//! point reads (named partition or auto bucket) and history answer at once
//! and after a restart, pages resume from their cursor, and failures carry
//! Elixir's codes.

use wf_application::commands::flow::text::{self, Parsed};
use wf_application::commands::flow::{self, FlowService, Term};
use wf_application::commands::queue::ServiceError;
use wf_node::composition::QueueNode;

use super::{TestResult, call, directory, open};

fn object(entries: Vec<(&str, Term)>) -> Term {
    Term::Map(
        entries
            .into_iter()
            .map(|(key, value)| (key.as_bytes().to_vec(), value))
            .collect(),
    )
}

fn text(value: &str) -> Term {
    Term::text(value)
}

fn run(
    node: &QueueNode,
    name: &str,
    entries: Vec<(&str, Term)>,
) -> TestResult<Result<Term, ServiceError>> {
    let command = flow::parse(name, &object(entries))?;
    call(|reply| node.execute(command, reply))
}

fn query(
    node: &QueueNode,
    query: &str,
    params: Vec<(&str, Term)>,
) -> TestResult<Result<Term, ServiceError>> {
    run(
        node,
        "FLOW.QUERY",
        vec![
            ("version", text("FQL1")),
            ("query", text(query)),
            ("params", object(params)),
        ],
    )
}

fn ids(result: &Term) -> Vec<String> {
    match result.get(b"records") {
        Some(Term::List(records)) => records
            .iter()
            .filter_map(
                |record| match record.get(b"id").or_else(|| record.get(b"event_id")) {
                    Some(Term::Bin(id)) => Some(String::from_utf8_lossy(id).into_owned()),
                    _ => None,
                },
            )
            .collect(),
        _ => Vec::new(),
    }
}

fn path(term: &Term, keys: &[&str]) -> Term {
    keys.iter().fold(term.clone(), |value, key| {
        value.get(key.as_bytes()).cloned().unwrap_or(Term::Nil)
    })
}

fn seed(node: &QueueNode) -> TestResult {
    for (id, state, now, priority, tenant) in [
        ("a", "ready", 1_000, 2, Some("acme")),
        ("b", "ready", 1_001, 0, Some("beta")),
        ("c", "ready", 1_002, 1, None),
        ("d", "review", 1_003, 0, Some("acme")),
    ] {
        let mut entries = vec![
            ("id", text(id)),
            ("type", text("tq")),
            ("partition_key", text("p")),
            ("state", text(state)),
            ("now_ms", Term::Int(now)),
            ("run_at_ms", Term::Int(now)),
            ("priority", Term::Int(priority)),
        ];
        entries.extend(tenant.map(|tenant| ("attributes", object(vec![("tenant", text(tenant))]))));
        run(node, "FLOW.CREATE", entries)??;
    }
    // A Flow of another type, and one in an auto bucket.
    run(
        node,
        "FLOW.CREATE",
        vec![
            ("id", text("e")),
            ("type", text("other")),
            ("partition_key", text("p")),
            ("now_ms", Term::Int(1_004)),
        ],
    )??;
    run(
        node,
        "FLOW.CREATE",
        vec![
            ("id", text("auto-1")),
            ("type", text("tq")),
            ("now_ms", Term::Int(1_005)),
        ],
    )??;
    Ok(())
}

fn check_reads(node: &QueueNode) -> TestResult {
    let params = || vec![("p", text("p")), ("t", text("tq"))];
    let base = "FROM runs WHERE partition_key = @p AND type = @t";
    let ready = query(
        node,
        &format!("{base} AND state = 'ready' ORDER BY updated_at_ms ASC LIMIT 10 RETURN RECORDS"),
        params(),
    )??;
    assert_eq!(ids(&ready), ["a", "b", "c"]);
    let by_priority = query(
        node,
        &format!("{base} ORDER BY priority DESC, updated_at_ms ASC LIMIT 10 RETURN RECORDS"),
        params(),
    )??;
    assert_eq!(ids(&by_priority), ["a", "c", "b", "d"]);
    let acme = query(
        node,
        &format!("{base} AND attribute.tenant = 'acme' RETURN COUNT"),
        params(),
    )??;
    assert_eq!(path(&acme, &["result", "value"]), Term::Int(2));
    let partition = query(
        node,
        "FROM runs WHERE partition_key = @p RETURN COUNT",
        vec![("p", text("p"))],
    )??;
    assert_eq!(path(&partition, &["result", "value"]), Term::Int(5));
    let missing = query(
        node,
        &format!(
            "{base} AND attribute.tenant IS MISSING ORDER BY updated_at_ms ASC LIMIT 10 RETURN RECORDS"
        ),
        params(),
    )??;
    assert_eq!(ids(&missing), ["c"]);
    let point = query(
        node,
        "FROM runs WHERE partition_key = @p AND run_id = 'a' RETURN RECORD (run_id, state, attribute.tenant)",
        vec![("p", text("p"))],
    )??;
    assert_eq!(
        path(&point, &["records"]),
        Term::List(vec![object(vec![
            ("attributes", object(vec![("tenant", text("acme"))])),
            ("id", text("a")),
            ("state", text("ready")),
        ])])
    );
    let auto = query(
        node,
        "FROM runs WHERE run_id = 'auto-1' RETURN RECORD",
        Vec::new(),
    )??;
    assert_eq!(ids(&auto), ["auto-1"]);
    let elsewhere = query(
        node,
        "FROM runs WHERE run_id = 'a' RETURN RECORD",
        Vec::new(),
    )??;
    assert_eq!(ids(&elsewhere).len(), 0);
    Ok(())
}

#[test]
fn queries_read_the_catalog_now_and_after_a_restart() -> TestResult {
    let path_dir = directory("flow-query")?;
    {
        let node = open(&path_dir, 4)?;
        seed(&node)?;
        check_reads(&node)?;
    }
    let node = open(&path_dir, 3)?;
    check_reads(&node)?;
    // Pages resume after the cursor's row.
    let page =
        "FROM runs WHERE partition_key = 'p' AND type = 'tq' ORDER BY updated_at_ms ASC LIMIT 2";
    let first = query(&node, &format!("{page} RETURN RECORDS"), Vec::new())??;
    assert_eq!(ids(&first), ["a", "b"]);
    assert_eq!(path(&first, &["page", "has_more"]), Term::Bool(true));
    let cursor = path(&first, &["page", "cursor"]);
    let second = query(
        &node,
        &format!("{page} CURSOR @c RETURN RECORDS"),
        vec![("c", cursor)],
    )??;
    assert_eq!(ids(&second), ["c", "d"]);
    // History pages too, newest first.
    let events = query(
        &node,
        "FROM events WHERE partition_key = 'p' AND run_id = 'a' ORDER BY event_id DESC LIMIT 5 RETURN RECORDS (event_id)",
        Vec::new(),
    )??;
    assert_eq!(ids(&events), ["1000-1"]);
    // EXPLAIN plans; ANALYZE also runs.
    let analyzed = query(
        &node,
        &format!("EXPLAIN ANALYZE {page} RETURN RECORDS"),
        Vec::new(),
    )??;
    assert_eq!(path(&analyzed, &["status"]), text("executed"));
    assert_eq!(path(&analyzed, &["actual", "result_records"]), Term::Int(2));
    let planned = query(&node, &format!("EXPLAIN {page} RETURN RECORDS"), Vec::new())??;
    assert_eq!(path(&planned, &["status"]), text("planned"));
    assert_eq!(path(&planned, &["actual"]), Term::Nil);
    Ok(())
}

#[test]
fn query_failures_carry_codes_and_indexes_describe_the_catalog() -> TestResult {
    let path_dir = directory("flow-query-errors")?;
    let node = open(&path_dir, 2)?;
    let failure = |result: Result<Term, ServiceError>| match result {
        Err(ServiceError::Query(failure)) => Some(failure.payload()),
        _ => None,
    };
    let unsupported = flow::parse(
        "FLOW.QUERY",
        &object(vec![
            ("version", text("FQL1")),
            (
                "query",
                text(
                    "FROM runs WHERE partition_key = 'p' AND unsupported = 1 ORDER BY updated_at_ms ASC LIMIT 2 RETURN RECORDS",
                ),
            ),
        ]),
    );
    let payload = failure(unsupported.map(|_| Term::Nil)).ok_or("not a query failure")?;
    assert_eq!(path(&payload, &["code"]), text("unsupported_field"));
    assert_eq!(path(&payload, &["position", "byte"]), Term::Int(41));
    assert!(matches!(path(&payload, &["hint"]), Term::Bin(hint) if !hint.is_empty()));
    let missing = query(
        &node,
        "FROM runs WHERE partition_key = @p ORDER BY updated_at_ms ASC LIMIT 2 RETURN RECORDS",
        Vec::new(),
    );
    assert!(matches!(missing, Err(error) if error.to_string() == "ERR FQL1 parameter is missing"));
    let Parsed::Command(indexes) = text::parse(b"FLOW.QUERY.INDEXES", &[])? else {
        return Err("indexes is not a command".into());
    };
    let status = call(|reply| node.execute(indexes, reply))??;
    assert_eq!(
        path(&status, &["contract_version"]),
        text("ferric.flow.query.indexes/v1")
    );
    let Term::List(entries) = path(&status, &["indexes"]) else {
        return Err("no indexes".into());
    };
    assert_eq!(entries.len(), 1);
    let entry = entries.first().ok_or("no index")?;
    assert_eq!(path(entry, &["coverage", "total_shards"]), Term::Int(2));
    let Parsed::Command(unknown) = text::parse(b"FLOW.QUERY.INDEXES", &[text("nope")])? else {
        return Err("indexes is not a command".into());
    };
    let refused = call(|reply| node.execute(unknown, reply))?;
    assert!(
        matches!(refused, Err(error) if error.to_string().starts_with("ERR query index not found"))
    );
    Ok(())
}

#[test]
fn a_collection_reads_only_its_partition() -> TestResult {
    let path_dir = directory("flow-query-partition")?;
    // One shard holds both partitions, so only the scope keeps them apart.
    let node = open(&path_dir, 1)?;
    for (id, partition) in [("x", "p1"), ("y", "p2"), ("z", "p2")] {
        run(
            &node,
            "FLOW.CREATE",
            vec![
                ("id", text(id)),
                ("type", text("ts")),
                ("partition_key", text(partition)),
                ("now_ms", Term::Int(1_000)),
            ],
        )??;
    }
    for (partition, expected) in [("p1", vec!["x"]), ("p2", vec!["y", "z"])] {
        let result = query(
            &node,
            "FROM runs WHERE partition_key = @p AND type = 'ts' ORDER BY created_at_ms ASC LIMIT 10 RETURN RECORDS",
            vec![("p", text(partition))],
        )??;
        let mut found = ids(&result);
        found.sort();
        assert_eq!(found, expected);
    }
    Ok(())
}
