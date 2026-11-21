//! `flow_signal_test.exs` (Elixir `main` 6a5f2bc34), every test.

use wf_application::commands::flow::Term;
use wf_node::composition::QueueNode;

use super::{claimed, events, field, get, has, int, map, ok, refusal, run, text, texts};
use crate::{TestResult, directory, open};

const P: &str = "tenant-a";

fn node(tag: &str) -> TestResult<QueueNode> {
    open(&directory(tag)?, 4)
}

fn create(node: &QueueNode, id: &str, flow_type: &str, extra: &[(&str, Term)]) -> TestResult {
    let mut entries = vec![("id", text(id)), ("type", text(flow_type))];
    entries.extend(extra.iter().cloned());
    ok(node, "FLOW.CREATE", &entries)?;
    Ok(())
}

fn in_p<'a>(entries: &[(&'a str, Term)]) -> Vec<(&'a str, Term)> {
    let mut all = vec![("partition_key", text(P))];
    all.extend(entries.iter().cloned());
    all
}

fn signal(node: &QueueNode, id: &str, entries: &[(&str, Term)]) -> TestResult<super::Reply> {
    let mut all = in_p(entries);
    all.push(("id", text(id)));
    run(node, "FLOW.SIGNAL", &all)
}

/// Claims one Flow of `flow_type` in partition `P` at `now`.
fn claim_one(node: &QueueNode, flow_type: &str, now: i64) -> TestResult<Term> {
    let records = claimed(
        node,
        &in_p(&[
            ("type", text(flow_type)),
            ("worker", text("worker-a")),
            ("lease_ms", int(10_000)),
            ("payload", Term::Bool(false)),
            ("now_ms", int(now)),
        ]),
    )?;
    records
        .into_iter()
        .next()
        .ok_or_else(|| "nothing claimed".into())
}

fn complete(node: &QueueNode, id: &str, lease: &Term, now: i64) -> TestResult {
    ok(
        node,
        "FLOW.COMPLETE",
        &in_p(&[
            ("id", text(id)),
            ("lease_token", field(lease, "lease_token")),
            ("fencing_token", field(lease, "fencing_token")),
            ("now_ms", int(now)),
        ]),
    )?;
    Ok(())
}

fn get_p(node: &QueueNode, id: &str, values: &[&str]) -> TestResult<Term> {
    let mut extra = vec![("partition_key", text(P))];
    if !values.is_empty() {
        extra.push(("values", texts(values)));
    }
    get(node, id, &extra)
}

#[test]
fn signal_attaches_named_values_and_history_records_the_signal() -> TestResult {
    let node = node("audit-signal-values")?;
    create(
        &node,
        "sv",
        "signal-values",
        &in_p(&[
            ("values", map(&[("order", text("order-bytes"))])),
            ("run_at_ms", int(1_000)),
            ("now_ms", int(1_000)),
        ]),
    )?;
    let signaled = signal(
        &node,
        "sv",
        &[
            ("signal", text("payment_received")),
            ("values", map(&[("payment_event", text("payment-bytes"))])),
            ("now_ms", int(1_100)),
        ],
    )??;
    assert_eq!(signaled, text("OK"));
    let fetched = get_p(&node, "sv", &["payment_event"])?;
    assert_eq!(field(&fetched, "state"), text("queued"));
    assert_eq!(
        field(&fetched, "values"),
        map(&[("payment_event", text("payment-bytes"))])
    );
    assert!(has(&field(&fetched, "value_refs"), "order"));
    let history = events(
        &node,
        "sv",
        &[("partition_key", text(P)), ("count", int(10))],
    )?;
    assert!(history.iter().any(|fields| {
        field(fields, "event") == text("signaled")
            && field(fields, "signal") == text("payment_received")
    }));
    Ok(())
}

#[test]
fn signal_can_atomically_attach_values_and_guarded_transition_state() -> TestResult {
    let node = node("audit-signal-transition")?;
    create(
        &node,
        "st",
        "signal-transition",
        &in_p(&[
            ("state", text("waiting_payment")),
            ("run_at_ms", int(10_000)),
            ("now_ms", int(1_000)),
        ]),
    )?;
    signal(
        &node,
        "st",
        &[
            ("signal", text("payment_received")),
            ("if_state", text("waiting_payment")),
            ("transition_to", text("verify_payment")),
            ("values", map(&[("payment_event", text("payment-v1"))])),
            ("run_at_ms", int(1_250)),
            ("now_ms", int(1_100)),
        ],
    )??;
    let fetched = get_p(&node, "st", &["payment_event"])?;
    assert_eq!(field(&fetched, "state"), text("verify_payment"));
    assert_eq!(field(&fetched, "next_run_at_ms"), int(1_250));
    assert_eq!(
        field(&fetched, "values"),
        map(&[("payment_event", text("payment-v1"))])
    );
    let records = claimed(
        &node,
        &in_p(&[
            ("type", text("signal-transition")),
            ("state", text("verify_payment")),
            ("worker", text("worker-a")),
            ("payload", Term::Bool(false)),
            ("values", texts(&["payment_event"])),
            ("now_ms", int(1_250)),
        ]),
    )?;
    assert_eq!(records.len(), 1);
    let record = records.first().cloned().unwrap_or(Term::Nil);
    assert_eq!(field(&record, "id"), text("st"));
    assert_eq!(
        field(&record, "values"),
        map(&[("payment_event", text("payment-v1"))])
    );
    Ok(())
}

fn fifo_type(node: &QueueNode, flow_type: &str) -> TestResult {
    ok(
        node,
        "FLOW.POLICY.SET",
        &[
            ("type", text(flow_type)),
            ("states", map(&[("queued", map(&[("mode", text("fifo"))]))])),
        ],
    )?;
    Ok(())
}

#[test]
fn signal_rejects_entry_into_a_fifo_state_without_an_explicit_partition() -> TestResult {
    let node = node("audit-signal-fifo-partition")?;
    fifo_type(&node, "sfp")?;
    ok(
        &node,
        "FLOW.CREATE",
        &[
            ("id", text("sf")),
            ("type", text("sfp")),
            ("state", text("waiting")),
            ("run_at_ms", int(10_000)),
            ("now_ms", int(1_000)),
        ],
    )?;
    let refused = run(
        &node,
        "FLOW.SIGNAL",
        &[
            ("id", text("sf")),
            ("signal", text("ready")),
            ("if_state", text("waiting")),
            ("transition_to", text("queued")),
            ("run_at_ms", int(2_000)),
            ("now_ms", int(1_100)),
        ],
    )?;
    assert_eq!(
        refusal(refused)?,
        "ERR flow partition_key is required for fifo state"
    );
    assert_eq!(field(&get(&node, "sf", &[])?, "state"), text("waiting"));
    Ok(())
}

#[test]
fn fifo_claims_follow_signal_state_entry_order_instead_of_creation_order() -> TestResult {
    let node = node("audit-signal-fifo-order")?;
    fifo_type(&node, "sfo")?;
    for (id, now) in [("created-first", 1_000), ("entered-first", 1_001)] {
        create(
            &node,
            id,
            "sfo",
            &in_p(&[
                ("state", text("waiting")),
                ("run_at_ms", int(10_000)),
                ("now_ms", int(now)),
            ]),
        )?;
    }
    for (id, now) in [("entered-first", 2_000), ("created-first", 2_001)] {
        signal(
            &node,
            id,
            &[
                ("signal", text("ready")),
                ("if_state", text("waiting")),
                ("transition_to", text("queued")),
                ("run_at_ms", int(3_000)),
                ("now_ms", int(now)),
            ],
        )??;
    }
    let records = claimed(
        &node,
        &in_p(&[
            ("type", text("sfo")),
            ("state", text("queued")),
            ("worker", text("signal-fifo-worker")),
            ("now_ms", int(3_000)),
        ]),
    )?;
    let ids: Vec<Term> = records.iter().map(|record| field(record, "id")).collect();
    assert_eq!(ids, vec![text("entered-first")]);
    Ok(())
}

#[test]
fn signal_transition_requires_an_if_state_guard() -> TestResult {
    let node = node("audit-signal-guard-required")?;
    create(
        &node,
        "sg",
        "signal-guard-required",
        &in_p(&[
            ("state", text("waiting_payment")),
            ("run_at_ms", int(10_000)),
            ("now_ms", int(1_000)),
        ]),
    )?;
    let refused = signal(
        &node,
        "sg",
        &[
            ("signal", text("payment_received")),
            ("transition_to", text("verify_payment")),
            ("run_at_ms", int(1_250)),
            ("now_ms", int(1_100)),
        ],
    )?;
    assert_eq!(
        refusal(refused)?,
        "ERR flow signal transition requires if_state"
    );
    let fetched = get_p(&node, "sg", &[])?;
    assert_eq!(field(&fetched, "state"), text("waiting_payment"));
    assert_eq!(field(&fetched, "next_run_at_ms"), int(10_000));
    Ok(())
}

#[test]
fn signal_cannot_transition_a_leased_running_flow_without_matching_running_guard() -> TestResult {
    let node = node("audit-signal-running-guard")?;
    create(
        &node,
        "sr",
        "signal-running",
        &in_p(&[("run_at_ms", int(1_000)), ("now_ms", int(1_000))]),
    )?;
    let lease = claim_one(&node, "signal-running", 1_000)?;
    let refused = signal(
        &node,
        "sr",
        &[
            ("signal", text("external_interrupt")),
            ("if_state", text("waiting")),
            ("transition_to", text("manual_review")),
            ("run_at_ms", int(1_200)),
            ("now_ms", int(1_100)),
        ],
    )?;
    assert_eq!(refusal(refused)?, "ERR flow state mismatch");
    let fetched = get_p(&node, "sr", &[])?;
    assert_eq!(field(&fetched, "state"), text("running"));
    assert_eq!(field(&fetched, "lease_token"), field(&lease, "lease_token"));
    assert_eq!(
        field(&fetched, "fencing_token"),
        field(&lease, "fencing_token")
    );
    Ok(())
}

#[test]
fn signal_cannot_transition_an_already_terminal_flow() -> TestResult {
    let node = node("audit-signal-terminal")?;
    create(
        &node,
        "sx",
        "signal-terminal",
        &in_p(&[("run_at_ms", int(1_000)), ("now_ms", int(1_000))]),
    )?;
    let lease = claim_one(&node, "signal-terminal", 1_000)?;
    complete(&node, "sx", &lease, 1_100)?;
    let refused = signal(
        &node,
        "sx",
        &[
            ("signal", text("external_event")),
            ("if_state", text("completed")),
            ("transition_to", text("queued")),
            ("now_ms", int(1_200)),
        ],
    )?;
    assert_eq!(refusal(refused)?, "ERR flow is terminal; use FLOW.REWIND");
    assert_eq!(field(&get_p(&node, "sx", &[])?, "state"), text("completed"));
    Ok(())
}

#[test]
fn signal_if_state_guard_rejects_stale_state_without_value_writes() -> TestResult {
    let node = node("audit-signal-stale")?;
    create(
        &node,
        "ss",
        "signal-guard",
        &in_p(&[
            ("state", text("waiting_inventory")),
            ("run_at_ms", int(1_000)),
            ("now_ms", int(1_000)),
        ]),
    )?;
    let refused = signal(
        &node,
        "ss",
        &[
            ("signal", text("payment_received")),
            ("if_state", text("waiting_payment")),
            ("transition_to", text("verify_payment")),
            ("values", map(&[("payment_event", text("payment-v1"))])),
            ("now_ms", int(1_100)),
        ],
    )?;
    assert_eq!(refusal(refused)?, "ERR flow state mismatch");
    let fetched = get_p(&node, "ss", &["payment_event"])?;
    assert_eq!(field(&fetched, "state"), text("waiting_inventory"));
    assert!(!has(&fetched, "values"), "{fetched:?}");
    assert!(!has(&field(&fetched, "value_refs"), "payment_event"));
    Ok(())
}

#[test]
fn signal_idempotency_key_replays_same_digest_and_rejects_conflicting_digest() -> TestResult {
    let node = node("audit-signal-idem")?;
    create(
        &node,
        "si",
        "signal-idem",
        &in_p(&[
            ("state", text("waiting_payment")),
            ("run_at_ms", int(1_000)),
            ("now_ms", int(1_000)),
        ]),
    )?;
    let first = [
        ("signal", text("payment_received")),
        ("idempotency_key", text("stripe_evt_1")),
        ("values", map(&[("payment_event", text("payment-v1"))])),
        ("now_ms", int(1_100)),
    ];
    assert_eq!(signal(&node, "si", &first)??, text("OK"));
    assert_eq!(signal(&node, "si", &first)??, text("OK"));
    let refused = signal(
        &node,
        "si",
        &[
            ("signal", text("payment_received")),
            ("idempotency_key", text("stripe_evt_1")),
            ("values", map(&[("payment_event", text("payment-v2"))])),
            ("override_values", texts(&["payment_event"])),
            ("now_ms", int(1_200)),
        ],
    )?;
    assert_eq!(refusal(refused)?, "ERR flow signal idempotency conflict");
    assert_eq!(
        field(&get_p(&node, "si", &["payment_event"])?, "values"),
        map(&[("payment_event", text("payment-v1"))])
    );
    Ok(())
}

#[test]
fn signal_idempotency_key_rejects_conflicting_transition_semantics() -> TestResult {
    let node = node("audit-signal-idem-transition")?;
    create(
        &node,
        "sj",
        "signal-idem-transition",
        &in_p(&[
            ("state", text("waiting_payment")),
            ("run_at_ms", int(1_000)),
            ("now_ms", int(1_000)),
        ]),
    )?;
    let moving = |target: &str, run_at: i64| {
        signal(
            &node,
            "sj",
            &[
                ("signal", text("payment_received")),
                ("idempotency_key", text("stripe_evt_2")),
                ("if_state", texts(&["waiting_payment", "waiting_capture"])),
                ("transition_to", text(target)),
                ("run_at_ms", int(run_at)),
                ("now_ms", int(1_100)),
            ],
        )
    };
    assert_eq!(moving("verify_payment", 1_200)??, text("OK"));
    assert_eq!(
        refusal(moving("manual_review", 1_200)?)?,
        "ERR flow signal idempotency conflict"
    );
    assert_eq!(
        refusal(moving("verify_payment", 1_300)?)?,
        "ERR flow signal idempotency conflict"
    );
    Ok(())
}

#[test]
fn passive_signal_preserves_active_lease_and_fencing_token() -> TestResult {
    let node = node("audit-signal-lease")?;
    create(
        &node,
        "sl",
        "signal-lease",
        &in_p(&[("run_at_ms", int(1_000)), ("now_ms", int(1_000))]),
    )?;
    let lease = claim_one(&node, "signal-lease", 1_000)?;
    signal(
        &node,
        "sl",
        &[
            ("signal", text("external_note")),
            ("values", map(&[("note", text("ok"))])),
            ("now_ms", int(1_100)),
        ],
    )??;
    let fetched = get_p(&node, "sl", &["note"])?;
    assert_eq!(field(&fetched, "state"), text("running"));
    assert_eq!(field(&fetched, "lease_owner"), text("worker-a"));
    assert_eq!(field(&fetched, "lease_token"), field(&lease, "lease_token"));
    assert_eq!(
        field(&fetched, "fencing_token"),
        field(&lease, "fencing_token")
    );
    assert_eq!(field(&fetched, "lease_deadline_ms"), int(11_000));
    assert_eq!(field(&fetched, "values"), map(&[("note", text("ok"))]));
    complete(&node, "sl", &lease, 1_200)
}

#[test]
fn retention_cleanup_removes_signal_owned_named_value_blobs() -> TestResult {
    let node = node("audit-signal-retention")?;
    create(
        &node,
        "sz",
        "signal-retention",
        &in_p(&[
            ("retention_ttl_ms", int(60_000)),
            ("run_at_ms", int(1_000)),
            ("now_ms", int(1_000)),
        ]),
    )?;
    signal(
        &node,
        "sz",
        &[
            ("signal", text("artifact_ready")),
            ("values", map(&[("artifact", text("artifact-bytes"))])),
            ("now_ms", int(1_050)),
        ],
    )??;
    let refs = field(&get_p(&node, "sz", &[])?, "value_refs");
    let artifact = field(&field(&refs, "artifact"), "ref");
    assert!(matches!(artifact, Term::Bin(_)), "{refs:?}");
    let mget = |reference: &Term| {
        ok(
            &node,
            "FLOW.VALUE.MGET",
            &[("refs", Term::List(vec![reference.clone()]))],
        )
    };
    assert_eq!(mget(&artifact)?, Term::List(vec![text("artifact-bytes")]));
    let lease = claim_one(&node, "signal-retention", 1_000)?;
    complete(&node, "sz", &lease, 1_100)?;
    let until = match field(&get_p(&node, "sz", &[])?, "terminal_retention_until_ms") {
        Term::Int(until) => until,
        other => return Err(format!("no retention time: {other:?}").into()),
    };
    let cleaned = ok(
        &node,
        "FLOW.RETENTION_CLEANUP",
        &[("limit", int(10)), ("now_ms", int(until + 1))],
    )?;
    assert!(
        matches!(field(&cleaned, "values"), Term::Int(count) if count >= 1),
        "{cleaned:?}"
    );
    assert_eq!(get_p(&node, "sz", &[])?, Term::Nil);
    assert_eq!(mget(&artifact)?, Term::List(vec![Term::Nil]));
    Ok(())
}

/// Not an Elixir test. The reference server (6a5f2bc34) checks FIFO entry
/// on every signal and so refuses a passive signal on a claimed Flow whose
/// run state is a FIFO state (probe `scenario_signal_fifo.py`, 21 Nov).
/// A passive signal enters no lane, so this node accepts it (user, 21 Nov:
/// no legacy behaviour needed).
#[test]
fn a_passive_signal_on_a_claimed_fifo_flow_keeps_its_lease() -> TestResult {
    let node = node("audit-signal-fifo-running")?;
    fifo_type(&node, "sfr")?;
    create(
        &node,
        "sq",
        "sfr",
        &in_p(&[("run_at_ms", int(1_000)), ("now_ms", int(1_000))]),
    )?;
    assert_eq!(
        signal(
            &node,
            "sq",
            &[("signal", text("note")), ("now_ms", int(1_200))]
        )??,
        text("OK")
    );
    let lease = claim_one(&node, "sfr", 1_300)?;
    assert_eq!(
        signal(
            &node,
            "sq",
            &[("signal", text("note")), ("now_ms", int(1_400))]
        )??,
        text("OK")
    );
    let fetched = get_p(&node, "sq", &[])?;
    assert_eq!(field(&fetched, "version"), int(4));
    assert_eq!(field(&fetched, "lease_token"), field(&lease, "lease_token"));
    Ok(())
}
