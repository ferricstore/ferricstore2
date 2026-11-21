//! `flow_test/sections/flow_spawn_children_rejects_missing_wait_state_
//! waiting_children.exs`, first part: what a spawn refuses, idempotent
//! groups, how a group resolves and closes, and what a restart keeps.

use wf_application::commands::flow::Term;
use wf_node::composition::QueueNode;

use super::{claimed, events, field, get, int, map, ok, run, text};
use crate::{TestResult, directory, open};

pub(super) const P: &str = "tenant";

pub(super) fn in_p() -> [(&'static str, Term); 1] {
    [("partition_key", text(P))]
}

pub(super) fn path(term: &Term, keys: &[&str]) -> Term {
    keys.iter()
        .fold(term.clone(), |value, key| field(&value, key))
}

/// Creates `id` of `flow_type` in `state` in `P`; its record.
pub(super) fn create(
    node: &QueueNode,
    id: &str,
    flow_type: &str,
    state: &str,
    now: i64,
) -> TestResult<Term> {
    ok(
        node,
        "FLOW.CREATE",
        &[
            ("id", text(id)),
            ("type", text(flow_type)),
            ("state", text(state)),
            ("partition_key", text(P)),
            ("run_at_ms", int(now)),
            ("now_ms", int(now)),
        ],
    )?;
    get(node, id, &in_p())
}

pub(super) fn child(id: &str) -> Term {
    map(&[("id", text(id)), ("type", text("child"))])
}

/// Elixir's usual group options; `extra` adds or replaces them.
pub(super) fn spawn(
    node: &QueueNode,
    parent: &str,
    children: Vec<Term>,
    extra: &[(&str, Term)],
) -> TestResult<Result<Term, String>> {
    let mut entries = vec![
        ("id", text(parent)),
        ("partition_key", text(P)),
        ("children", Term::List(children)),
        ("group_id", text("fanout")),
        ("wait", text("all")),
        ("wait_state", text("waiting_children")),
        ("on_child_failed", text("fail_parent")),
        ("on_parent_closed", text("cancel_children")),
        (
            "exhaust_to",
            map(&[
                ("success", text("children_done")),
                ("failure", text("failed")),
            ]),
        ),
        ("from_state", text("dispatch")),
    ];
    entries.retain(|(key, _)| !extra.iter().any(|(given, _)| given == key));
    // A nil option is one left out, as Elixir's tests leave it.
    entries.extend(
        extra
            .iter()
            .filter(|(_, value)| *value != Term::Nil)
            .cloned(),
    );
    Ok(run(node, "FLOW.SPAWN_CHILDREN", &entries)?.map_err(|error| error.to_string()))
}

/// Claims child `id` of type `child` in `P`, as Elixir's helper does.
pub(super) fn claim_child(node: &QueueNode, id: &str, worker: &str) -> TestResult<Term> {
    let records = claimed(
        node,
        &[
            ("type", text("child")),
            ("partition_key", text(P)),
            ("worker", text(worker)),
            ("limit", int(1)),
            ("now_ms", int(9_000_000_000_000)),
        ],
    )?;
    let [record] = records.as_slice() else {
        return Err(format!("one child claim expected, got {records:?}").into());
    };
    assert_eq!(field(record, "id"), text(id));
    Ok(record.clone())
}

/// Completes (`FLOW.COMPLETE`) or fails (`FLOW.FAIL`) a claimed Flow.
pub(super) fn finish(
    node: &QueueNode,
    name: &str,
    lease: &Term,
    extra: &[(&str, Term)],
) -> TestResult<Result<Term, String>> {
    let mut entries = vec![
        ("id", field(lease, "id")),
        ("partition_key", text(P)),
        ("lease_token", field(lease, "lease_token")),
        ("fencing_token", field(lease, "fencing_token")),
    ];
    entries.extend(extra.iter().cloned());
    Ok(run(node, name, &entries)?.map_err(|error| error.to_string()))
}

#[test]
fn a_spawn_that_waits_needs_a_wait_state_and_changes_nothing_without_one() -> TestResult {
    // Elixir: "flow_spawn_children rejects missing wait_state when waiting
    // for children" and "... rejects empty wait_state for running parent".
    let node = open(&directory("audit-kids-wait-state")?, 4)?;
    let parent = create(&node, "parent", "parent", "dispatch", 1_000)?;
    let missing = spawn(
        &node,
        "parent",
        vec![child("kid")],
        &[
            ("wait_state", Term::Nil),
            ("fencing_token", field(&parent, "fencing_token")),
        ],
    )?;
    let required = Err("ERR flow wait_state is required when waiting for children".to_owned());
    assert_eq!(missing, required);
    assert_eq!(get(&node, "kid", &in_p())?, Term::Nil);
    let unchanged = get(&node, "parent", &in_p())?;
    assert_eq!(field(&unchanged, "state"), text("dispatch"));
    assert_eq!(field(&unchanged, "child_groups"), Term::Map(Vec::new()));

    create(&node, "runner", "parent-running", "queued", 1_000)?;
    let records = claimed(
        &node,
        &[
            ("type", text("parent-running")),
            ("partition_key", text(P)),
            ("worker", text("parent-worker")),
            ("limit", int(1)),
            ("now_ms", int(1_000)),
        ],
    )?;
    let [lease] = records.as_slice() else {
        return Err("one claim".into());
    };
    let empty = spawn(
        &node,
        "runner",
        vec![child("kid2")],
        &[
            ("wait_state", text("")),
            ("from_state", Term::Nil),
            ("lease_token", field(lease, "lease_token")),
            ("fencing_token", field(lease, "fencing_token")),
        ],
    )?;
    assert_eq!(empty, required);
    assert_eq!(get(&node, "kid2", &in_p())?, Term::Nil);
    let still = get(&node, "runner", &in_p())?;
    assert_eq!(field(&still, "state"), text("running"));
    assert_eq!(field(&still, "lease_token"), field(lease, "lease_token"));
    Ok(())
}

#[test]
fn a_repeated_child_completion_counts_once() -> TestResult {
    // Elixir: "stale child terminal command does not double count parent
    // child group".
    let node = open(&directory("audit-kids-stale")?, 4)?;
    let parent = create(&node, "parent", "parent", "dispatch", 3_000)?;
    let fence = [
        ("fencing_token", field(&parent, "fencing_token")),
        ("now_ms", int(3_010)),
    ];
    spawn(&node, "parent", vec![child("a"), child("b")], &fence)??;
    let lease = claim_child(&node, "a", "worker-a")?;
    finish(&node, "FLOW.COMPLETE", &lease, &[("now_ms", int(3_020))])??;
    let first = get(&node, "parent", &in_p())?;
    assert_eq!(
        path(&first, &["child_groups", "fanout", "summary", "completed"]),
        int(1)
    );
    finish(&node, "FLOW.COMPLETE", &lease, &[("now_ms", int(3_030))])??;
    let after = get(&node, "parent", &in_p())?;
    assert_eq!(field(&after, "version"), field(&first, "version"));
    let group = path(&after, &["child_groups", "fanout"]);
    assert_eq!(path(&group, &["summary", "completed"]), int(1));
    assert_eq!(path(&group, &["children", "a"]), text("completed"));
    assert_eq!(path(&group, &["children", "b"]), text("running"));
    Ok(())
}

#[test]
fn a_nested_completion_resolves_the_parent_and_the_grandparent() -> TestResult {
    // Elixir: "nested child completion resolves parent and propagates to
    // grandparent" and "colocated nested child completion resolves parent
    // and grandparent".
    let node = open(&directory("audit-kids-nested")?, 4)?;
    let done = map(&[("success", text("completed")), ("failure", text("failed"))]);
    let top = create(&node, "grand", "parent", "dispatch", 4_000)?;
    let middle = map(&[
        ("id", text("middle")),
        ("type", text("child")),
        ("state", text("dispatch")),
    ]);
    spawn(
        &node,
        "grand",
        vec![middle],
        &[
            ("group_id", text("outer")),
            ("wait_state", text("waiting_middle")),
            ("exhaust_to", done.clone()),
            ("fencing_token", field(&top, "fencing_token")),
            ("now_ms", int(4_010)),
        ],
    )??;
    let middle = get(&node, "middle", &in_p())?;
    spawn(
        &node,
        "middle",
        vec![child("leaf")],
        &[
            ("group_id", text("inner")),
            ("wait_state", text("waiting_leaf")),
            ("exhaust_to", done),
            ("fencing_token", field(&middle, "fencing_token")),
            ("now_ms", int(4_020)),
        ],
    )??;
    let lease = claim_child(&node, "leaf", "worker-leaf")?;
    finish(&node, "FLOW.COMPLETE", &lease, &[("now_ms", int(4_030))])??;
    let middle = get(&node, "middle", &in_p())?;
    assert_eq!(field(&middle, "state"), text("completed"));
    assert_eq!(
        path(&middle, &["child_groups", "inner", "resolved"]),
        text("success")
    );
    let grand = get(&node, "grand", &in_p())?;
    assert_eq!(field(&grand, "state"), text("completed"));
    let outer = path(&grand, &["child_groups", "outer"]);
    assert_eq!(field(&outer, "resolved"), text("success"));
    assert_eq!(path(&outer, &["children", "middle"]), text("completed"));
    assert_eq!(path(&outer, &["summary", "completed"]), int(1));
    Ok(())
}

#[test]
fn a_failed_child_fails_its_parent_or_is_ignored() -> TestResult {
    // Elixir: "flow_spawn_children can fail parent on child failure or
    // ignore terminal failures".
    let node = open(&directory("audit-kids-failure")?, 4)?;
    let exhaust = map(&[
        ("success", text("children_done")),
        ("failure", text("children_failed")),
    ]);
    let parent = create(&node, "fail-parent", "parent", "dispatch", 1_000)?;
    let options = [
        ("exhaust_to", exhaust.clone()),
        ("fencing_token", field(&parent, "fencing_token")),
    ];
    spawn(&node, "fail-parent", vec![child("fail-kid")], &options)??;
    let lease = claim_child(&node, "fail-kid", "worker-fail")?;
    finish(&node, "FLOW.FAIL", &lease, &[("error", text("boom"))])??;
    let failed = get(&node, "fail-parent", &in_p())?;
    assert_eq!(field(&failed, "state"), text("children_failed"));
    let group = path(&failed, &["child_groups", "fanout"]);
    assert_eq!(field(&group, "resolved"), text("failure"));
    let result = path(&group, &["results", "fail-kid"]);
    assert_eq!(field(&result, "status"), text("failed"));
    assert!(matches!(field(&result, "error_ref"), Term::Bin(_)));

    let parent = create(&node, "ignore-parent", "parent", "dispatch", 1_000)?;
    let options = [
        ("exhaust_to", exhaust),
        ("on_child_failed", text("ignore")),
        ("on_parent_closed", text("abandon_children")),
        ("fencing_token", field(&parent, "fencing_token")),
    ];
    spawn(&node, "ignore-parent", vec![child("ignore-kid")], &options)??;
    let lease = claim_child(&node, "ignore-kid", "worker-ignore")?;
    finish(&node, "FLOW.FAIL", &lease, &[("error", text("ignored"))])??;
    let ignored = get(&node, "ignore-parent", &in_p())?;
    assert_eq!(field(&ignored, "state"), text("children_done"));
    let group = path(&ignored, &["child_groups", "fanout"]);
    assert_eq!(field(&group, "resolved"), text("success"));
    assert_eq!(path(&group, &["summary", "failed"]), int(1));
    Ok(())
}

#[test]
fn cancelling_a_parent_cancels_its_children_and_records_them() -> TestResult {
    // Elixir: "flow_cancel parent cancels direct running children when
    // configured" and "terminal parent cancellation cancels colocated
    // children".
    let node = open(&directory("audit-kids-cancel")?, 4)?;
    let parent = create(&node, "parent", "parent", "dispatch", 1_000)?;
    let fence = [("fencing_token", field(&parent, "fencing_token"))];
    spawn(&node, "parent", vec![child("a"), child("b")], &fence)??;
    let waiting = get(&node, "parent", &in_p())?;
    ok(
        &node,
        "FLOW.CANCEL",
        &[
            ("id", text("parent")),
            ("partition_key", text(P)),
            ("fencing_token", field(&waiting, "fencing_token")),
            ("reason", text("parent closed")),
        ],
    )?;
    for id in ["a", "b"] {
        assert_eq!(
            field(&get(&node, id, &in_p())?, "state"),
            text("cancelled"),
            "{id}"
        );
    }
    let closed = get(&node, "parent", &in_p())?;
    let group = path(&closed, &["child_groups", "fanout"]);
    assert_eq!(field(&group, "resolved"), text("failure"));
    assert_eq!(path(&group, &["summary", "cancelled"]), int(2));
    for id in ["a", "b"] {
        assert_eq!(path(&group, &["results", id, "status"]), text("cancelled"));
        assert_eq!(path(&group, &["children", id]), text("cancelled"));
    }
    Ok(())
}

#[test]
fn a_group_spawn_is_idempotent_and_refuses_a_different_group() -> TestResult {
    // Elixir: "flow_spawn_children is idempotent by group id and rejects
    // conflicts", "... idempotency includes child max_active_ms" and "...
    // remains idempotent after child progress".
    let node = open(&directory("audit-kids-idempotent")?, 4)?;
    let quiet = |fence: Term| {
        vec![
            ("on_child_failed", text("ignore")),
            ("on_parent_closed", text("abandon_children")),
            (
                "exhaust_to",
                map(&[
                    ("success", text("children_done")),
                    ("failure", text("children_failed")),
                ]),
            ),
            ("fencing_token", fence),
        ]
    };
    let parent = create(&node, "p1", "parent", "dispatch", 1_000)?;
    let options = quiet(field(&parent, "fencing_token"));
    spawn(&node, "p1", vec![child("c1")], &options)??;
    let first = get(&node, "p1", &in_p())?;
    spawn(&node, "p1", vec![child("c1")], &options)??;
    let same = get(&node, "p1", &in_p())?;
    assert_eq!(field(&same, "version"), field(&first, "version"));
    let conflict = Err("ERR flow child group idempotency conflict".to_owned());
    assert_eq!(
        spawn(&node, "p1", vec![child("other")], &options)?,
        conflict
    );

    let parent = create(&node, "p2", "parent", "dispatch", 1_000)?;
    let options = quiet(field(&parent, "fencing_token"));
    let limited = |ms: i64| {
        map(&[
            ("id", text("c2")),
            ("type", text("child")),
            ("max_active_ms", int(ms)),
        ])
    };
    spawn(&node, "p2", vec![limited(1_000)], &options)??;
    assert_eq!(
        spawn(&node, "p2", vec![limited(2_000)], &options)?,
        conflict
    );
    assert_eq!(
        field(&get(&node, "c2", &in_p())?, "max_active_ms"),
        int(1_000)
    );

    // Its own node, so the claims below meet only its children.
    let node = open(&directory("audit-kids-idempotent-progress")?, 4)?;
    let parent = create(&node, "p3", "parent", "dispatch", 1_000)?;
    let options = quiet(field(&parent, "fencing_token"));
    spawn(&node, "p3", vec![child("c3a"), child("c3b")], &options)??;
    let waiting = get(&node, "p3", &in_p())?;
    let lease = claim_child(&node, "c3a", "worker-a")?;
    finish(&node, "FLOW.COMPLETE", &lease, &[])??;
    spawn(&node, "p3", vec![child("c3a"), child("c3b")], &options)??;
    // Children in another order are the same group (Elixir sorts them).
    spawn(&node, "p3", vec![child("c3b"), child("c3a")], &options)??;
    let again = get(&node, "p3", &in_p())?;
    assert_eq!(field(&again, "id"), field(&waiting, "id"));
    let group = path(&again, &["child_groups", "fanout"]);
    assert_eq!(path(&group, &["summary", "completed"]), int(1));
    assert_eq!(path(&group, &["children", "c3a"]), text("completed"));
    Ok(())
}

#[test]
fn a_terminal_parent_spawns_no_new_group() -> TestResult {
    // Elixir: "flow_spawn_children rejects new groups on terminal parents".
    let node = open(&directory("audit-kids-terminal")?, 4)?;
    let parent = create(&node, "parent", "parent", "dispatch", 1_000)?;
    ok(
        &node,
        "FLOW.CANCEL",
        &[
            ("id", text("parent")),
            ("partition_key", text(P)),
            ("fencing_token", field(&parent, "fencing_token")),
        ],
    )?;
    let cancelled = get(&node, "parent", &in_p())?;
    assert_eq!(field(&cancelled, "state"), text("cancelled"));
    let refused = spawn(
        &node,
        "parent",
        vec![child("kid")],
        &[
            ("from_state", text("cancelled")),
            ("fencing_token", field(&cancelled, "fencing_token")),
        ],
    )?;
    assert_eq!(refused, Err("ERR flow parent is terminal".to_owned()));
    Ok(())
}

#[test]
fn a_parent_that_closes_cancels_its_other_open_groups() -> TestResult {
    // Elixir: "child group resolution cancels other open child groups when
    // parent closes".
    let node = open(&directory("audit-kids-close-groups")?, 4)?;
    let parent = create(&node, "parent", "parent", "dispatch", 1_000)?;
    spawn(
        &node,
        "parent",
        vec![child("failing")],
        &[
            ("group_id", text("fanout-a")),
            ("fencing_token", field(&parent, "fencing_token")),
        ],
    )??;
    spawn(
        &node,
        "parent",
        vec![child("sibling")],
        &[
            ("group_id", text("fanout-b")),
            ("on_child_failed", text("ignore")),
            ("from_state", text("waiting_children")),
            (
                "fencing_token",
                field(&get(&node, "parent", &in_p())?, "fencing_token"),
            ),
        ],
    )??;
    let lease = claim_child(&node, "failing", "worker-fail")?;
    finish(&node, "FLOW.FAIL", &lease, &[("error", text("boom"))])??;
    let closed = get(&node, "parent", &in_p())?;
    assert_eq!(field(&closed, "state"), text("failed"));
    assert_eq!(
        path(&closed, &["child_groups", "fanout-a", "resolved"]),
        text("failure")
    );
    let second = path(&closed, &["child_groups", "fanout-b"]);
    assert_eq!(path(&second, &["children", "sibling"]), text("cancelled"));
    assert_eq!(field(&second, "resolved"), text("failure"));
    assert_eq!(
        field(&get(&node, "sibling", &in_p())?, "state"),
        text("cancelled")
    );
    Ok(())
}

#[test]
fn a_child_outside_its_parent_s_slot_is_refused() -> TestResult {
    // Elixir: "flow_spawn_children rejects child partition overrides across
    // Raft groups" and "Raft routing rejects child dependencies across
    // independent groups".
    let node = open(&directory("audit-kids-slot")?, 4)?;
    let parent = create(&node, "parent", "parent", "dispatch", 1_000)?;
    let far = map(&[
        ("id", text("far")),
        ("type", text("child")),
        ("partition_key", text("elsewhere")),
    ]);
    let refused = spawn(
        &node,
        "parent",
        vec![far],
        &[("fencing_token", field(&parent, "fencing_token"))],
    )?;
    assert_eq!(
        refused,
        Err("CROSSSLOT Flow dependency keys must hash to the same shard".to_owned())
    );
    let unchanged = get(&node, "parent", &in_p())?;
    assert_eq!(field(&unchanged, "state"), text("dispatch"));
    assert_eq!(
        field(&unchanged, "fencing_token"),
        field(&parent, "fencing_token")
    );
    assert_eq!(
        get(&node, "far", &[("partition_key", text("elsewhere"))])?,
        Term::Nil
    );
    Ok(())
}

#[test]
fn a_resolved_group_survives_a_restart_without_counting_twice() -> TestResult {
    // Elixir: "colocated child completion survives Ra shard restart without
    // duplicate parent summary" (a reopen replays the node's log).
    let path_ = directory("audit-kids-restart")?;
    {
        let node = open(&path_, 4)?;
        let parent = create(&node, "parent", "parent", "dispatch", 1_000)?;
        spawn(
            &node,
            "parent",
            vec![child("kid")],
            &[("fencing_token", field(&parent, "fencing_token"))],
        )??;
        let lease = claim_child(&node, "kid", "worker-replay")?;
        finish(&node, "FLOW.COMPLETE", &lease, &[("result", text("ok"))])??;
        let resolved = get(&node, "parent", &in_p())?;
        assert_eq!(field(&resolved, "state"), text("children_done"));
        assert_eq!(
            path(
                &resolved,
                &["child_groups", "fanout", "summary", "completed"]
            ),
            int(1)
        );
    }
    let node = open(&path_, 3)?;
    let replayed = get(&node, "parent", &in_p())?;
    assert_eq!(field(&replayed, "state"), text("children_done"));
    let group = path(&replayed, &["child_groups", "fanout"]);
    assert_eq!(path(&group, &["children", "kid"]), text("completed"));
    assert_eq!(path(&group, &["summary", "completed"]), int(1));
    let child_completed = events(&node, "parent", &in_p())?
        .iter()
        .filter(|fields| field(fields, "event") == text("child_completed"))
        .count();
    assert_eq!(child_completed, 1);
    Ok(())
}
