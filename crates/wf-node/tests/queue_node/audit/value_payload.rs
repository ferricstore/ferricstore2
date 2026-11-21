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

use super::{claimed, field, get, has, int, map, ok, run, text};
use crate::{TestResult, directory, open};

const P: &str = "tenant-a";
const R: &str = "tenant-retention";

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
    entries.extend(extra.iter().cloned());
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

fn until(record: &Term) -> TestResult<i64> {
    match field(record, "terminal_retention_until_ms") {
        Term::Int(until) => Ok(until),
        other => Err(format!("no retention time: {other:?}").into()),
    }
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

#[test]
fn cancel_rejects_public_reason_ref_input_without_touching_external_value() -> TestResult {
    let node = node("audit-payload-reason-ref")?;
    create(
        &node,
        "vr",
        "value-cancel-reason-ref",
        &with(
            R,
            &[
                ("payload", map(&[("large", text(&"x".repeat(256)))])),
                ("retention_ttl_ms", int(100)),
                ("run_at_ms", int(1_000)),
                ("now_ms", int(1_000)),
            ],
        ),
    )?;
    let created = get(&node, "vr", &with(R, &[]))?;
    let refused = run(
        &node,
        "FLOW.CANCEL",
        &with(
            R,
            &[
                ("id", text("vr")),
                ("fencing_token", field(&created, "fencing_token")),
                ("reason_ref", text("flow-value-cancel-reason-user-key")),
            ],
        ),
    )?;
    // Elixir names `reason` in its text; this node refuses the option.
    assert!(refused.is_err(), "{refused:?}");
    assert_eq!(
        field(&get(&node, "vr", &with(R, &[]))?, "state"),
        text("queued")
    );
    Ok(())
}

#[test]
fn rewind_from_terminal_back_to_active_clears_value_ref_expiration() -> TestResult {
    let node = node("audit-payload-rewind")?;
    let payload = map(&[("large", text(&"x".repeat(256)))]);
    create(
        &node,
        "vw",
        "value-rewind-retention",
        &with(
            R,
            &[
                ("payload", payload.clone()),
                ("retention_ttl_ms", int(60_000)),
                ("run_at_ms", int(1_000)),
                ("now_ms", int(1_000)),
            ],
        ),
    )?;
    let created = get(&node, "vw", &with(R, &[]))?;
    let Term::List(history) = ok(
        &node,
        "FLOW.HISTORY",
        &with(R, &[("id", text("vw")), ("count", int(10))]),
    )?
    else {
        return Err("history is not a list".into());
    };
    assert_eq!(history.len(), 1);
    let Some(Term::List(first)) = history.first() else {
        return Err("no created event".into());
    };
    let created_event = first.first().cloned().unwrap_or(Term::Nil);
    let records = claim_all(&node, R, &[("type", text("value-rewind-retention"))])?;
    let mut complete = with(R, &[("id", text("vw")), ("now_ms", int(1_000))]);
    complete.extend(lease(records.first().unwrap_or(&Term::Nil)));
    ok(&node, "FLOW.COMPLETE", &complete)?;
    let cleanup_now = until(&get(&node, "vw", &with(R, &[]))?)? + 1;
    ok(
        &node,
        "FLOW.REWIND",
        &with(
            R,
            &[
                ("id", text("vw")),
                ("to_event", created_event),
                ("now_ms", int(1_000)),
            ],
        ),
    )?;
    let rewound = get(&node, "vw", &with(R, &[]))?;
    assert_eq!(field(&rewound, "state"), field(&created, "state"));
    assert_eq!(
        field(&rewound, "payload_ref"),
        field(&created, "payload_ref")
    );
    assert_eq!(field(&rewound, "terminal_retention_until_ms"), Term::Nil);
    // Elixir reads at the cleanup time; a cleanup then must keep it.
    ok(
        &node,
        "FLOW.RETENTION_CLEANUP",
        &[("limit", int(10)), ("now_ms", int(cleanup_now))],
    )?;
    let fetched = full(&node, "vw", R)?;
    assert_eq!(field(&fetched, "state"), field(&created, "state"));
    assert_eq!(field(&fetched, "payload"), payload);
    assert_eq!(
        mget(&node, vec![field(&created, "payload_ref")], &[])?,
        Term::List(vec![payload])
    );
    Ok(())
}
