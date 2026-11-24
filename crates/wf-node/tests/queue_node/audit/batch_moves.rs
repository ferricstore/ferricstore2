//! `flow_test/sections/flow_claim_due_automatically_reclaims_expired_
//! leases_ratio.exs`, its batches: `TRANSITION_MANY` and `COMPLETE_MANY`
//! move a partition's Flows together or not at all, an independent batch
//! answers per item, a shared payload gives each Flow its own reference
//! and is logged once, and items without a partition go to their own auto
//! partition.

use wf_application::commands::flow::Term;
use wf_node::composition::QueueNode;

use super::{claimed, events, field, get, int, map, ok, refusal, run, text};
use crate::{TestResult, directory, open};

const P: &str = "tenant-batch";

fn in_p() -> [(&'static str, Term); 1] {
    [("partition_key", text(P))]
}

/// Creates `ids` of `flow_type` in `P`, due at 1000.
fn create_many(node: &QueueNode, flow_type: &str, ids: &[&str]) -> TestResult {
    ok(
        node,
        "FLOW.CREATE_MANY",
        &[
            ("partition_key", text(P)),
            ("type", text(flow_type)),
            ("state", text("queued")),
            ("run_at_ms", int(1_000)),
            ("now_ms", int(900)),
            ("items", Term::List(ids.iter().map(|id| text(id)).collect())),
        ],
    )?;
    Ok(())
}

fn fenced(ids: &[(&str, i64)]) -> Term {
    Term::List(
        ids.iter()
            .map(|(id, fence)| map(&[("id", text(id)), ("fencing_token", int(*fence))]))
            .collect(),
    )
}

fn transition_many(
    node: &QueueNode,
    to: &str,
    items: Term,
    extra: &[(&str, Term)],
) -> TestResult<Result<Term, String>> {
    let mut entries = vec![
        ("partition_key", text(P)),
        ("from_state", text("queued")),
        ("to_state", text(to)),
        ("items", items),
        ("run_at_ms", int(2_000)),
        ("now_ms", int(1_100)),
    ];
    entries.extend(extra.iter().cloned());
    Ok(run(node, "FLOW.TRANSITION_MANY", &entries)?.map_err(|error| error.to_string()))
}

fn state(node: &QueueNode, id: &str) -> TestResult<Term> {
    Ok(field(&get(node, id, &in_p())?, "state"))
}

fn claim_all(node: &QueueNode, flow_type: &str, state: &str) -> TestResult<Vec<Term>> {
    claimed(
        node,
        &[
            ("type", text(flow_type)),
            ("partition_key", text(P)),
            ("state", text(state)),
            ("worker", text("worker-a")),
            ("limit", int(10)),
            ("now_ms", int(2_000)),
        ],
    )
}

#[test]
fn a_transition_batch_moves_a_partition_together_or_not_at_all() -> TestResult {
    // Elixir: "flow_transition_many atomically moves one-partition batch",
    // "... rejects direct moves into running state", "... rolls back when
    // any item fails guard" and "... spans shards and rolls back failing
    // shard group".
    let node = open(&directory("audit-batch-move")?, 4)?;
    create_many(&node, "bulk", &["a", "b"])?;
    transition_many(&node, "ready", fenced(&[("a", 0), ("b", 0)]), &[])??;
    for id in ["a", "b"] {
        let record = get(&node, id, &in_p())?;
        assert_eq!(
            (field(&record, "state"), field(&record, "partition_key")),
            (text("ready"), text(P))
        );
    }
    assert_eq!(claim_all(&node, "bulk", "queued")?, Vec::<Term>::new());
    assert_eq!(claim_all(&node, "bulk", "ready")?.len(), 2);

    create_many(&node, "bulk-running", &["r1", "r2"])?;
    let running = transition_many(&node, "running", fenced(&[("r1", 0), ("r2", 0)]), &[])?;
    assert_eq!(
        running,
        Err("ERR flow running state is only entered by FLOW.CLAIM_DUE".to_owned())
    );
    for id in ["r1", "r2"] {
        assert_eq!(state(&node, id)?, text("queued"));
    }

    create_many(&node, "bulk-rollback", &["good", "bad"])?;
    let stale = transition_many(&node, "ready", fenced(&[("good", 0), ("bad", 1)]), &[])?;
    assert_eq!(stale, Err("ERR stale flow lease".to_owned()));
    for id in ["good", "bad"] {
        let record = get(&node, id, &in_p())?;
        assert_eq!(
            (field(&record, "state"), field(&record, "version")),
            (text("queued"), int(1))
        );
        let names: Vec<Term> = events(&node, id, &in_p())?
            .iter()
            .map(|fields| field(fields, "event"))
            .collect();
        assert_eq!(names, [text("created")]);
    }
    assert_eq!(claim_all(&node, "bulk-rollback", "queued")?.len(), 2);
    Ok(())
}

#[test]
fn an_independent_transition_batch_keeps_its_successes() -> TestResult {
    // Elixir: "flow_transition_many independent keeps successful items when
    // one item fails".
    let node = open(&directory("audit-batch-independent")?, 4)?;
    create_many(&node, "bulk-independent", &["bad", "good"])?;
    let reply = transition_many(
        &node,
        "ready",
        fenced(&[("bad", 1), ("good", 0)]),
        &[("independent", Term::Bool(true))],
    )??;
    assert_eq!(
        reply,
        Term::List(vec![
            Term::List(vec![text("error"), text("ERR stale flow lease")]),
            text("ok")
        ])
    );
    assert_eq!(state(&node, "bad")?, text("queued"));
    assert_eq!(state(&node, "good")?, text("ready"));
    Ok(())
}

#[test]
fn a_shared_batch_payload_gives_each_flow_its_own_reference() -> TestResult {
    // Elixir: "flow_transition_many shared payload keeps per-flow refs and
    // values".
    let node = open(&directory("audit-batch-payload")?, 4)?;
    create_many(&node, "bulk-payload", &["a", "b"])?;
    let payload = map(&[("step", text("ready")), ("bytes", text(&"x".repeat(256)))]);
    transition_many(
        &node,
        "ready",
        fenced(&[("a", 0), ("b", 0)]),
        &[("payload", payload.clone())],
    )??;
    let refs: Vec<Term> = ["a", "b"]
        .iter()
        .map(|id| get(&node, id, &in_p()).map(|record| field(&record, "payload_ref")))
        .collect::<TestResult<_>>()?;
    assert_ne!(refs.first(), refs.get(1));
    for id in ["a", "b"] {
        let full = get(
            &node,
            id,
            &[("partition_key", text(P)), ("full", Term::Bool(true))],
        )?;
        assert_eq!(
            super::sorted(&field(&full, "payload")),
            super::sorted(&payload)
        );
    }
    let records = claim_all(&node, "bulk-payload", "ready")?;
    assert_eq!(records.len(), 2);
    assert!(
        records
            .iter()
            .all(|record| super::sorted(&field(record, "payload")) == super::sorted(&payload))
    );
    Ok(())
}

fn log_bytes(path: &std::path::Path) -> TestResult<u64> {
    let mut total = 0;
    for entry in std::fs::read_dir(path.join("log"))? {
        total += entry?.metadata()?.len();
    }
    Ok(total)
}

/// Elixir: "`flow_transition_many` shared payload is encoded once in the
/// replicated command" and "independent `flow_transition_many` shared
/// payload is encoded once per shard": 40 Flows take one 4 KiB payload and
/// the log grows by less than 20 of it; the Flows read it after replay.
fn a_shared_payload_is_logged_once(tag: &str, independent: bool) -> TestResult {
    let path = directory(tag)?;
    let node = open(&path, 4)?;
    let ids: Vec<String> = (1..=40).map(|index| format!("shared-{index}")).collect();
    let ids: Vec<&str> = ids.iter().map(String::as_str).collect();
    create_many(&node, "bulk-shared-log", &ids)?;
    let payload = text(&"x".repeat(4_096));
    let mut extra = vec![("payload", payload.clone())];
    if independent {
        extra.push(("independent", Term::Bool(true)));
    }
    let before = log_bytes(&path)?;
    let items = fenced(&ids.iter().map(|id| (*id, 0)).collect::<Vec<_>>());
    transition_many(&node, "ready", items, &extra)??;
    let logged = log_bytes(&path)?.saturating_sub(before);
    assert!(logged < 4_096 * 20, "the batch logged {logged} bytes");
    drop(node);
    let node = open(&path, 4)?;
    for id in ids {
        let full = get(
            &node,
            id,
            &[("partition_key", text(P)), ("full", Term::Bool(true))],
        )?;
        assert_eq!(field(&full, "payload"), payload, "{id}");
        assert_eq!(field(&full, "state"), text("ready"), "{id}");
    }
    drop(node);
    std::fs::remove_dir_all(path)?;
    Ok(())
}

#[test]
fn a_batch_s_shared_payload_is_logged_once() -> TestResult {
    a_shared_payload_is_logged_once("audit-batch-shared-log", false)
}

#[test]
fn an_independent_batch_s_shared_payload_is_logged_once_per_shard() -> TestResult {
    a_shared_payload_is_logged_once("audit-batch-shared-log-independent", true)
}

fn lease_items(records: &[Term], bump: Option<&str>) -> TestResult<Term> {
    let mut items = Vec::new();
    for record in records {
        let Term::Int(mut fence) = field(record, "fencing_token") else {
            return Err("no fence".into());
        };
        if bump.is_some_and(|id| field(record, "id") == text(id)) {
            fence = fence.saturating_add(1);
        }
        items.push(map(&[
            ("id", field(record, "id")),
            ("lease_token", field(record, "lease_token")),
            ("fencing_token", int(fence)),
        ]));
    }
    Ok(Term::List(items))
}

#[test]
fn a_completion_batch_finishes_a_partition_together_or_not_at_all() -> TestResult {
    // Elixir: "flow_complete_many atomically completes one-partition batch",
    // "... rolls back when any item fails guard" and "... spans shards and
    // rolls back failing shard group".
    let node = open(&directory("audit-batch-complete")?, 4)?;
    create_many(&node, "bulk-complete", &["a", "b"])?;
    let records = claim_all(&node, "bulk-complete", "queued")?;
    let done = [
        ("partition_key", text(P)),
        ("items", lease_items(&records, None)?),
        ("now_ms", int(2_001)),
    ];
    ok(&node, "FLOW.COMPLETE_MANY", &done)?;
    for id in ["a", "b"] {
        assert_eq!(state(&node, id)?, text("completed"));
    }

    create_many(&node, "bulk-complete-rollback", &["good", "bad"])?;
    let records = claim_all(&node, "bulk-complete-rollback", "queued")?;
    let stale = [
        ("partition_key", text(P)),
        ("items", lease_items(&records, Some("bad"))?),
        ("now_ms", int(2_001)),
    ];
    assert_eq!(
        super::refusal(run(&node, "FLOW.COMPLETE_MANY", &stale)?)?,
        "ERR stale flow lease"
    );
    for id in ["good", "bad"] {
        let record = get(&node, id, &in_p())?;
        assert_eq!(
            (field(&record, "state"), field(&record, "version")),
            (text("running"), int(2))
        );
    }
    Ok(())
}

#[test]
fn items_without_a_partition_go_to_their_own_auto_partition() -> TestResult {
    // Elixir: "flow many commands route auto-partition records by their real
    // state key".
    let node = open(&directory("audit-batch-auto")?, 4)?;
    ok(
        &node,
        "FLOW.CREATE_MANY",
        &[
            ("type", text("auto-many")),
            ("state", text("queued")),
            ("run_at_ms", int(1_000)),
            ("now_ms", int(900)),
            ("items", Term::List(vec![text("auto-a"), text("auto-b")])),
        ],
    )?;
    for id in ["auto-a", "auto-b"] {
        let record = get(&node, id, &[])?;
        assert!(
            matches!(field(&record, "partition_key"), Term::Bin(key) if key.starts_with(b"__flow_auto__"))
        );
    }
    let moved = [
        ("from_state", text("queued")),
        ("to_state", text("ready")),
        ("run_at_ms", int(1_500)),
        ("now_ms", int(1_100)),
        ("items", fenced(&[("auto-a", 0), ("auto-b", 0)])),
    ];
    ok(&node, "FLOW.TRANSITION_MANY", &moved)?;
    for id in ["auto-a", "auto-b"] {
        assert_eq!(field(&get(&node, id, &[])?, "state"), text("ready"));
    }
    let records = claimed(
        &node,
        &[
            ("type", text("auto-many")),
            ("state", text("ready")),
            ("worker", text("w")),
            ("limit", int(2)),
            ("now_ms", int(1_500)),
        ],
    )?;
    assert_eq!(records.len(), 2);
    let items = lease_items(&records, None)?;
    ok(
        &node,
        "FLOW.COMPLETE_MANY",
        &[("items", items), ("now_ms", int(1_600))],
    )?;
    for id in ["auto-a", "auto-b"] {
        assert_eq!(field(&get(&node, id, &[])?, "state"), text("completed"));
    }
    let _ = refusal;
    Ok(())
}
