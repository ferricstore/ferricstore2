//! `flow_test/sections/flow_create_stores_debug_lineage_metadata_indexes_
//! it.exs`: a Flow's parent, root and correlation ids are kept, shown in
//! its history and found by lineage queries (Elixir's `flow_by_parent`,
//! `flow_by_root` and `flow_by_correlation`, as the SDK's `FLOW.QUERY`
//! text); batch writes answer `OK` and refuse other return modes; an
//! independent batch without partitions gives each Flow its own auto
//! partition, claimable by those keys. (Its index tables, cold scans and
//! `before_id` cursors are Elixir's; the SDK's text has none of them.)

use wf_application::commands::flow::Term;
use wf_node::composition::QueueNode;

use super::{claimed, events, field, get, int, map, ok, refusal, run, text};
use crate::{TestResult, directory, open};

const P: &str = "tenant-lineage";

fn in_p() -> [(&'static str, Term); 1] {
    [("partition_key", text(P))]
}

/// Creates `id` in `P` at `now` with `extra` options.
fn create(node: &QueueNode, id: &str, now: i64, extra: &[(&str, Term)]) -> TestResult {
    let mut entries = vec![
        ("id", text(id)),
        ("type", text("lineage")),
        ("partition_key", text(P)),
        ("run_at_ms", int(now)),
        ("now_ms", int(now)),
    ];
    entries.extend(extra.iter().cloned());
    ok(node, "FLOW.CREATE", &entries)?;
    Ok(())
}

/// A lineage query's options, as the SDK's builder reads them.
#[derive(Default)]
struct Window {
    state: Option<&'static str>,
    from_ms: Option<i64>,
    to_ms: Option<i64>,
    rev: bool,
    count: i64,
}

/// The ids the SDK's lineage query text finds for `selector` = `id` in
/// `P`.
fn lineage(node: &QueueNode, selector: &str, id: &str, window: &Window) -> TestResult<Vec<Term>> {
    let mut predicates = vec![
        "partition_key = @partition_key".to_owned(),
        format!("{selector} = @lineage_id"),
    ];
    let mut params = vec![("partition_key", text(P)), ("lineage_id", text(id))];
    if let Some(state) = window.state {
        predicates.push("state = @state".to_owned());
        params.push(("state", text(state)));
    }
    if window.from_ms.is_some() || window.to_ms.is_some() {
        predicates.push("updated_at_ms BETWEEN @from_ms AND @to_ms".to_owned());
        params.push(("from_ms", int(window.from_ms.unwrap_or(0))));
        params.push(("to_ms", int(window.to_ms.unwrap_or(9_007_199_254_740_991))));
    }
    let direction = if window.rev { "DESC" } else { "ASC" };
    let count = if window.count == 0 { 10 } else { window.count };
    let query = format!(
        "FROM runs WHERE {} ORDER BY updated_at_ms {direction} LIMIT {count} RETURN RECORDS",
        predicates.join(" AND ")
    );
    let result = ok(
        node,
        "FLOW.QUERY",
        &[
            ("version", text("FQL1")),
            ("query", text(&query)),
            ("params", map(&params)),
        ],
    )?;
    let Term::List(records) = field(&result, "records") else {
        return Err(format!("no records in {result:?}").into());
    };
    Ok(records.iter().map(|record| field(record, "id")).collect())
}

fn ids(names: &[&str]) -> Vec<Term> {
    names.iter().map(|name| text(name)).collect()
}

#[test]
fn a_flow_keeps_its_lineage_and_lineage_queries_find_it() -> TestResult {
    // Elixir: "flow_create stores debug lineage metadata and indexes it"
    // and "flow_create stores default root without indexing one unique
    // root per flow".
    let node = open(&directory("audit-lineage-stored")?, 4)?;
    create(
        &node,
        "child",
        1_000,
        &[
            ("parent_flow_id", text("parent")),
            ("root_flow_id", text("root")),
            ("correlation_id", text("order")),
        ],
    )?;
    let record = get(&node, "child", &in_p())?;
    let lineage_of = |term: &Term| {
        (
            field(term, "parent_flow_id"),
            field(term, "root_flow_id"),
            field(term, "correlation_id"),
        )
    };
    let expected = (text("parent"), text("root"), text("order"));
    assert_eq!(lineage_of(&record), expected);
    let found = Window::default();
    for (selector, id) in [
        ("parent_flow_id", "parent"),
        ("root_flow_id", "root"),
        ("correlation_id", "order"),
    ] {
        assert_eq!(
            lineage(&node, selector, id, &found)?,
            ids(&["child"]),
            "{selector}"
        );
    }
    let history = events(&node, "child", &in_p())?;
    let [created] = history.as_slice() else {
        return Err(format!("one event expected, got {history:?}").into());
    };
    assert_eq!(lineage_of(created), expected);

    create(&node, "alone", 1_000, &[])?;
    assert_eq!(
        field(&get(&node, "alone", &in_p())?, "root_flow_id"),
        text("alone")
    );
    Ok(())
}

#[test]
fn lineage_queries_take_windows_states_order_and_counts() -> TestResult {
    // Elixir: "flow_by_parent root and correlation query lineage indexes"
    // (its terminal-only query as a state; the SDK refuses terminal_only
    // with a lineage query).
    let node = open(&directory("audit-lineage-queries")?, 4)?;
    let family = |parent: Option<&str>| {
        let mut entries = vec![("correlation_id", text("order"))];
        if let Some(parent) = parent {
            entries.push(("parent_flow_id", text(parent)));
            entries.push(("root_flow_id", text("root")));
        }
        entries
    };
    create(&node, "root", 1_000, &family(None))?;
    create(&node, "child-a", 2_000, &family(Some("root")))?;
    create(&node, "child-b", 3_000, &family(Some("root")))?;
    create(&node, "grandchild", 4_000, &family(Some("child-a")))?;
    let window = |from_ms, to_ms, rev, count| Window {
        from_ms,
        to_ms,
        rev,
        count,
        ..Window::default()
    };
    let by_parent = |window: &Window| lineage(&node, "parent_flow_id", "root", window);
    assert_eq!(by_parent(&Window::default())?, ids(&["child-a", "child-b"]));
    assert_eq!(
        by_parent(&window(Some(1_500), Some(3_500), true, 10))?,
        ids(&["child-b", "child-a"])
    );
    assert_eq!(by_parent(&window(None, None, true, 1))?, ids(&["child-b"]));
    assert_eq!(
        lineage(&node, "root_flow_id", "root", &Window::default())?,
        ids(&["root", "child-a", "child-b", "grandchild"])
    );
    let queued = Window {
        state: Some("queued"),
        ..window(Some(2_500), Some(3_500), false, 10)
    };
    assert_eq!(
        lineage(&node, "root_flow_id", "root", &queued)?,
        ids(&["child-b"])
    );
    assert_eq!(
        lineage(
            &node,
            "correlation_id",
            "order",
            &window(None, None, false, 2)
        )?,
        ids(&["root", "child-a"])
    );
    let fence = field(&get(&node, "child-a", &in_p())?, "fencing_token");
    ok(
        &node,
        "FLOW.CANCEL",
        &[
            ("id", text("child-a")),
            ("partition_key", text(P)),
            ("fencing_token", fence),
            ("now_ms", int(5_000)),
        ],
    )?;
    let cancelled = Window {
        state: Some("cancelled"),
        ..Window::default()
    };
    assert_eq!(
        lineage(&node, "correlation_id", "order", &cancelled)?,
        ids(&["child-a"])
    );
    Ok(())
}

#[test]
fn batch_writes_answer_ok() -> TestResult {
    // Elixir: "flow write APIs return ok and reject return option", its
    // writes.
    let node = open(&directory("audit-lineage-return")?, 4)?;
    let batch = |extra: &[(&str, Term)]| {
        let mut entries = vec![
            ("partition_key", text(P)),
            ("type", text("return-api")),
            ("state", text("queued")),
            ("run_at_ms", int(1_000)),
            ("now_ms", int(900)),
        ];
        entries.extend(extra.iter().cloned());
        run(&node, "FLOW.CREATE_MANY", &entries)
    };
    let created = batch(&[(
        "items",
        Term::List(vec![map(&[
            ("id", text("r")),
            ("payload", text("payload:r")),
        ])]),
    )])??;
    assert_eq!(created, text("OK"));
    let moved = ok(
        &node,
        "FLOW.TRANSITION_MANY",
        &[
            ("partition_key", text(P)),
            ("from_state", text("queued")),
            ("to_state", text("ready")),
            (
                "items",
                Term::List(vec![map(&[("id", text("r")), ("fencing_token", int(0))])]),
            ),
            ("run_at_ms", int(1_000)),
            ("now_ms", int(950)),
        ],
    )?;
    assert_eq!(moved, text("OK"));
    let records = claimed(
        &node,
        &[
            ("type", text("return-api")),
            ("partition_key", text(P)),
            ("state", text("ready")),
            ("worker", text("worker-return")),
            ("limit", int(1)),
            ("now_ms", int(1_000)),
        ],
    )?;
    let [lease] = records.as_slice() else {
        return Err("one claim".into());
    };
    assert_eq!(field(lease, "payload"), text("payload:r"));
    assert!(matches!(field(lease, "lease_token"), Term::Bin(_)));
    assert!(matches!(field(lease, "fencing_token"), Term::Int(_)));
    let done = ok(
        &node,
        "FLOW.COMPLETE_MANY",
        &[
            ("partition_key", text(P)),
            (
                "items",
                Term::List(vec![map(&[
                    ("id", field(lease, "id")),
                    ("lease_token", field(lease, "lease_token")),
                    ("fencing_token", field(lease, "fencing_token")),
                ])]),
            ),
            ("result", text("ok")),
            ("now_ms", int(1_100)),
        ],
    )?;
    assert_eq!(done, text("OK"));
    Ok(())
}

#[test]
fn an_independent_batch_without_partitions_spreads_and_stays_claimable() -> TestResult {
    // Elixir: "flow_create_many independent auto-partitions without
    // client-side grouping" and "... auto-partitioned flows are claimable
    // by partition keys".
    let node = open(&directory("audit-lineage-auto")?, 4)?;
    let at = [("run_at_ms", int(1_000)), ("now_ms", int(1_000))];
    ok(
        &node,
        "FLOW.CREATE",
        &[
            &[("id", text("existing")), ("type", text("auto"))][..],
            &at[..],
        ]
        .concat(),
    )?;
    let names: Vec<String> = (0..128).map(|index| format!("auto-{index}")).collect();
    let mut items = vec![text("existing")];
    items.extend(names.iter().map(|name| text(name)));
    let reply = ok(
        &node,
        "FLOW.CREATE_MANY",
        &[
            &[
                ("type", text("auto")),
                ("items", Term::List(items)),
                ("independent", Term::Bool(true)),
            ][..],
            &at[..],
        ]
        .concat(),
    )?;
    let Term::List(results) = reply else {
        return Err(format!("per-item results expected, got {reply:?}").into());
    };
    let exists = Term::List(vec![text("error"), text("ERR flow already exists")]);
    assert_eq!(results.first(), Some(&exists));
    assert!(results.iter().skip(1).all(|result| *result == text("ok")));
    let mut partitions = Vec::new();
    for name in &names {
        let record = get(&node, name, &[])?;
        assert_eq!(field(&record, "state"), text("queued"));
        let partition = field(&record, "partition_key");
        assert!(matches!(&partition, Term::Bin(key) if key.starts_with(b"__flow_auto__")));
        if !partitions.contains(&partition) {
            partitions.push(partition);
        }
    }
    let mut taken = Vec::new();
    for keys in partitions.chunks(32) {
        let records = claimed(
            &node,
            &[
                ("type", text("auto")),
                ("partition_keys", Term::List(keys.to_vec())),
                ("state", text("queued")),
                ("worker", text("worker-auto")),
                ("lease_ms", int(30_000)),
                ("limit", int(256)),
                ("now_ms", int(1_000)),
            ],
        )?;
        taken.extend(records.iter().map(|record| field(record, "id")));
    }
    let mut expected: Vec<Term> = names.iter().map(|name| text(name)).collect();
    let key = |id: &Term| match id {
        Term::Bin(bytes) => bytes.clone(),
        _ => Vec::new(),
    };
    expected.sort_by_key(key);
    taken.sort_by_key(key);
    // The existing Flow shares an auto partition with none of the batch's
    // names, or is claimed with them; either way the batch's Flows are all
    // claimed.
    taken.retain(|id| *id != text("existing"));
    assert_eq!(taken, expected);
    Ok(())
}

#[test]
fn batch_commands_refuse_other_return_modes() -> TestResult {
    // Elixir: "flow write APIs return ok and reject return option", its
    // refusal (audit gap G29), for create and mutation batches.
    let node = open(&directory("audit-lineage-return-mode")?, 4)?;
    let batch = |extra: &[(&str, Term)]| {
        let mut entries = vec![
            ("partition_key", text(P)),
            ("type", text("return-api")),
            ("state", text("queued")),
            ("run_at_ms", int(1_000)),
            ("now_ms", int(900)),
        ];
        entries.extend(extra.iter().cloned());
        run(&node, "FLOW.CREATE_MANY", &entries)
    };
    let full = batch(&[
        ("items", Term::List(vec![text("refused")])),
        ("return", text("full")),
    ])?;
    assert_eq!(
        refusal(full)?,
        "ERR flow return must be items or ok_on_success"
    );
    assert_eq!(get(&node, "refused", &in_p())?, Term::Nil);
    let listed = batch(&[
        ("items", Term::List(vec![text("listed")])),
        ("return", text("ITEMS")),
    ])??;
    assert_eq!(listed, text("OK"));
    let cancel = run(
        &node,
        "FLOW.CANCEL_MANY",
        &[
            ("partition_key", text(P)),
            (
                "items",
                Term::List(vec![map(&[
                    ("id", text("listed")),
                    ("fencing_token", int(0)),
                ])]),
            ),
            ("return", text("full")),
        ],
    )?;
    assert_eq!(
        refusal(cancel)?,
        "ERR flow return must be items or ok_on_success"
    );
    assert_eq!(
        field(&get(&node, "listed", &in_p())?, "state"),
        text("queued")
    );
    Ok(())
}
