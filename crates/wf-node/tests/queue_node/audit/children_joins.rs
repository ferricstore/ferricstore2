//! `flow_test/sections/flow_spawn_children_rejects_missing_wait_state_
//! waiting_children.exs`, second part: a resolved group moves a running
//! parent to its next state, due at once when that state is not terminal.

use wf_application::commands::flow::Term;
use wf_node::composition::QueueNode;

use super::children_rules::{P, child, claim_child, create, finish, in_p, path, spawn};
use super::{claimed, field, get, int, map, ok, text};
use crate::{TestResult, directory, open};

/// Claims the one due Flow of `flow_type` in `state` at `now`.
fn claim_state(node: &QueueNode, flow_type: &str, state: &str, now: i64) -> TestResult<Vec<Term>> {
    claimed(
        node,
        &[
            ("type", text(flow_type)),
            ("state", text(state)),
            ("partition_key", text(P)),
            ("worker", text("worker-join")),
            ("lease_ms", int(30_000)),
            ("limit", int(1)),
            ("now_ms", int(now)),
        ],
    )
}

/// A running parent of `flow_type` claimed at `now` spawns `children`
/// into a group ending in `success` or `failure`.
fn running_parent(
    node: &QueueNode,
    flow_type: &str,
    (now, children): (i64, Vec<Term>),
    (wait, success, failure): (&str, &str, &str),
) -> TestResult {
    create(node, "parent", flow_type, "queued", now)?;
    let records = claim_state(node, flow_type, "queued", now)?;
    let [lease] = records.as_slice() else {
        return Err("one parent claim".into());
    };
    spawn(
        node,
        "parent",
        children,
        &[
            ("group_id", text("join")),
            ("wait", text(wait)),
            (
                "on_child_failed",
                text(if wait == "any" {
                    "ignore"
                } else {
                    "fail_parent"
                }),
            ),
            (
                "exhaust_to",
                map(&[("success", text(success)), ("failure", text(failure))]),
            ),
            ("from_state", text("running")),
            ("lease_token", field(lease, "lease_token")),
            ("fencing_token", field(lease, "fencing_token")),
            ("now_ms", int(now + 1)),
        ],
    )??;
    Ok(())
}

fn due_child(id: &str, at: i64) -> Term {
    map(&[
        ("id", text(id)),
        ("type", text("child")),
        ("run_at_ms", int(at)),
    ])
}

#[test]
fn a_resolved_join_makes_the_parent_due_in_its_next_state() -> TestResult {
    // Elixir: "resolved nonterminal child join schedules the parent for a
    // due claim".
    let node = open(&directory("audit-join-due")?, 4)?;
    create(&node, "parent", "join-due", "queued", 1_000)?;
    let records = claim_state(&node, "join-due", "queued", 1_000)?;
    let [first] = records.as_slice() else {
        return Err("one claim".into());
    };
    ok(
        &node,
        "FLOW.RETRY",
        &[
            ("id", text("parent")),
            ("partition_key", text(P)),
            ("lease_token", field(first, "lease_token")),
            ("fencing_token", field(first, "fencing_token")),
            ("error", text("retry before child join")),
            ("run_at_ms", int(1_010)),
            ("now_ms", int(1_005)),
        ],
    )?;
    assert_eq!(field(&get(&node, "parent", &in_p())?, "attempts"), int(1));
    let records = claim_state(&node, "join-due", "queued", 1_010)?;
    let [lease] = records.as_slice() else {
        return Err("one claim after retry".into());
    };
    spawn(
        &node,
        "parent",
        vec![due_child("kid", 1_000)],
        &[
            ("group_id", text("join-due")),
            (
                "exhaust_to",
                map(&[
                    ("success", text("native_join_next")),
                    ("failure", text("native_join_failed")),
                ]),
            ),
            ("from_state", text("running")),
            ("lease_token", field(lease, "lease_token")),
            ("fencing_token", field(lease, "fencing_token")),
            ("now_ms", int(1_010)),
        ],
    )??;
    let kid = claim_child(&node, "kid", "worker-join-child")?;
    finish(&node, "FLOW.COMPLETE", &kid, &[("now_ms", int(2_000))])??;
    let joined = get(&node, "parent", &in_p())?;
    assert_eq!(field(&joined, "state"), text("native_join_next"));
    assert_eq!(field(&joined, "run_state"), text("queued"));
    assert_eq!(field(&joined, "attempts"), int(1));
    assert_eq!(field(&joined, "next_run_at_ms"), int(2_000));
    let records = claim_state(&node, "join-due", "native_join_next", 2_000)?;
    let [successor] = records.as_slice() else {
        return Err("the joined parent is not due".into());
    };
    assert_eq!(field(successor, "id"), text("parent"));
    assert_eq!(field(successor, "state"), text("running"));
    assert_eq!(field(successor, "run_state"), text("native_join_next"));
    assert_eq!(field(successor, "attempts"), int(1));
    Ok(())
}

#[test]
fn wait_any_keeps_the_parent_until_a_child_succeeds() -> TestResult {
    // Elixir: "wait any keeps a parent blocked until an unresolved sibling
    // closes".
    let node = open(&directory("audit-join-any")?, 4)?;
    let kids = vec![child("failing"), child("winning")];
    running_parent(
        &node,
        "join-any",
        (4_000, kids),
        ("any", "join_any_next", "join_any_failed"),
    )?;
    let failing = claim_child(&node, "failing", "worker-fail")?;
    let winning = claim_child(&node, "winning", "worker-win")?;
    let failure = [("error", text("wait any failure")), ("now_ms", int(4_010))];
    finish(&node, "FLOW.FAIL", &failing, &failure)??;
    assert_eq!(
        field(&get(&node, "failing", &in_p())?, "state"),
        text("failed")
    );
    let blocked = get(&node, "parent", &in_p())?;
    assert_eq!(field(&blocked, "state"), text("waiting_children"));
    assert_eq!(field(&blocked, "run_state"), text("queued"));
    assert_eq!(field(&blocked, "next_run_at_ms"), Term::Nil);
    let group = path(&blocked, &["child_groups", "join"]);
    assert_eq!(field(&group, "resolved"), Term::Nil);
    assert_eq!(path(&group, &["children", "failing"]), text("failed"));
    assert_eq!(path(&group, &["children", "winning"]), text("running"));
    assert_eq!(
        claim_state(&node, "join-any", "join_any_next", 4_010)?,
        Vec::<Term>::new()
    );
    finish(&node, "FLOW.COMPLETE", &winning, &[("now_ms", int(4_020))])??;
    let joined = get(&node, "parent", &in_p())?;
    assert_eq!(field(&joined, "state"), text("join_any_next"));
    assert_eq!(field(&joined, "run_state"), text("queued"));
    assert_eq!(field(&joined, "next_run_at_ms"), int(4_020));
    let records = claim_state(&node, "join-any", "join_any_next", 4_020)?;
    let [successor] = records.as_slice() else {
        return Err("the joined parent is not due".into());
    };
    assert_eq!(field(successor, "id"), text("parent"));
    assert_eq!(field(successor, "run_state"), text("join_any_next"));
    Ok(())
}

#[test]
fn a_joined_parent_is_found_by_a_claim_across_shards() -> TestResult {
    // Elixir: "resolved nonterminal child join is included in a cross-shard
    // due scan".
    let node = open(&directory("audit-join-scan")?, 4)?;
    let kids = vec![child("kid")];
    running_parent(
        &node,
        "join-scan",
        (5_000, kids),
        ("all", "cross_shard_next", "cross_shard_failed"),
    )?;
    ok(
        &node,
        "FLOW.CREATE",
        &[
            ("id", text("decoy")),
            ("type", text("join-scan")),
            ("state", text("queued")),
            ("partition_key", text("other-tenant")),
            ("run_at_ms", int(5_020)),
            ("now_ms", int(5_020)),
        ],
    )?;
    let kid = claim_child(&node, "kid", "worker-kid")?;
    finish(&node, "FLOW.COMPLETE", &kid, &[("now_ms", int(5_020))])??;
    let joined = get(&node, "parent", &in_p())?;
    assert_eq!(field(&joined, "state"), text("cross_shard_next"));
    assert_eq!(field(&joined, "next_run_at_ms"), int(5_020));
    let records = claimed(
        &node,
        &[
            ("type", text("join-scan")),
            ("partition_key", text("any")),
            ("state", text("any")),
            ("worker", text("worker-scan")),
            ("lease_ms", int(30_000)),
            ("limit", int(2)),
            ("now_ms", int(5_020)),
        ],
    )?;
    let mut seen: Vec<(Term, Term)> = records
        .iter()
        .map(|record| (field(record, "id"), field(record, "run_state")))
        .collect();
    seen.sort_by_key(|pair| format!("{pair:?}"));
    assert_eq!(
        seen,
        [
            (text("decoy"), text("queued")),
            (text("parent"), text("cross_shard_next"))
        ]
    );
    Ok(())
}

/// A join resolved by `finished` (`FLOW.COMPLETE` or `FLOW.FAIL`) moves the
/// parent to the group's success or failure state; due only when it is not
/// terminal.
fn join_case(tag: &str, finished: &str, (success, failure): (&str, &str), due: bool) -> TestResult {
    let node = open(&directory(tag)?, 4)?;
    let kids = vec![due_child("kid", 10_000)];
    running_parent(
        &node,
        "join-controls",
        (10_000, kids),
        ("all", success, failure),
    )?;
    let kid = claim_child(&node, "kid", "worker-kid")?;
    let extra = [
        ("error", text("join controls failure")),
        ("now_ms", int(10_010)),
    ];
    let extra = if finished == "FLOW.FAIL" {
        &extra[..]
    } else {
        &extra[1..]
    };
    finish(&node, finished, &kid, extra)??;
    let expected = if finished == "FLOW.COMPLETE" {
        success
    } else {
        failure
    };
    let joined = get(&node, "parent", &in_p())?;
    assert_eq!(field(&joined, "state"), text(expected));
    assert_eq!(field(&joined, "run_state"), text("queued"));
    let successors = claim_state(&node, "join-controls", expected, 10_010)?;
    if due {
        assert_eq!(field(&joined, "next_run_at_ms"), int(10_010));
        let [successor] = successors.as_slice() else {
            return Err("the joined parent is not due".into());
        };
        assert_eq!(field(successor, "run_state"), text(expected));
    } else {
        assert_eq!(field(&joined, "next_run_at_ms"), Term::Nil);
        assert_eq!(successors, Vec::<Term>::new());
    }
    Ok(())
}

#[test]
fn a_join_s_terminal_outcome_is_not_due_and_a_nonterminal_one_is() -> TestResult {
    // Elixir: "resolved failed child join schedules a nonterminal failure
    // successor", "resolved successful child join leaves terminal success
    // out of the due index" and "resolved failed child join leaves terminal
    // failure out of the due index".
    join_case(
        "audit-join-fail-next",
        "FLOW.FAIL",
        ("join_success_next", "join_failure_next"),
        true,
    )?;
    join_case(
        "audit-join-success-terminal",
        "FLOW.COMPLETE",
        ("completed", "failed"),
        false,
    )?;
    join_case(
        "audit-join-failure-terminal",
        "FLOW.FAIL",
        ("completed", "failed"),
        false,
    )
}

#[test]
fn complete_many_resolves_a_group_of_its_children() -> TestResult {
    // Elixir: "flow_complete_many resolves colocated child groups".
    let node = open(&directory("audit-kids-many")?, 4)?;
    let parent = create(&node, "parent", "parent", "dispatch", 1_000)?;
    spawn(
        &node,
        "parent",
        vec![child("a"), child("b")],
        &[("fencing_token", field(&parent, "fencing_token"))],
    )??;
    let a = claim_child(&node, "a", "worker-cross-a")?;
    let b = claim_child(&node, "b", "worker-cross-b")?;
    let item = |lease: &Term| {
        map(&[
            ("id", field(lease, "id")),
            ("partition_key", text(P)),
            ("lease_token", field(lease, "lease_token")),
            ("fencing_token", field(lease, "fencing_token")),
        ])
    };
    ok(
        &node,
        "FLOW.COMPLETE_MANY",
        &[
            ("items", Term::List(vec![item(&a), item(&b)])),
            ("now_ms", int(8_000)),
        ],
    )?;
    for id in ["a", "b"] {
        assert_eq!(field(&get(&node, id, &in_p())?, "state"), text("completed"));
    }
    let done = get(&node, "parent", &in_p())?;
    assert_eq!(field(&done, "state"), text("children_done"));
    let group = path(&done, &["child_groups", "fanout"]);
    assert_eq!(field(&group, "resolved"), text("success"));
    assert_eq!(path(&group, &["summary", "completed"]), int(2));
    Ok(())
}
