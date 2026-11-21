//! `flow_value_payload_test.exs` (Elixir `main` 6a5f2bc34); the tests that
//! need structured values, batch item fields or history values wait for
//! those features. Not ported:
//! the two "does not materialize existing generated payload value refs"
//! tests (Elixir copies no generated ref at terminal time; their MGET after
//! cleanup reads a value Elixir's cleanup left, which the cleanup tests
//! below contradict) and the retention sweeper test (covered by
//! `retention::a_completed_flow_is_removed_after_retention`).

use wf_application::commands::flow::Term;
use wf_node::composition::QueueNode;

use super::{claimed, events, field, get, has, int, map, ok, run, text};
use crate::{TestResult, directory, open};

const P: &str = "tenant-a";

fn node(tag: &str) -> TestResult<QueueNode> {
    open(&directory(tag)?, 4)
}

fn with<'a>(partition: &str, entries: &[(&'a str, Term)]) -> Vec<(&'a str, Term)> {
    let mut all = vec![("partition_key", text(partition))];
    all.extend(entries.iter().cloned());
    all
}

fn create(node: &QueueNode, id: &str, flow_type: &str, extra: &[(&str, Term)]) -> TestResult {
    let mut entries = vec![("id", text(id)), ("type", text(flow_type))];
    entries.extend(extra.iter().cloned());
    ok(node, "FLOW.CREATE", &entries)?;
    Ok(())
}

fn claim_all(node: &QueueNode, partition: &str, extra: &[(&str, Term)]) -> TestResult<Vec<Term>> {
    let mut entries = with(
        partition,
        &[
            ("worker", text("worker-1")),
            ("limit", int(10)),
            ("now_ms", int(1_000)),
        ],
    );
    // An extra option replaces the default of the same name.
    for (key, value) in extra {
        entries.retain(|(existing, _)| existing != key);
        entries.push((key, value.clone()));
    }
    claimed(node, &entries)
}

fn lease(record: &Term) -> [(&'static str, Term); 2] {
    [
        ("lease_token", field(record, "lease_token")),
        ("fencing_token", field(record, "fencing_token")),
    ]
}

fn full(node: &QueueNode, id: &str, partition: &str) -> TestResult<Term> {
    get(node, id, &with(partition, &[("full", Term::Bool(true))]))
}

fn mget(node: &QueueNode, refs: Vec<Term>, extra: &[(&str, Term)]) -> TestResult<Term> {
    let mut entries = vec![("refs", Term::List(refs))];
    entries.extend(extra.iter().cloned());
    ok(node, "FLOW.VALUE.MGET", &entries)
}

fn order() -> Term {
    map(&[
        ("order_id", int(123)),
        ("items", Term::List(vec![text("book"), text("pen")])),
    ])
}

#[test]
fn create_stores_a_full_payload_value_and_claim_get_hydrate_it_from_internal_storage() -> TestResult
{
    let node = node("audit-payload-create")?;
    create(
        &node,
        "vc",
        "value-payload",
        &with(
            P,
            &[
                ("payload", order()),
                ("run_at_ms", int(1_000)),
                ("now_ms", int(1_000)),
            ],
        ),
    )?;
    let created = get(&node, "vc", &with(P, &[]))?;
    assert!(matches!(field(&created, "payload_ref"), Term::Bin(ref bytes) if !bytes.is_empty()));
    assert!(!has(&created, "payload"));
    assert_eq!(field(&full(&node, "vc", P)?, "payload"), order());
    let records = claim_all(
        &node,
        P,
        &[("type", text("value-payload")), ("limit", int(1))],
    )?;
    assert_eq!(records.len(), 1);
    assert_eq!(
        field(records.first().unwrap_or(&Term::Nil), "payload"),
        order()
    );
    Ok(())
}

#[test]
fn shared_value_ttl_uses_command_time_when_now_ms_is_omitted() -> TestResult {
    let node = node("audit-payload-ttl")?;
    let put = ok(
        &node,
        "FLOW.VALUE.PUT",
        &with(P, &[("value", text("expires")), ("ttl_ms", int(60_000))]),
    )?;
    assert_eq!(
        mget(&node, vec![field(&put, "ref")], &[])?,
        Term::List(vec![text("expires")])
    );
    Ok(())
}

#[test]
fn shared_value_expiry_stays_in_the_exact_integer_domain() -> TestResult {
    let node = node("audit-payload-exact")?;
    let max_exact = 9_007_199_254_740_991;
    let put = |entries: &[(&str, Term)]| {
        let mut all = vec![("value", text("invalid"))];
        all.extend(entries.iter().cloned());
        run(&node, "FLOW.VALUE.PUT", &all)
    };
    // Elixir's wording differs; what matters is that each is refused. Its
    // third case (an expiry one past 2^53 - 1) is no limit here: times are
    // u64, so that value is stored and expires normally.
    for entries in [
        vec![("now_ms", int(max_exact + 1))],
        vec![("ttl_ms", int(max_exact + 1))],
    ] {
        assert!(put(&entries)?.is_err(), "{entries:?}");
    }
    Ok(())
}

#[test]
fn value_mget_returns_hot_values_and_leaves_ordinary_missing_refs_as_nil() -> TestResult {
    let node = node("audit-payload-mget-missing")?;
    let put = ok(
        &node,
        "FLOW.VALUE.PUT",
        &with(P, &[("value", text("hot-value"))]),
    )?;
    assert_eq!(
        mget(
            &node,
            vec![field(&put, "ref"), text("ordinary-missing-flow-value-ref")],
            &[]
        )?,
        Term::List(vec![text("hot-value"), Term::Nil])
    );
    Ok(())
}

/// Not an Elixir test: audit gaps G6 and G7 with byte payloads. Each event
/// shows the payload, result and error versions it named, the replaced
/// ones included, also once the Flow is on disk (ADR-016 phase C).
#[test]
fn history_values_show_each_events_own_versions() -> TestResult {
    let node = node("audit-payload-history-versions")?;
    create(
        &node,
        "vv",
        "value-versions",
        &with(
            P,
            &[
                ("payload", text("p1")),
                ("run_at_ms", int(1_000)),
                ("now_ms", int(1_000)),
            ],
        ),
    )?;
    let first = claim_all(&node, P, &[("type", text("value-versions"))])?;
    let mut moved = with(
        P,
        &[
            ("id", text("vv")),
            ("from_state", text("running")),
            ("to_state", text("waiting")),
            ("payload", text("p2")),
            ("run_at_ms", int(1_000)),
            ("now_ms", int(1_100)),
        ],
    );
    moved.extend(lease(first.first().unwrap_or(&Term::Nil)));
    ok(&node, "FLOW.TRANSITION", &moved)?;
    let second = claim_all(
        &node,
        P,
        &[("type", text("value-versions")), ("state", text("waiting"))],
    )?;
    let mut done = with(
        P,
        &[
            ("id", text("vv")),
            ("result", text("r1")),
            ("now_ms", int(1_200)),
        ],
    );
    done.extend(lease(second.first().unwrap_or(&Term::Nil)));
    ok(&node, "FLOW.COMPLETE", &done)?;
    // A later round, so the finished Flow is read back from disk.
    let hydrated = super::events(
        &node,
        "vv",
        &with(P, &[("count", int(10)), ("values", Term::Bool(true))]),
    )?;
    let shown: Vec<(Term, Term, Term)> = hydrated
        .iter()
        .map(|fields| {
            (
                field(fields, "event"),
                field(fields, "payload"),
                field(fields, "result"),
            )
        })
        .collect();
    assert_eq!(
        shown,
        vec![
            (text("created"), text("p1"), Term::Nil),
            (text("claimed"), text("p1"), Term::Nil),
            (text("transitioned"), text("p2"), Term::Nil),
            (text("claimed"), text("p2"), Term::Nil),
            (text("completed"), text("p2"), text("r1")),
        ]
    );
    Ok(())
}

#[test]
fn transition_can_replace_payload_and_retry_error_preserves_current_payload() -> TestResult {
    let node = node("audit-payload-transition")?;
    create(
        &node,
        "vt",
        "value-transition",
        &with(
            P,
            &[
                ("payload", text("initial")),
                ("run_at_ms", int(1_000)),
                ("now_ms", int(1_000)),
            ],
        ),
    )?;
    let first = claim_all(&node, P, &[("type", text("value-transition"))])?;
    let first = first.first().cloned().unwrap_or(Term::Nil);
    let step = map(&[("step", text("waiting"))]);
    let mut entries = with(
        P,
        &[
            ("id", text("vt")),
            ("from_state", text("running")),
            ("to_state", text("waiting")),
            ("payload", step.clone()),
            ("run_at_ms", int(2_000)),
            ("now_ms", int(1_100)),
        ],
    );
    entries.extend(lease(&first));
    ok(&node, "FLOW.TRANSITION", &entries)?;
    let moved = get(&node, "vt", &with(P, &[]))?;
    assert!(matches!(field(&moved, "payload_ref"), Term::Bin(_)));
    assert_ne!(field(&moved, "payload_ref"), field(&first, "payload_ref"));
    let again = claim_all(
        &node,
        P,
        &[
            ("type", text("value-transition")),
            ("state", text("waiting")),
            ("worker", text("worker-2")),
            ("now_ms", int(2_000)),
        ],
    )?;
    let again = again.first().cloned().unwrap_or(Term::Nil);
    assert_eq!(field(&again, "payload"), step);
    let reason = map(&[("reason", text("temporary"))]);
    let mut retry = with(
        P,
        &[
            ("id", text("vt")),
            ("error", reason.clone()),
            ("run_at_ms", int(3_000)),
            ("now_ms", int(2_100)),
        ],
    );
    retry.extend(lease(&again));
    ok(&node, "FLOW.RETRY", &retry)?;
    let retried = get(&node, "vt", &with(P, &[]))?;
    assert_eq!(field(&retried, "payload_ref"), field(&again, "payload_ref"));
    assert!(matches!(field(&retried, "error_ref"), Term::Bin(_)));
    assert!(!has(&retried, "payload") && !has(&retried, "error"));
    let fetched = full(&node, "vt", P)?;
    assert_eq!(field(&fetched, "payload"), step);
    assert_eq!(field(&fetched, "error"), reason);
    Ok(())
}

#[test]
fn complete_and_fail_store_result_error_values_without_requiring_public_refs() -> TestResult {
    let node = node("audit-payload-terminal")?;
    for id in ["vk", "vf"] {
        create(
            &node,
            id,
            "value-terminal",
            &with(P, &[("run_at_ms", int(1_000)), ("now_ms", int(1_000))]),
        )?;
    }
    let records = claim_all(
        &node,
        P,
        &[("type", text("value-terminal")), ("limit", int(2))],
    )?;
    let find = |id: &str| {
        records
            .iter()
            .find(|record| field(record, "id") == text(id))
            .cloned()
            .unwrap_or(Term::Nil)
    };
    let sent = map(&[("status", text("sent"))]);
    let bad = map(&[("code", text("bad_input"))]);
    let mut complete = with(
        P,
        &[
            ("id", text("vk")),
            ("result", sent.clone()),
            ("now_ms", int(1_100)),
        ],
    );
    complete.extend(lease(&find("vk")));
    ok(&node, "FLOW.COMPLETE", &complete)?;
    let mut fail = with(
        P,
        &[
            ("id", text("vf")),
            ("error", bad.clone()),
            ("now_ms", int(1_100)),
        ],
    );
    fail.extend(lease(&find("vf")));
    ok(&node, "FLOW.FAIL", &fail)?;
    assert!(matches!(
        field(&get(&node, "vk", &with(P, &[]))?, "result_ref"),
        Term::Bin(_)
    ));
    assert!(matches!(
        field(&get(&node, "vf", &with(P, &[]))?, "error_ref"),
        Term::Bin(_)
    ));
    assert_eq!(field(&full(&node, "vk", P)?, "result"), sent);
    assert_eq!(field(&full(&node, "vf", P)?, "error"), bad);
    Ok(())
}

#[test]
fn value_mget_can_omit_values_larger_than_max_bytes_without_decoding_them() -> TestResult {
    let node = node("audit-payload-mget-omit")?;
    let payload = map(&[("large", text(&"x".repeat(512)))]);
    let put = ok(
        &node,
        "FLOW.VALUE.PUT",
        &with(P, &[("value", payload.clone())]),
    )?;
    let reference = field(&put, "ref");
    let Term::List(omitted) = mget(&node, vec![reference.clone()], &[("max_bytes", int(1))])?
    else {
        return Err("mget is not a list".into());
    };
    let omitted = omitted.first().cloned().unwrap_or(Term::Nil);
    assert_eq!(field(&omitted, "ref"), reference);
    assert_eq!(field(&omitted, "value_omitted"), Term::Bool(true));
    assert!(matches!(field(&omitted, "value_size"), Term::Int(size) if size > 1));
    for key in ["max_bytes", "value_max_bytes", "payload_max_bytes"] {
        assert_eq!(
            mget(&node, vec![reference.clone()], &[(key, int(64 * 1024))])?,
            Term::List(vec![payload.clone()]),
            "{key}"
        );
    }
    Ok(())
}

#[test]
fn history_only_hydrates_stored_values_when_values_option_is_requested() -> TestResult {
    let node = node("audit-payload-history")?;
    create(
        &node,
        "vh",
        "value-history",
        &with(
            P,
            &[
                ("payload", map(&[("input", int(1))])),
                ("run_at_ms", int(1_000)),
                ("now_ms", int(1_000)),
            ],
        ),
    )?;
    let records = claim_all(&node, P, &[("type", text("value-history"))])?;
    let mut complete = with(
        P,
        &[
            ("id", text("vh")),
            ("result", map(&[("output", int(2))])),
            ("now_ms", int(1_100)),
        ],
    );
    complete.extend(lease(records.first().unwrap_or(&Term::Nil)));
    ok(&node, "FLOW.COMPLETE", &complete)?;
    let plain = events(&node, "vh", &with(P, &[("count", int(10))]))?;
    assert!(
        plain
            .iter()
            .all(|fields| !has(fields, "payload") && !has(fields, "result"))
    );
    let hydrated = events(
        &node,
        "vh",
        &with(P, &[("count", int(10)), ("values", Term::Bool(true))]),
    )?;
    let by_event = |name: &str| {
        hydrated
            .iter()
            .find(|fields| field(fields, "event") == text(name))
            .cloned()
            .unwrap_or(Term::Nil)
    };
    let input = map(&[("input", int(1))]);
    assert_eq!(field(&by_event("created"), "payload"), input);
    assert_eq!(field(&by_event("claimed"), "payload"), input);
    assert_eq!(field(&by_event("completed"), "payload"), input);
    assert_eq!(
        field(&by_event("completed"), "result"),
        map(&[("output", int(2))])
    );
    Ok(())
}

fn batch_item(record: &Term, extra: &[(&str, Term)]) -> Term {
    let mut entries = vec![("id", field(record, "id"))];
    entries.extend(lease(record));
    entries.extend(extra.iter().cloned());
    map(&entries)
}

#[test]
fn batch_apis_also_persist_full_value_fields() -> TestResult {
    let node = node("audit-payload-batch")?;
    let partition = "tenant-b";
    let kind = |value: &str| map(&[("kind", text(value))]);
    ok(
        &node,
        "FLOW.CREATE_MANY",
        &with(
            partition,
            &[
                (
                    "items",
                    Term::List(vec![
                        map(&[("id", text("bc")), ("payload", kind("complete"))]),
                        map(&[("id", text("br")), ("payload", kind("retry"))]),
                        map(&[("id", text("bf")), ("payload", kind("fail"))]),
                    ]),
                ),
                ("type", text("value-batch")),
                ("run_at_ms", int(1_000)),
                ("now_ms", int(1_000)),
            ],
        ),
    )?;
    for id in ["bc", "br", "bf"] {
        let created = get(&node, id, &with(partition, &[]))?;
        assert!(matches!(field(&created, "payload_ref"), Term::Bin(_)));
    }
    let records = claim_all(
        &node,
        partition,
        &[("type", text("value-batch")), ("limit", int(3))],
    )?;
    let find = |id: &str| {
        records
            .iter()
            .find(|record| field(record, "id") == text(id))
            .cloned()
            .unwrap_or(Term::Nil)
    };
    let many = |name: &str, item: Term, extra: &[(&str, Term)]| {
        let mut entries = with(
            partition,
            &[("items", Term::List(vec![item])), ("now_ms", int(1_100))],
        );
        entries.extend(extra.iter().cloned());
        ok(&node, name, &entries)
    };
    many(
        "FLOW.COMPLETE_MANY",
        batch_item(&find("bc"), &[("result", Term::List(vec![text("done")]))]),
        &[],
    )?;
    let retry_error = map(&[("retry", Term::Bool(true))]);
    many(
        "FLOW.RETRY_MANY",
        batch_item(
            &find("br"),
            &[
                ("error", retry_error.clone()),
                ("payload", kind("retry-updated")),
            ],
        ),
        &[("run_at_ms", int(2_000))],
    )?;
    let fail_error = Term::List(vec![text("bad"), text("input")]);
    many(
        "FLOW.FAIL_MANY",
        batch_item(&find("bf"), &[("error", fail_error.clone())]),
        &[],
    )?;
    let retried = get(&node, "br", &with(partition, &[]))?;
    assert!(matches!(
        field(&get(&node, "bc", &with(partition, &[]))?, "result_ref"),
        Term::Bin(_)
    ));
    assert!(matches!(field(&retried, "error_ref"), Term::Bin(_)));
    assert_ne!(
        field(&retried, "payload_ref"),
        field(&find("br"), "payload_ref")
    );
    assert!(matches!(
        field(&get(&node, "bf", &with(partition, &[]))?, "error_ref"),
        Term::Bin(_)
    ));
    assert_eq!(
        field(&full(&node, "bc", partition)?, "result"),
        Term::List(vec![text("done")])
    );
    let fetched = full(&node, "br", partition)?;
    assert_eq!(field(&fetched, "payload"), kind("retry-updated"));
    assert_eq!(field(&fetched, "error"), retry_error);
    let failed = full(&node, "bf", partition)?;
    assert_eq!(field(&failed, "payload"), kind("fail"));
    assert_eq!(field(&failed, "error"), fail_error);
    Ok(())
}
