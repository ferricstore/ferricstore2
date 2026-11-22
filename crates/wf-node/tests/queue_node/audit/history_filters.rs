//! `flow_test/sections/flow_history_supports_range_reverse_event_worker_
//! filters.exs`: history ranges, order, event and worker filters, windows
//! of versions, caps and event ids; and the terminal and failure lists
//! (Elixir's retired `FLOW.TERMINALS`, as the SDK's `FLOW.QUERY` text).
//! Elixir's hot and cold history tiers are its storage: the node keeps one
//! history per Flow, capped by `history_max_events`.

use wf_application::commands::flow::Term;
use wf_node::composition::QueueNode;

use super::{claimed, field, get, int, map, ok, refusal, run, text};
use crate::{TestResult, directory, open};

const P: &str = "tenant-history";

fn in_p<'a>(entries: &[(&'a str, Term)]) -> Vec<(&'a str, Term)> {
    let mut all = vec![("partition_key", text(P))];
    all.extend(entries.iter().cloned());
    all
}

/// `(event id, fields)` of `id`'s history with `options`.
fn history(node: &QueueNode, id: &str, options: &[(&str, Term)]) -> TestResult<Vec<(Term, Term)>> {
    let mut entries = in_p(&[("id", text(id))]);
    entries.extend(options.iter().cloned());
    let Term::List(events) = ok(node, "FLOW.HISTORY", &entries)? else {
        return Err("history is not a list".into());
    };
    Ok(events
        .into_iter()
        .filter_map(|event| match event {
            Term::List(mut parts) if parts.len() == 2 => {
                let fields = parts.pop()?;
                Some((parts.pop()?, fields))
            }
            _ => None,
        })
        .collect())
}

fn names(events: &[(Term, Term)]) -> Vec<Term> {
    events
        .iter()
        .map(|(_, fields)| field(fields, "event"))
        .collect()
}

fn versions(events: &[(Term, Term)]) -> Vec<Term> {
    events
        .iter()
        .map(|(_, fields)| field(fields, "version"))
        .collect()
}

fn claim_one(
    node: &QueueNode,
    flow_type: &str,
    state: &str,
    worker: &str,
    now: i64,
) -> TestResult<Term> {
    let records = claimed(
        node,
        &in_p(&[
            ("type", text(flow_type)),
            ("state", text(state)),
            ("worker", text(worker)),
            ("lease_ms", int(30_000)),
            ("limit", int(1)),
            ("now_ms", int(now)),
        ]),
    )?;
    let [record] = records.as_slice() else {
        return Err(format!("one claim expected, got {records:?}").into());
    };
    Ok(record.clone())
}

fn lease(record: &Term) -> Vec<(&'static str, Term)> {
    vec![
        ("id", field(record, "id")),
        ("partition_key", text(P)),
        ("lease_token", field(record, "lease_token")),
        ("fencing_token", field(record, "fencing_token")),
    ]
}

fn create(node: &QueueNode, id: &str, flow_type: &str, extra: &[(&str, Term)]) -> TestResult {
    let mut entries = in_p(&[
        ("id", text(id)),
        ("type", text(flow_type)),
        ("state", text("queued")),
    ]);
    entries.retain(|(key, _)| !extra.iter().any(|(given, _)| given == key));
    entries.extend(extra.iter().cloned());
    ok(node, "FLOW.CREATE", &entries)?;
    Ok(())
}

#[test]
fn history_filters_by_event_range_time_order_event_worker_and_version() -> TestResult {
    // Elixir: "flow_history supports range reverse event and worker
    // filters".
    let node = open(&directory("audit-hist-filters")?, 4)?;
    create(
        &node,
        "h",
        "query",
        &[("run_at_ms", int(1_000)), ("now_ms", int(1_000))],
    )?;
    let first = claim_one(&node, "query", "queued", "worker-a", 1_100)?;
    let mut moved = lease(&first);
    moved.extend([
        ("from_state", text("running")),
        ("to_state", text("email")),
        ("run_at_ms", int(1_200)),
        ("now_ms", int(1_200)),
    ]);
    ok(&node, "FLOW.TRANSITION", &moved)?;
    let second = claim_one(&node, "query", "email", "worker-b", 1_250)?;
    let mut failed = lease(&second);
    failed.extend([("error", text("boom")), ("now_ms", int(1_300))]);
    ok(&node, "FLOW.FAIL", &failed)?;
    let all = history(&node, "h", &[("count", int(10))])?;
    assert_eq!(
        names(&all),
        [
            text("created"),
            text("claimed"),
            text("transitioned"),
            text("claimed"),
            text("failed")
        ]
    );
    let id_at = |index: usize| all.get(index).map_or(Term::Nil, |(id, _)| id.clone());
    let range = history(
        &node,
        "h",
        &[
            ("from_event", id_at(1)),
            ("to_event", id_at(3)),
            ("count", int(10)),
        ],
    )?;
    assert_eq!(
        names(&range),
        [text("claimed"), text("transitioned"), text("claimed")]
    );
    let reverse = history(
        &node,
        "h",
        &[
            ("from_ms", int(1_100)),
            ("to_ms", int(1_300)),
            ("rev", Term::Bool(true)),
            ("count", int(2)),
        ],
    )?;
    assert_eq!(names(&reverse), [text("failed"), text("claimed")]);
    let worker = history(
        &node,
        "h",
        &[
            ("event", text("claimed")),
            ("worker", text("worker-b")),
            ("count", int(10)),
        ],
    )?;
    let [(_, only)] = worker.as_slice() else {
        return Err(format!("one event expected, got {worker:?}").into());
    };
    assert_eq!(
        (field(only, "event"), field(only, "lease_owner")),
        (text("claimed"), text("worker-b"))
    );
    let window = history(
        &node,
        "h",
        &[
            ("from_version", int(2)),
            ("to_version", int(4)),
            ("count", int(10)),
        ],
    )?;
    assert_eq!(versions(&window), [text("2"), text("3"), text("4")]);
    Ok(())
}

#[test]
fn version_windows_return_their_events_and_the_latest_page() -> TestResult {
    // Elixir: "flow_history filtered cold query finds recent tail events
    // beyond oldest scan window", "... returns latest matching page", "...
    // keeps old head window before filtering" and "... finds retained middle
    // events beyond sampled edges" (scan windows are Elixir's storage).
    let node = open(&directory("audit-hist-windows")?, 4)?;
    let caps = [
        ("history_max_events", int(200)),
        ("run_at_ms", int(1_000)),
        ("now_ms", int(1_000)),
    ];
    create(
        &node,
        "w",
        "windows",
        &[&[("state", text("s0"))][..], &caps[..]].concat(),
    )?;
    for step in 1..=30 {
        let entries = in_p(&[
            ("id", text("w")),
            ("from_state", text(&format!("s{}", step - 1))),
            ("to_state", text(&format!("s{step}"))),
            ("fencing_token", int(0)),
            ("run_at_ms", int(1_000 + step)),
            ("now_ms", int(1_000 + step)),
        ]);
        ok(&node, "FLOW.TRANSITION", &entries)?;
    }
    assert_eq!(field(&get(&node, "w", &in_p(&[]))?, "version"), int(31));
    let as_text = |range: std::ops::RangeInclusive<i64>| {
        range
            .map(|version| text(&version.to_string()))
            .collect::<Vec<_>>()
    };
    let tail = history(
        &node,
        "w",
        &[
            ("from_version", int(20)),
            ("to_version", int(25)),
            ("count", int(10)),
        ],
    )?;
    assert_eq!(versions(&tail), as_text(20..=25));
    let latest = history(
        &node,
        "w",
        &[
            ("from_version", int(20)),
            ("to_version", int(25)),
            ("count", int(3)),
        ],
    )?;
    assert_eq!(versions(&latest), as_text(23..=25));
    let head = history(&node, "w", &[("to_version", int(5)), ("count", int(10))])?;
    assert_eq!(versions(&head), as_text(1..=5));
    let middle = history(
        &node,
        "w",
        &[
            ("from_version", int(16)),
            ("to_version", int(16)),
            ("count", int(1)),
        ],
    )?;
    assert_eq!(versions(&middle), as_text(16..=16));
    Ok(())
}

#[test]
fn history_keeps_its_cap_and_its_event_ids_rise() -> TestResult {
    // Elixir: "flow history max events hard-caps stored history records",
    // "flow history max events defaults to 100k and rejects invalid caps",
    // "flow history configured maximum cannot exceed hard cap", "flow
    // history hot max accepts zero" and "flow_history event ids stay
    // monotonic when claim time is behind record time".
    let node = open(&directory("audit-hist-cap")?, 4)?;
    let capped = [
        ("history_max_events", int(5)),
        ("run_at_ms", int(1_000)),
        ("now_ms", int(1_000)),
    ];
    create(&node, "cap", "hard-cap", &capped)?;
    let created = history(&node, "cap", &[("count", int(10))])?;
    let created_id = created
        .first()
        .map(|(id, _)| id.clone())
        .ok_or("no created event")?;
    let leased = claim_one(&node, "hard-cap", "queued", "worker-a", 1_010)?;
    for index in 1..=5 {
        let mut extend = lease(&leased);
        extend.extend([("lease_ms", int(30_000)), ("now_ms", int(1_020 + index))]);
        ok(&node, "FLOW.EXTEND_LEASE", &extend)?;
    }
    let mut done = lease(&leased);
    done.push(("now_ms", int(1_100)));
    ok(&node, "FLOW.COMPLETE", &done)?;
    let events = history(&node, "cap", &[("count", int(10))])?;
    assert_eq!(events.len(), 5);
    assert!(events.iter().all(|(id, _)| *id != created_id));
    let extended = text("lease_extended");
    assert_eq!(
        names(&events),
        [
            extended.clone(),
            extended.clone(),
            extended.clone(),
            extended,
            text("completed")
        ]
    );

    create(
        &node,
        "default",
        "default-cap",
        &[("run_at_ms", int(1_000))],
    )?;
    assert_eq!(
        field(&get(&node, "default", &in_p(&[]))?, "history_max_events"),
        int(100_000)
    );
    let bad = |extra: &[(&str, Term)]| {
        let mut entries = in_p(&[("id", text("bad")), ("type", text("bad-cap"))]);
        entries.extend(extra.iter().cloned());
        run(&node, "FLOW.CREATE", &entries)
    };
    assert_eq!(
        refusal(bad(&[
            ("history_hot_max_events", int(10)),
            ("history_max_events", int(5))
        ])?)?,
        "ERR flow history_max_events must be greater than or equal to history_hot_max_events"
    );
    assert_eq!(
        refusal(bad(&[("history_max_events", int(2_000_000))])?)?,
        "ERR flow history_max_events exceeds maximum 1000000"
    );
    let zero = [
        ("history_hot_max_events", int(0)),
        ("history_max_events", int(5)),
        ("run_at_ms", int(1_000)),
    ];
    create(&node, "zero", "hot-zero", &zero)?;
    assert_eq!(
        field(&get(&node, "zero", &in_p(&[]))?, "history_hot_max_events"),
        int(0)
    );

    create(
        &node,
        "mono",
        "monotonic",
        &[("run_at_ms", int(1_000)), ("now_ms", int(10_000))],
    )?;
    claim_one(&node, "monotonic", "queued", "worker-a", 2_000)?;
    let events = history(&node, "mono", &[("count", int(10))])?;
    let ids: Vec<Term> = events.iter().map(|(id, _)| id.clone()).collect();
    assert_eq!(ids, [text("10000-1"), text("10000-2")]);
    assert_eq!(names(&events), [text("created"), text("claimed")]);
    Ok(())
}

/// The SDK's terminal or failure query over `states`, in `P`.
fn terminals(
    node: &QueueNode,
    states: &[&str],
    window: Option<(i64, i64)>,
    rev: bool,
    count: i64,
) -> TestResult<Vec<Term>> {
    let placeholders: Vec<String> = (0..states.len())
        .map(|index| format!("@terminal_{index}"))
        .collect();
    let mut fql = format!(
        "FROM runs WHERE partition_key = @partition_key AND type = @type AND state IN ({})",
        placeholders.join(", ")
    );
    let mut params = vec![
        ("partition_key".to_owned(), text(P)),
        ("type".to_owned(), text("terminals")),
    ];
    for (index, state) in states.iter().enumerate() {
        params.push((format!("terminal_{index}"), text(state)));
    }
    if let Some((from, to)) = window {
        fql.push_str(" AND updated_at_ms BETWEEN @from_ms AND @to_ms");
        params.extend([
            ("from_ms".to_owned(), int(from)),
            ("to_ms".to_owned(), int(to)),
        ]);
    }
    let order = if rev { "DESC" } else { "ASC" };
    let tail = [
        " ORDER BY updated_at_ms ",
        order,
        " LIMIT ",
        &count.to_string(),
        " RETURN RECORDS",
    ];
    fql.push_str(&tail.concat());
    let params = Term::Map(
        params
            .into_iter()
            .map(|(key, value)| (key.into_bytes(), value))
            .collect(),
    );
    let result = ok(
        node,
        "FLOW.QUERY",
        &[
            ("version", text("FQL1")),
            ("query", text(&fql)),
            ("params", params),
        ],
    )?;
    let Term::List(records) = field(&result, "records") else {
        return Err(format!("no records in {result:?}").into());
    };
    Ok(records.iter().map(|record| field(record, "id")).collect())
}

#[test]
fn terminal_and_failure_lists_filter_by_state_and_time() -> TestResult {
    // Elixir: "flow_terminals and flow_failures list terminal records by
    // state and time range" and "flow_terminals returns completed failed
    // and cancelled records with the same timestamp".
    let node = open(&directory("audit-hist-terminals")?, 4)?;
    for id in ["fail-a", "fail-b", "done", "cancel"] {
        create(&node, id, "terminals", &[("run_at_ms", int(1_000))])?;
    }
    let mut leases = Vec::new();
    for _ in 0..4 {
        leases.push(claim_one(
            &node,
            "terminals",
            "queued",
            "worker-failures",
            1_000,
        )?);
    }
    for leased in &leases {
        let id = field(leased, "id");
        let (name, now) = match &id {
            Term::Bin(bytes) if bytes == b"fail-a" => ("FLOW.FAIL", 1_500),
            Term::Bin(bytes) if bytes == b"fail-b" => ("FLOW.FAIL", 2_500),
            Term::Bin(bytes) if bytes == b"done" => ("FLOW.COMPLETE", 2_000),
            _ => ("FLOW.CANCEL", 1_800),
        };
        let mut entries = lease(leased);
        entries.push(("now_ms", int(now)));
        ok(&node, name, &entries)?;
    }
    assert_eq!(
        terminals(&node, &["failed"], Some((1_000, 2_000)), false, 10)?,
        [text("fail-a")]
    );
    assert_eq!(
        terminals(&node, &["completed"], None, false, 10)?,
        [text("done")]
    );
    assert_eq!(
        terminals(&node, &["cancelled"], None, false, 10)?,
        [text("cancel")]
    );
    let all = ["completed", "failed", "cancelled"];
    assert_eq!(
        terminals(&node, &all, Some((1_400, 2_100)), false, 10)?,
        [text("fail-a"), text("cancel"), text("done")]
    );
    assert_eq!(
        terminals(&node, &all, Some((1_400, 2_100)), true, 2)?,
        [text("done"), text("cancel")]
    );
    // Three Flows finished at one time all show.
    for id in ["same-done", "same-failed", "same-cancelled"] {
        create(&node, id, "terminals", &[("run_at_ms", int(3_000))])?;
    }
    for _ in 0..3 {
        let finished = claim_one(&node, "terminals", "queued", "worker-same", 3_000)?;
        let name = match field(&finished, "id") {
            Term::Bin(bytes) if bytes == b"same-done" => "FLOW.COMPLETE",
            Term::Bin(bytes) if bytes == b"same-failed" => "FLOW.FAIL",
            _ => "FLOW.CANCEL",
        };
        let mut entries = lease(&finished);
        entries.push(("now_ms", int(3_100)));
        ok(&node, name, &entries)?;
    }
    let same = terminals(&node, &all, Some((3_100, 3_100)), false, 20)?;
    assert_eq!(same.len(), 3, "{same:?}");
    let _ = map;
    Ok(())
}
