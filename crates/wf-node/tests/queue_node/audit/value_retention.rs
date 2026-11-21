//! `flow_value_payload_test.exs` (Elixir `main` 6a5f2bc34), the tests of
//! cancel reasons, retention and rewinds out of retention.

use wf_application::commands::flow::Term;
use wf_node::composition::QueueNode;

use super::{claimed, events, field, get, int, map, ok, refusal, run, text};
use crate::{TestResult, directory, open};

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

fn until(record: &Term) -> TestResult<i64> {
    match field(record, "terminal_retention_until_ms") {
        Term::Int(until) => Ok(until),
        other => Err(format!("no retention time: {other:?}").into()),
    }
}

fn at_least(reply: &Term, key: &str, minimum: i64) -> bool {
    matches!(field(reply, key), Term::Int(count) if count >= minimum)
}

#[test]
fn cancel_stores_inline_reason_payload_as_an_owned_terminal_value() -> TestResult {
    let node = node("audit-payload-cancel-reason")?;
    let reason = map(&[
        ("code", text("user_cancelled")),
        ("details", text(&"x".repeat(256))),
    ]);
    create(
        &node,
        "vx",
        "value-cancel-reason",
        &with(
            R,
            &[
                ("payload", map(&[("large", text(&"p".repeat(256)))])),
                ("retention_ttl_ms", int(2_000)),
                ("run_at_ms", int(1_000)),
                ("now_ms", int(1_000)),
            ],
        ),
    )?;
    let created = get(&node, "vx", &with(R, &[]))?;
    ok(
        &node,
        "FLOW.CANCEL",
        &with(
            R,
            &[
                ("id", text("vx")),
                ("fencing_token", field(&created, "fencing_token")),
                ("reason", reason.clone()),
                ("now_ms", int(1_001)),
            ],
        ),
    )?;
    let cancelled = get(&node, "vx", &with(R, &[]))?;
    let error_ref = field(&cancelled, "error_ref");
    assert!(matches!(error_ref, Term::Bin(ref bytes) if !bytes.is_empty()));
    assert_eq!(field(&full(&node, "vx", R)?, "error"), reason);
    let cleaned = ok(
        &node,
        "FLOW.RETENTION_CLEANUP",
        &[("limit", int(10)), ("now_ms", int(until(&cancelled)? + 1))],
    )?;
    assert!(
        at_least(&cleaned, "flows", 1) && at_least(&cleaned, "values", 1),
        "{cleaned:?}"
    );
    assert_eq!(
        mget(&node, vec![error_ref], &[])?,
        Term::List(vec![Term::Nil])
    );
    Ok(())
}

#[test]
fn retention_cleanup_removes_expired_terminal_state_history_and_owned_values() -> TestResult {
    let node = node("audit-payload-cleanup")?;
    create(
        &node,
        "vy",
        "value-retention-cleanup",
        &with(
            R,
            &[
                ("payload", map(&[("large", text(&"p".repeat(256)))])),
                ("retention_ttl_ms", int(60_000)),
                ("run_at_ms", int(1_000)),
                ("now_ms", int(1_000)),
            ],
        ),
    )?;
    let created = get(&node, "vy", &with(R, &[]))?;
    let records = claim_all(&node, R, &[("type", text("value-retention-cleanup"))])?;
    let mut complete = with(
        R,
        &[
            ("id", text("vy")),
            ("result", map(&[("ok", Term::Bool(true))])),
            ("now_ms", int(1_100)),
        ],
    );
    complete.extend(lease(records.first().unwrap_or(&Term::Nil)));
    ok(&node, "FLOW.COMPLETE", &complete)?;
    let completed = get(&node, "vy", &with(R, &[]))?;
    let cleaned = ok(
        &node,
        "FLOW.RETENTION_CLEANUP",
        &[("limit", int(10)), ("now_ms", int(until(&completed)? + 1))],
    )?;
    assert!(at_least(&cleaned, "flows", 1), "{cleaned:?}");
    assert!(at_least(&cleaned, "history", 1), "{cleaned:?}");
    assert!(at_least(&cleaned, "values", 2), "{cleaned:?}");
    assert_eq!(get(&node, "vy", &with(R, &[]))?, Term::Nil);
    assert_eq!(
        events(&node, "vy", &with(R, &[("count", int(10))]))?,
        Vec::new()
    );
    assert_eq!(
        mget(
            &node,
            vec![
                field(&created, "payload_ref"),
                field(&completed, "result_ref")
            ],
            &[]
        )?,
        Term::List(vec![Term::Nil, Term::Nil])
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
    assert_eq!(
        refusal(refused)?,
        "ERR flow reason_ref input is not supported; use reason"
    );
    assert_eq!(
        field(&get(&node, "vr", &with(R, &[]))?, "state"),
        text("queued")
    );
    Ok(())
}

#[test]
fn retention_cleanup_returns_an_exact_continuation_without_a_final_empty_pass() -> TestResult {
    let node = node("audit-payload-continuation")?;
    let partition = "tenant-retention-continuation";
    for id in ["ra", "rb"] {
        create(
            &node,
            id,
            "retention-continuation",
            &with(
                partition,
                &[
                    ("retention_ttl_ms", int(60_000)),
                    ("run_at_ms", int(1_000)),
                    ("now_ms", int(1_000)),
                ],
            ),
        )?;
    }
    let records = claim_all(
        &node,
        partition,
        &[("type", text("retention-continuation")), ("limit", int(2))],
    )?;
    assert_eq!(records.len(), 2);
    for record in &records {
        let mut complete = with(
            partition,
            &[("id", field(record, "id")), ("now_ms", int(1_100))],
        );
        complete.extend(lease(record));
        ok(&node, "FLOW.COMPLETE", &complete)?;
    }
    let mut now = 0;
    for id in ["ra", "rb"] {
        now = now.max(until(&get(&node, id, &with(partition, &[]))?)? + 1);
    }
    let first = ok(
        &node,
        "FLOW.RETENTION_CLEANUP",
        &[("limit", int(1)), ("now_ms", int(now))],
    )?;
    assert_eq!(field(&first, "flows"), int(1));
    assert_eq!(field(&first, "more?"), Term::Bool(true));
    let continuation = field(&first, "continuation");
    assert!(matches!(continuation, Term::Bin(_)));
    let second = ok(
        &node,
        "FLOW.RETENTION_CLEANUP",
        &[
            ("limit", int(1)),
            ("now_ms", int(now)),
            ("continuation", continuation),
        ],
    )?;
    assert_eq!(field(&second, "flows"), int(1));
    assert_eq!(field(&second, "more?"), Term::Bool(false));
    assert_eq!(field(&second, "continuation"), Term::Nil);
    for id in ["ra", "rb"] {
        assert_eq!(get(&node, id, &with(partition, &[]))?, Term::Nil);
    }
    let cleanup = |continuation: Term| {
        run(
            &node,
            "FLOW.RETENTION_CLEANUP",
            &[
                ("limit", int(1)),
                ("now_ms", int(now)),
                ("continuation", continuation),
            ],
        )
    };
    // Refused; Elixir's wording differs.
    assert!(cleanup(Term::Bin(vec![1, 9, 0, 0, 0, 0]))?.is_err());
    assert_eq!(
        refusal(cleanup(map(&[("shard", int(0))]))?)?,
        "ERR flow continuation must be a binary token"
    );
    Ok(())
}
