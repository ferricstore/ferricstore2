//! `flow_named_values_test.exs` (Elixir `main` 6a5f2bc34), every test.

use wf_application::commands::flow::Term;
use wf_node::composition::QueueNode;

use super::{claimed, field, get, has, int, map, ok, refusal, run, text, texts};
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
    let mut entries = vec![
        ("id", text(id)),
        ("type", text(flow_type)),
        ("run_at_ms", int(1_000)),
        ("now_ms", int(1_000)),
    ];
    entries.extend(extra.iter().cloned());
    ok(node, "FLOW.CREATE", &entries)?;
    Ok(())
}

fn claim_all(node: &QueueNode, flow_type: &str, partition: Option<&str>) -> TestResult<Vec<Term>> {
    let mut entries = vec![
        ("type", text(flow_type)),
        ("worker", text("worker-a")),
        ("limit", int(10)),
        ("now_ms", int(1_000)),
        ("payload", Term::Bool(false)),
    ];
    entries.extend(partition.map(|partition| ("partition_key", text(partition))));
    claimed(node, &entries)
}

fn claim_one(node: &QueueNode, flow_type: &str, partition: Option<&str>) -> TestResult<Term> {
    claim_all(node, flow_type, partition)?
        .into_iter()
        .next()
        .ok_or_else(|| "nothing claimed".into())
}

fn lease(record: &Term) -> [(&'static str, Term); 2] {
    [
        ("lease_token", field(record, "lease_token")),
        ("fencing_token", field(record, "fencing_token")),
    ]
}

fn value_ref(record: &Term, name: &str) -> Term {
    field(&field(&field(record, "value_refs"), name), "ref")
}

fn mget(node: &QueueNode, refs: Vec<Term>) -> TestResult<Term> {
    ok(node, "FLOW.VALUE.MGET", &[("refs", Term::List(refs))])
}

fn retention_until(record: &Term) -> TestResult<i64> {
    match field(record, "terminal_retention_until_ms") {
        Term::Int(until) => Ok(until),
        other => Err(format!("no retention time: {other:?}").into()),
    }
}

fn at_least(reply: &Term, key: &str, minimum: i64) -> bool {
    matches!(field(reply, key), Term::Int(count) if count >= minimum)
}

#[test]
fn create_stores_multiple_named_values_and_get_hydrates_selected_names_only() -> TestResult {
    let node = node("audit-named-create")?;
    create(
        &node,
        "nc",
        "named-values",
        &with(
            P,
            &[(
                "values",
                map(&[
                    ("order", text("order-bytes")),
                    ("payment", text("payment-bytes")),
                ]),
            )],
        ),
    )?;
    let ref_only = get(&node, "nc", &with(P, &[]))?;
    assert!(has(&field(&ref_only, "value_refs"), "order"));
    assert!(!has(&ref_only, "values"), "{ref_only:?}");
    let hydrated = get(&node, "nc", &with(P, &[("values", texts(&["order"]))]))?;
    assert_eq!(
        field(&hydrated, "values"),
        map(&[("order", text("order-bytes"))])
    );
    Ok(())
}

#[test]
fn named_value_put_is_idempotent_and_override_creates_a_new_immutable_ref() -> TestResult {
    let node = node("audit-named-put")?;
    create(&node, "np", "named-values", &with(P, &[]))?;
    let put = |value: &str, override_value: bool, now: i64| {
        run(
            &node,
            "FLOW.VALUE.PUT",
            &with(
                P,
                &[
                    ("value", text(value)),
                    ("owner_flow_id", text("np")),
                    ("name", text("order")),
                    ("override", Term::Bool(override_value)),
                    ("now_ms", int(now)),
                ],
            ),
        )
    };
    let first = put("order-v1", false, 1_010)??;
    assert_eq!(field(&first, "created"), Term::Bool(true));
    assert_eq!(field(&first, "stored"), Term::Bool(true));
    let replay = put("order-v1", false, 1_020)??;
    assert_eq!(field(&replay, "ref"), field(&first, "ref"));
    assert_eq!(field(&replay, "created"), Term::Bool(false));
    assert_eq!(field(&replay, "stored"), Term::Bool(false));
    assert_eq!(
        refusal(put("order-v2", false, 1_030)?)?,
        "ERR flow value order already exists with different digest; use OVERRIDE true"
    );
    let replaced = put("order-v2", true, 1_040)??;
    assert_ne!(field(&replaced, "ref"), field(&first, "ref"));
    assert_eq!(field(&replaced, "version"), int(2));
    let fetched = get(&node, "np", &with(P, &[("values", texts(&["order"]))]))?;
    assert_eq!(
        field(&fetched, "values"),
        map(&[("order", text("order-v2"))])
    );
    Ok(())
}

/// The Elixir test lowers the blob side-channel threshold; the node has no
/// side channel, so what remains is that a large value reads back whole.
#[test]
fn waraft_blob_side_channel_named_value_put_returns_original_large_value() -> TestResult {
    let node = node("audit-named-large")?;
    create(&node, "nl", "named-values", &with(P, &[]))?;
    let payload = "large-named-value".repeat(64);
    let put = ok(
        &node,
        "FLOW.VALUE.PUT",
        &with(
            P,
            &[
                ("value", text(&payload)),
                ("owner_flow_id", text("nl")),
                ("name", text("doc")),
                ("now_ms", int(1_010)),
            ],
        ),
    )?;
    assert_eq!(
        mget(&node, vec![field(&put, "ref")])?,
        Term::List(vec![text(&payload)])
    );
    Ok(())
}

#[test]
fn claim_due_hydrates_selected_named_values_with_one_logical_request() -> TestResult {
    let node = node("audit-named-claim")?;
    create(
        &node,
        "nk",
        "named-claim",
        &with(
            P,
            &[(
                "values",
                map(&[
                    ("order", text("order-bytes")),
                    ("payment", text("payment-bytes")),
                ]),
            )],
        ),
    )?;
    let records = claimed(
        &node,
        &with(
            P,
            &[
                ("type", text("named-claim")),
                ("worker", text("worker-a")),
                ("limit", int(1)),
                ("now_ms", int(1_000)),
                ("payload", Term::Bool(false)),
                ("values", texts(&["payment"])),
            ],
        ),
    )?;
    let record = records.first().cloned().unwrap_or(Term::Nil);
    assert_eq!(field(&record, "id"), text("nk"));
    assert_eq!(
        field(&record, "values"),
        map(&[("payment", text("payment-bytes"))])
    );
    Ok(())
}

#[test]
fn value_mget_decodes_refs_and_preserves_request_order() -> TestResult {
    let node = node("audit-named-mget")?;
    create(
        &node,
        "nm",
        "named-mget",
        &with(
            P,
            &[(
                "values",
                map(&[("a", text("value-a")), ("b", text("value-b"))]),
            )],
        ),
    )?;
    let flow = get(&node, "nm", &with(P, &[]))?;
    let (a, b) = (value_ref(&flow, "a"), value_ref(&flow, "b"));
    assert_eq!(
        mget(&node, vec![b.clone(), a, b])?,
        Term::List(vec![text("value-b"), text("value-a"), text("value-b")])
    );
    Ok(())
}

#[test]
fn transition_can_add_and_drop_named_values_without_hydrating_bytes() -> TestResult {
    let node = node("audit-named-transition")?;
    create(
        &node,
        "nt",
        "named-transition",
        &with(P, &[("values", map(&[("order", text("order-v1"))]))]),
    )?;
    let lease = lease(&claim_one(&node, "named-transition", Some(P))?);
    let mut entries = with(
        P,
        &[
            ("id", text("nt")),
            ("from_state", text("running")),
            ("to_state", text("waiting")),
            ("values", map(&[("payment", text("payment-v1"))])),
            ("drop_values", texts(&["order"])),
            ("run_at_ms", int(2_000)),
            ("now_ms", int(1_100)),
        ],
    );
    entries.extend(lease);
    ok(&node, "FLOW.TRANSITION", &entries)?;
    let fetched = get(
        &node,
        "nt",
        &with(P, &[("values", texts(&["order", "payment"]))]),
    )?;
    assert_eq!(
        field(&fetched, "values"),
        map(&[("payment", text("payment-v1"))])
    );
    assert!(!has(&field(&fetched, "value_refs"), "order"));
    Ok(())
}

#[test]
fn terminal_commands_can_attach_named_values() -> TestResult {
    let node = node("audit-named-terminal")?;
    create(&node, "nx", "named-terminal", &with(P, &[]))?;
    let mut entries = with(
        P,
        &[
            ("id", text("nx")),
            ("values", map(&[("receipt", text("receipt-v1"))])),
            ("now_ms", int(1_100)),
        ],
    );
    entries.extend(lease(&claim_one(&node, "named-terminal", Some(P))?));
    ok(&node, "FLOW.COMPLETE", &entries)?;
    let fetched = get(&node, "nx", &with(P, &[("values", texts(&["receipt"]))]))?;
    assert_eq!(field(&fetched, "state"), text("completed"));
    assert_eq!(
        field(&fetched, "values"),
        map(&[("receipt", text("receipt-v1"))])
    );
    Ok(())
}

const R: &str = "tenant-retention";

#[test]
fn retention_cleanup_removes_owned_named_value_blobs() -> TestResult {
    let node = node("audit-named-retention")?;
    create(
        &node,
        "nr",
        "named-retention",
        &with(
            R,
            &[
                ("values", map(&[("order", text("order-v1"))])),
                ("retention_ttl_ms", int(60_000)),
            ],
        ),
    )?;
    let order = value_ref(&get(&node, "nr", &with(R, &[]))?, "order");
    assert_eq!(
        mget(&node, vec![order.clone()])?,
        Term::List(vec![text("order-v1")])
    );
    let mut entries = with(
        R,
        &[
            ("id", text("nr")),
            ("values", map(&[("receipt", text("receipt-v1"))])),
            ("now_ms", int(1_100)),
        ],
    );
    entries.extend(lease(&claim_one(&node, "named-retention", Some(R))?));
    ok(&node, "FLOW.COMPLETE", &entries)?;
    let completed = get(&node, "nr", &with(R, &[]))?;
    let receipt = value_ref(&completed, "receipt");
    assert!(matches!(receipt, Term::Bin(_)));
    let cleaned = ok(
        &node,
        "FLOW.RETENTION_CLEANUP",
        &[
            ("limit", int(10)),
            ("now_ms", int(retention_until(&completed)? + 1)),
        ],
    )?;
    assert!(at_least(&cleaned, "flows", 1), "{cleaned:?}");
    assert!(at_least(&cleaned, "values", 2), "{cleaned:?}");
    assert_eq!(get(&node, "nr", &with(R, &[]))?, Term::Nil);
    assert_eq!(
        mget(&node, vec![order, receipt])?,
        Term::List(vec![Term::Nil, Term::Nil])
    );
    Ok(())
}

#[test]
fn transition_many_stores_named_value_blobs_for_every_transitioned_flow() -> TestResult {
    let node = node("audit-named-transition-many")?;
    let partition = "tenant-transition-many";
    for id in ["tm-a", "tm-b"] {
        create(&node, id, "named-transition-many", &with(partition, &[]))?;
    }
    let items: Vec<Term> = claim_all(&node, "named-transition-many", Some(partition))?
        .iter()
        .map(|record| {
            map(&[
                ("id", field(record, "id")),
                ("lease_token", field(record, "lease_token")),
                ("fencing_token", field(record, "fencing_token")),
            ])
        })
        .collect();
    assert_eq!(items.len(), 2);
    ok(
        &node,
        "FLOW.TRANSITION_MANY",
        &with(
            partition,
            &[
                ("from_state", text("running")),
                ("to_state", text("waiting")),
                ("items", Term::List(items)),
                ("values", map(&[("step", text("transition-many"))])),
                ("run_at_ms", int(2_000)),
                ("now_ms", int(1_100)),
            ],
        ),
    )?;
    for id in ["tm-a", "tm-b"] {
        let fetched = get(&node, id, &with(partition, &[("values", texts(&["step"]))]))?;
        assert_eq!(field(&fetched, "state"), text("waiting"));
        assert_eq!(
            field(&fetched, "values"),
            map(&[("step", text("transition-many"))])
        );
    }
    Ok(())
}

#[test]
fn rewind_restores_named_value_refs_from_the_target_history_snapshot() -> TestResult {
    let node = node("audit-named-rewind")?;
    create(
        &node,
        "nw",
        "named-rewind",
        &[("values", map(&[("order", text("order-v1"))]))],
    )?;
    let history = super::events(&node, "nw", &[("count", int(10))])?;
    let created = history.first().cloned().unwrap_or(Term::Nil);
    assert_eq!(field(&created, "event"), text("created"));
    let event_id = ok(
        &node,
        "FLOW.HISTORY",
        &[("id", text("nw")), ("count", int(10))],
    )?;
    let Term::List(entries) = event_id else {
        return Err("history is not a list".into());
    };
    let Some(Term::List(first)) = entries.first() else {
        return Err("no first event".into());
    };
    let created_id = first.first().cloned().unwrap_or(Term::Nil);
    let mut transition = vec![
        ("id", text("nw")),
        ("from_state", text("running")),
        ("to_state", text("waiting")),
        ("values", map(&[("order", text("order-v2"))])),
        ("override_values", texts(&["order"])),
        ("run_at_ms", int(2_000)),
        ("now_ms", int(1_100)),
    ];
    transition.extend(lease(&claim_one(&node, "named-rewind", None)?));
    ok(&node, "FLOW.TRANSITION", &transition)?;
    let after = get(&node, "nw", &[("values", texts(&["order"]))])?;
    assert_eq!(field(&after, "values"), map(&[("order", text("order-v2"))]));
    ok(
        &node,
        "FLOW.REWIND",
        &[
            ("id", text("nw")),
            ("to_event", created_id),
            ("expect_state", text("waiting")),
            ("run_at_ms", int(3_000)),
            ("now_ms", int(1_200)),
        ],
    )?;
    let rewound = get(&node, "nw", &[("values", texts(&["order"]))])?;
    assert_eq!(field(&rewound, "state"), text("queued"));
    assert_eq!(
        field(&rewound, "values"),
        map(&[("order", text("order-v1"))])
    );
    Ok(())
}

#[test]
fn retention_cleanup_removes_overridden_named_value_versions_from_history() -> TestResult {
    let node = node("audit-named-retention-override")?;
    create(
        &node,
        "no",
        "named-retention-override",
        &with(
            R,
            &[
                ("values", map(&[("order", text("order-v1"))])),
                ("retention_ttl_ms", int(60_000)),
            ],
        ),
    )?;
    let old = value_ref(&get(&node, "no", &with(R, &[]))?, "order");
    let replaced = ok(
        &node,
        "FLOW.VALUE.PUT",
        &with(
            R,
            &[
                ("value", text("order-v2")),
                ("owner_flow_id", text("no")),
                ("name", text("order")),
                ("override", Term::Bool(true)),
                ("now_ms", int(1_050)),
            ],
        ),
    )?;
    let new = field(&replaced, "ref");
    assert_ne!(new, old);
    assert_eq!(
        mget(&node, vec![old.clone(), new.clone()])?,
        Term::List(vec![text("order-v1"), text("order-v2")])
    );
    let mut entries = with(R, &[("id", text("no")), ("now_ms", int(1_100))]);
    entries.extend(lease(&claim_one(
        &node,
        "named-retention-override",
        Some(R),
    )?));
    ok(&node, "FLOW.COMPLETE", &entries)?;
    let completed = get(&node, "no", &with(R, &[]))?;
    let cleaned = ok(
        &node,
        "FLOW.RETENTION_CLEANUP",
        &[
            ("limit", int(10)),
            ("now_ms", int(retention_until(&completed)? + 1)),
        ],
    )?;
    assert!(at_least(&cleaned, "values", 2), "{cleaned:?}");
    assert_eq!(
        mget(&node, vec![old, new])?,
        Term::List(vec![Term::Nil, Term::Nil])
    );
    Ok(())
}

#[test]
fn create_many_and_spawn_children_support_per_item_named_values() -> TestResult {
    let node = node("audit-named-many")?;
    let partition = "tenant-named-many";
    create(&node, "parent", "named-parent", &with(partition, &[]))?;
    ok(
        &node,
        "FLOW.CREATE_MANY",
        &with(
            partition,
            &[
                (
                    "items",
                    Term::List(vec![map(&[
                        ("id", text("created")),
                        ("values", map(&[("order", text("order-created"))])),
                        ("value_refs", map(&[("external", text("external-ref"))])),
                    ])]),
                ),
                ("type", text("named-many-created")),
                ("run_at_ms", int(1_000)),
                ("now_ms", int(1_000)),
            ],
        ),
    )?;
    let created = get(
        &node,
        "created",
        &with(partition, &[("values", texts(&["order"]))]),
    )?;
    assert_eq!(
        field(&created, "values"),
        map(&[("order", text("order-created"))])
    );
    assert_eq!(value_ref(&created, "external"), text("external-ref"));
    let parent = claim_one(&node, "named-parent", Some(partition))?;
    let mut spawn = with(
        partition,
        &[
            ("id", text("parent")),
            (
                "children",
                Term::List(vec![map(&[
                    ("id", text("child")),
                    ("type", text("named-child")),
                    ("values", map(&[("order", text("order-child"))])),
                    (
                        "value_refs",
                        map(&[("external", text("external-child-ref"))]),
                    ),
                ])]),
            ),
            ("group_id", text("named-group")),
            ("wait_state", text("waiting")),
            ("success", text("completed")),
            ("failure", text("failed")),
            ("now_ms", int(1_100)),
        ],
    );
    spawn.extend(lease(&parent));
    ok(&node, "FLOW.SPAWN_CHILDREN", &spawn)?;
    let child = get(
        &node,
        "child",
        &with(partition, &[("values", texts(&["order"]))]),
    )?;
    assert_eq!(
        field(&child, "values"),
        map(&[("order", text("order-child"))])
    );
    assert_eq!(value_ref(&child, "external"), text("external-child-ref"));
    Ok(())
}
