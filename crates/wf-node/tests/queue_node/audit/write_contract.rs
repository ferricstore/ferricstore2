//! `flow_write_contract_test.exs`: write commands answer success only,
//! claims and reads answer the Flow, batch replies, step commands and input
//! limits that leave a Flow untouched.

use std::time::{SystemTime, UNIX_EPOCH};

use wf_application::commands::flow::Term;
use wf_node::composition::QueueNode;

use super::{claimed, field, get, int, map, ok, text};
use crate::{TestResult, directory, open};

pub(super) fn now() -> TestResult<i64> {
    Ok(i64::try_from(
        SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis(),
    )?)
}

pub(super) fn payload(step: &str) -> Term {
    map(&[("step", text(step))])
}

pub(super) fn int_of(term: &Term) -> TestResult<i64> {
    match term {
        Term::Int(value) => Ok(*value),
        other => Err(format!("not an integer: {other:?}").into()),
    }
}

/// Creates Flow `id` of `flow_type`, due at `now`, and claims it.
fn created_and_claimed(node: &QueueNode, id: &str, flow_type: &str, now: i64) -> TestResult<Term> {
    ok(
        node,
        "FLOW.CREATE",
        &[
            ("id", text(id)),
            ("type", text(flow_type)),
            ("payload", payload(flow_type)),
            ("run_at_ms", int(now)),
        ],
    )?;
    let records = claimed(
        node,
        &[
            ("type", text(flow_type)),
            ("limit", int(1)),
            ("now_ms", int(now)),
            ("worker", text("contract")),
        ],
    )?;
    let [record] = records.as_slice() else {
        return Err(format!("one claim expected, got {records:?}").into());
    };
    assert_eq!(field(record, "id"), text(id));
    Ok(record.clone())
}

/// The lease fields of a claimed record.
pub(super) fn lease(record: &Term) -> [(&'static str, Term); 3] {
    [
        ("id", field(record, "id")),
        ("lease_token", field(record, "lease_token")),
        ("fencing_token", field(record, "fencing_token")),
    ]
}

type Entries = Vec<(&'static str, Term)>;

pub(super) fn with(lease: [(&'static str, Term); 3], extra: &[(&'static str, Term)]) -> Entries {
    lease.into_iter().chain(extra.iter().cloned()).collect()
}

fn state(node: &QueueNode, id: &str, extra: &[(&str, Term)]) -> TestResult<Term> {
    Ok(field(&get(node, id, extra)?, "state"))
}

#[test]
fn singular_write_commands_return_success_only_while_claim_and_get_return_flow_data() -> TestResult
{
    // Elixir: "singular write commands return success only while claim/get
    // return flow data".
    let node = open(&directory("audit-write-singular")?, 4)?;
    let now = now()?;
    let ok_text = text("OK");
    let created = ok(
        &node,
        "FLOW.CREATE",
        &[
            ("id", text("queued")),
            ("type", text("contract-queued")),
            ("payload", payload("queued")),
            ("run_at_ms", int(now + 60_000)),
        ],
    )?;
    assert_eq!(created, ok_text);
    let queued = get(&node, "queued", &[])?;
    assert_eq!(field(&queued, "id"), text("queued"));
    assert!(matches!(field(&queued, "payload_ref"), Term::Bin(_)));

    let claim = created_and_claimed(&node, "cancel", "contract-cancel", now)?;
    let reply = ok(
        &node,
        "FLOW.CANCEL",
        &with(lease(&claim), &[("reason", text("contract"))]),
    )?;
    assert_eq!(reply, ok_text);
    assert_eq!(state(&node, "cancel", &[])?, text("cancelled"));

    let claim = created_and_claimed(&node, "complete", "contract-complete", now)?;
    assert!(matches!(field(&claim, "lease_token"), Term::Bin(_)));
    let result = map(&[("ok", Term::Bool(true))]);
    let reply = ok(
        &node,
        "FLOW.COMPLETE",
        &with(lease(&claim), &[("result", result.clone())]),
    )?;
    assert_eq!(reply, ok_text);
    assert_eq!(state(&node, "complete", &[])?, text("completed"));

    let claim = created_and_claimed(&node, "fail", "contract-fail", now)?;
    let error = map(&[("reason", text("contract"))]);
    let reply = ok(
        &node,
        "FLOW.FAIL",
        &with(lease(&claim), &[("error", error)]),
    )?;
    assert_eq!(reply, ok_text);
    assert_eq!(state(&node, "fail", &[])?, text("failed"));

    let claim = created_and_claimed(&node, "retry", "contract-retry", now)?;
    let reply = ok(
        &node,
        "FLOW.RETRY",
        &with(lease(&claim), &[("run_at_ms", int(now + 1_000))]),
    )?;
    assert_eq!(reply, ok_text);
    assert_eq!(state(&node, "retry", &[])?, text("queued"));

    let claim = created_and_claimed(&node, "transition", "contract-transition", now)?;
    let moved = with(
        lease(&claim),
        &[
            ("from_state", text("running")),
            ("to_state", text("waiting")),
            ("run_at_ms", int(now + 2_000)),
        ],
    );
    assert_eq!(ok(&node, "FLOW.TRANSITION", &moved)?, ok_text);
    assert_eq!(state(&node, "transition", &[])?, text("waiting"));

    ok(
        &node,
        "FLOW.CREATE",
        &[
            ("id", text("rewind")),
            ("type", text("contract-rewind")),
            ("payload", payload("rewind")),
            ("run_at_ms", int(now)),
        ],
    )?;
    let Term::List(history) = ok(
        &node,
        "FLOW.HISTORY",
        &[("id", text("rewind")), ("count", int(10))],
    )?
    else {
        return Err("history is not a list".into());
    };
    let [Term::List(created_event)] = history.as_slice() else {
        return Err(format!("one created event expected, got {history:?}").into());
    };
    let created_event_id = created_event.first().cloned().ok_or("no event id")?;
    let claim = created_and_claimed_existing(&node, "rewind", "contract-rewind", now)?;
    ok(
        &node,
        "FLOW.COMPLETE",
        &with(lease(&claim), &[("result", result)]),
    )?;
    let reply = ok(
        &node,
        "FLOW.REWIND",
        &[("id", text("rewind")), ("to_event", created_event_id)],
    )?;
    assert_eq!(reply, ok_text);
    assert_eq!(state(&node, "rewind", &[])?, text("queued"));
    Ok(())
}

/// Claims the one due Flow of `flow_type`, which must be `id`.
fn created_and_claimed_existing(
    node: &QueueNode,
    id: &str,
    flow_type: &str,
    now: i64,
) -> TestResult<Term> {
    let records = claimed(
        node,
        &[
            ("type", text(flow_type)),
            ("limit", int(1)),
            ("now_ms", int(now)),
            ("worker", text("contract")),
        ],
    )?;
    let [record] = records.as_slice() else {
        return Err(format!("one claim expected, got {records:?}").into());
    };
    assert_eq!(field(record, "id"), text(id));
    Ok(record.clone())
}

const P: &str = "contract-many";

/// Creates `ids` in partition `P` as one batch and claims them, in id order.
fn many_claimed(
    node: &QueueNode,
    ids: &[&str],
    flow_type: &str,
    now: i64,
) -> TestResult<Vec<Term>> {
    let items = Term::List(ids.iter().map(|id| text(id)).collect());
    ok(
        node,
        "FLOW.CREATE_MANY",
        &[
            ("partition_key", text(P)),
            ("type", text(flow_type)),
            ("run_at_ms", int(now)),
            ("items", items),
        ],
    )?;
    let mut records = claimed(
        node,
        &[
            ("type", text(flow_type)),
            ("partition_key", text(P)),
            ("limit", int(i64::try_from(ids.len())?)),
            ("now_ms", int(now)),
            ("worker", text("contract")),
        ],
    )?;
    assert_eq!(records.len(), ids.len());
    records.sort_by_key(|record| format!("{:?}", field(record, "id")));
    Ok(records)
}

/// Lease items of claimed records, as `[id, lease_token, fencing_token]`.
fn items(records: &[Term]) -> Term {
    Term::List(
        records
            .iter()
            .map(|record| {
                map(&[
                    ("id", field(record, "id")),
                    ("lease_token", field(record, "lease_token")),
                    ("fencing_token", field(record, "fencing_token")),
                ])
            })
            .collect(),
    )
}

fn assert_states(node: &QueueNode, ids: &[&str], expected: &str) -> TestResult {
    for id in ids {
        assert_eq!(
            state(node, id, &[("partition_key", text(P))])?,
            text(expected),
            "{id}"
        );
    }
    Ok(())
}

fn many(
    node: &QueueNode,
    name: &str,
    records: &[Term],
    extra: &[(&str, Term)],
) -> TestResult<Term> {
    let mut entries = vec![("partition_key", text(P)), ("items", items(records))];
    entries.extend(extra.iter().cloned());
    ok(node, name, &entries)
}

#[test]
fn batch_write_commands_return_success_only() -> TestResult {
    // Elixir: "batch write commands return success only while claim/get
    // return flow data".
    let node = open(&directory("audit-write-many")?, 4)?;
    let now = now()?;
    let created = ok(
        &node,
        "FLOW.CREATE_MANY",
        &[
            ("partition_key", text(P)),
            ("type", text("many-create")),
            ("payload", payload("create")),
            ("run_at_ms", int(now + 60_000)),
            (
                "items",
                Term::List(vec![text("create-a"), text("create-b")]),
            ),
        ],
    )?;
    assert_eq!(created, text("OK"));
    for id in ["create-a", "create-b"] {
        let record = get(&node, id, &[("partition_key", text(P))])?;
        assert_eq!(field(&record, "id"), text(id));
        assert_eq!(field(&record, "state"), text("queued"));
    }
    let cases: [(&str, &str, Entries, &str); 5] = [
        (
            "complete",
            "FLOW.COMPLETE_MANY",
            vec![("result", map(&[("ok", Term::Bool(true))]))],
            "completed",
        ),
        (
            "retry",
            "FLOW.RETRY_MANY",
            vec![("run_at_ms", int(now + 1_000))],
            "queued",
        ),
        (
            "fail",
            "FLOW.FAIL_MANY",
            vec![("error", map(&[("reason", text("contract"))]))],
            "failed",
        ),
        (
            "transition",
            "FLOW.TRANSITION_MANY",
            vec![
                ("from_state", text("running")),
                ("to_state", text("waiting")),
                ("run_at_ms", int(now + 2_000)),
            ],
            "waiting",
        ),
        (
            "cancel",
            "FLOW.CANCEL_MANY",
            vec![("reason", text("contract"))],
            "cancelled",
        ),
    ];
    for (name, command, extra, expected) in cases {
        let ids = [format!("{name}-a"), format!("{name}-b")];
        let ids: Vec<&str> = ids.iter().map(String::as_str).collect();
        let records = many_claimed(&node, &ids, &format!("many-{name}"), now)?;
        assert_eq!(
            many(&node, command, &records, &extra)?,
            text("OK"),
            "{command}"
        );
        assert_states(&node, &ids, expected)?;
    }
    Ok(())
}

#[test]
fn independent_complete_many_answers_each_item_or_one_ok() -> TestResult {
    // Elixir: "independent complete_many returns per-item success and
    // completes valid jobs", "... can return single ok on full success" and
    // "... ok-on-success preserves per-item failures".
    let node = open(&directory("audit-write-independent")?, 4)?;
    let now = now()?;
    let ids = ["ind-a", "ind-b"];
    let records = many_claimed(&node, &ids, "independent", now)?;
    let reply = many(
        &node,
        "FLOW.COMPLETE_MANY",
        &records,
        &[
            ("result", map(&[("ok", Term::Bool(true))])),
            ("independent", Term::Bool(true)),
        ],
    )?;
    assert_eq!(reply, Term::List(vec![text("OK"), text("OK")]));
    assert_states(&node, &ids, "completed")?;

    let ids = ["ok-a", "ok-b"];
    let records = many_claimed(&node, &ids, "independent-ok", now)?;
    let once = [
        ("independent", Term::Bool(true)),
        ("return", text("ok_on_success")),
    ];
    assert_eq!(
        many(&node, "FLOW.COMPLETE_MANY", &records, &once)?,
        text("OK")
    );
    assert_states(&node, &ids, "completed")?;

    let records = many_claimed(&node, &["err-a", "err-b"], "independent-err", now)?;
    let [bad, good] = records.as_slice() else {
        return Err("two claims".into());
    };
    let stale_fence = int(int_of(&field(bad, "fencing_token"))? + 1);
    let items = Term::List(vec![
        map(&[
            ("id", field(bad, "id")),
            ("lease_token", field(bad, "lease_token")),
            ("fencing_token", stale_fence),
        ]),
        map(&[
            ("id", field(good, "id")),
            ("lease_token", field(good, "lease_token")),
            ("fencing_token", field(good, "fencing_token")),
        ]),
    ]);
    let reply = ok(
        &node,
        "FLOW.COMPLETE_MANY",
        &[
            ("partition_key", text(P)),
            ("items", items),
            ("independent", Term::Bool(true)),
            ("return", text("ok_on_success")),
        ],
    )?;
    assert_eq!(
        reply,
        Term::List(vec![
            Term::List(vec![text("error"), text("ERR stale flow lease")]),
            text("ok"),
        ])
    );
    Ok(())
}
