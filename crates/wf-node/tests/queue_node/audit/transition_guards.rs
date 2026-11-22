//! `flow_test/sections/flow_fail_many_atomically_fails_one_partition_batch.exs`,
//! its single-Flow commands: a transition checks the state it leaves and a
//! running Flow's lease, never enters a terminal state and refuses a state
//! name over 64 bytes; `FLOW.FAIL` and `FLOW.CANCEL` finish a Flow and drop
//! its due work; a batch retry that exhausts settles the parent's group.

use wf_application::commands::flow::Term;
use wf_node::composition::QueueNode;

use super::children_rules::{P, child, claim_child, create, in_p, path, spawn};
use super::{claimed, field, get, int, map, ok, refusal, run, text};
use crate::{TestResult, directory, open};

fn claim_one(node: &QueueNode, flow_type: &str, now: i64) -> TestResult<Vec<Term>> {
    claimed(
        node,
        &[
            ("type", text(flow_type)),
            ("state", text("queued")),
            ("worker", text("worker-a")),
            ("lease_ms", int(30_000)),
            ("limit", int(10)),
            ("now_ms", int(now)),
        ],
    )
}

fn transition(
    node: &QueueNode,
    id: &str,
    from: &str,
    to: &str,
    extra: &[(&str, Term)],
) -> TestResult<Result<Term, String>> {
    let mut entries = vec![
        ("id", text(id)),
        ("from_state", text(from)),
        ("to_state", text(to)),
        ("run_at_ms", int(2_000)),
    ];
    entries.extend(extra.iter().cloned());
    Ok(run(node, "FLOW.TRANSITION", &entries)?.map_err(|error| error.to_string()))
}

fn queued(node: &QueueNode, id: &str, flow_type: &str) -> TestResult {
    ok(
        node,
        "FLOW.CREATE",
        &[
            ("id", text(id)),
            ("type", text(flow_type)),
            ("state", text("queued")),
            ("run_at_ms", int(1_000)),
            ("now_ms", int(900)),
        ],
    )?;
    Ok(())
}

#[test]
fn a_transition_checks_the_state_it_leaves_and_a_running_lease() -> TestResult {
    // Elixir: "flow_transition enforces expected state and running lease
    // guard".
    let node = open(&directory("audit-guards-lease")?, 4)?;
    queued(&node, "guard", "checkout")?;
    let wrong = transition(
        &node,
        "guard",
        "running",
        "completed",
        &[("fencing_token", int(0))],
    )?;
    assert_eq!(wrong, Err("ERR flow wrong state".to_owned()));
    let record = get(&node, "guard", &[])?;
    assert_eq!(
        (field(&record, "state"), field(&record, "version")),
        (text("queued"), int(1))
    );
    let records = claim_one(&node, "checkout", 1_000)?;
    let [lease] = records.as_slice() else {
        return Err("one claim".into());
    };
    let fence = field(lease, "fencing_token");
    let Term::Int(raised) = fence else {
        return Err("no fence".into());
    };
    let stale = Err("ERR stale flow lease".to_owned());
    let unleased = transition(
        &node,
        "guard",
        "running",
        "next",
        &[("fencing_token", fence.clone())],
    )?;
    assert_eq!(unleased, stale);
    let lease_token = ("lease_token", field(lease, "lease_token"));
    let fenced = [
        lease_token.clone(),
        ("fencing_token", int(raised.saturating_add(1))),
    ];
    assert_eq!(
        transition(&node, "guard", "running", "next", &fenced)?,
        stale
    );
    transition(
        &node,
        "guard",
        "running",
        "next",
        &[lease_token, ("fencing_token", fence)],
    )??;
    let record = get(&node, "guard", &[])?;
    assert_eq!(field(&record, "state"), text("next"));
    assert_eq!(field(&record, "lease_token"), Term::Nil);
    Ok(())
}

#[test]
fn a_transition_never_enters_a_terminal_state() -> TestResult {
    // Elixir: "flow_transition rejects terminal states so terminal hooks
    // stay centralized".
    let node = open(&directory("audit-guards-terminal")?, 4)?;
    let parent = create(&node, "parent", "parent", "dispatch", 1_000)?;
    spawn(
        &node,
        "parent",
        vec![child("kid")],
        &[
            ("on_child_failed", text("ignore")),
            ("on_parent_closed", text("abandon_children")),
            ("fencing_token", field(&parent, "fencing_token")),
            (
                "exhaust_to",
                map(&[
                    ("success", text("children_done")),
                    ("failure", text("children_failed")),
                ]),
            ),
        ],
    )??;
    let lease = claim_child(&node, "kid", "worker-terminal-transition")?;
    let refused = transition(
        &node,
        "kid",
        "running",
        "completed",
        &[
            ("partition_key", text(P)),
            ("lease_token", field(&lease, "lease_token")),
            ("fencing_token", field(&lease, "fencing_token")),
        ],
    )?;
    assert_eq!(
        refused,
        Err("ERR terminal flow state requires FLOW.COMPLETE, FLOW.FAIL, or FLOW.CANCEL".to_owned())
    );
    let parent = get(&node, "parent", &in_p())?;
    assert_eq!(field(&parent, "state"), text("waiting_children"));
    assert_eq!(
        path(&parent, &["child_groups", "fanout", "children", "kid"]),
        text("running")
    );
    Ok(())
}

#[test]
fn a_transition_refuses_a_state_over_64_bytes_and_changes_nothing() -> TestResult {
    // Elixir: "flow_transition rolls back index changes when derived keys
    // are invalid" (its uncapped internal transition is Elixir's).
    let node = open(&directory("audit-guards-long")?, 4)?;
    queued(&node, "long", "audit")?;
    let huge = "x".repeat(65_536);
    let refused = transition(
        &node,
        "long",
        "queued",
        &huge,
        &[("fencing_token", int(0)), ("now_ms", int(1_100))],
    )?;
    assert_eq!(
        refused,
        Err("ERR flow state is longer than 64 bytes".to_owned())
    );
    let record = get(&node, "long", &[])?;
    assert_eq!(
        (field(&record, "state"), field(&record, "version")),
        (text("queued"), int(1))
    );
    let records = claim_one(&node, "audit", 1_000)?;
    let [claim] = records.as_slice() else {
        return Err("one claim".into());
    };
    assert_eq!(field(claim, "id"), text("long"));
    Ok(())
}

#[test]
fn fail_and_cancel_finish_a_flow_and_drop_its_due_work() -> TestResult {
    // Elixir: "flow_fail and flow_cancel write terminal states and remove
    // due work".
    let node = open(&directory("audit-guards-finish")?, 4)?;
    queued(&node, "to-fail", "jobs-fail")?;
    queued(&node, "to-cancel", "jobs-cancel")?;
    let records = claim_one(&node, "jobs-fail", 1_000)?;
    let [lease] = records.as_slice() else {
        return Err("one claim".into());
    };
    let Term::Int(fence) = field(lease, "fencing_token") else {
        return Err("no fence".into());
    };
    let fail = |fence: i64| {
        run(
            &node,
            "FLOW.FAIL",
            &[
                ("id", text("to-fail")),
                ("lease_token", field(lease, "lease_token")),
                ("fencing_token", int(fence)),
                ("error", text("error:to-fail")),
                ("now_ms", int(1_500)),
            ],
        )
    };
    assert_eq!(
        refusal(fail(fence.saturating_add(1))?)?,
        "ERR stale flow lease"
    );
    fail(fence)??;
    let failed = get(&node, "to-fail", &[])?;
    assert_eq!(field(&failed, "state"), text("failed"));
    assert!(matches!(field(&failed, "error_ref"), Term::Bin(_)));
    assert_ne!(field(&failed, "error_ref"), text("error:to-fail"));
    assert_eq!(field(&failed, "lease_token"), Term::Nil);
    assert_eq!(field(&failed, "next_run_at_ms"), Term::Nil);
    ok(
        &node,
        "FLOW.CANCEL",
        &[
            ("id", text("to-cancel")),
            ("fencing_token", int(0)),
            ("reason", text("reason:to-cancel")),
            ("now_ms", int(1_500)),
        ],
    )?;
    let cancelled = get(&node, "to-cancel", &[])?;
    assert_eq!(field(&cancelled, "state"), text("cancelled"));
    assert!(matches!(field(&cancelled, "error_ref"), Term::Bin(_)));
    assert_ne!(field(&cancelled, "error_ref"), text("reason:to-cancel"));
    assert_eq!(field(&cancelled, "next_run_at_ms"), Term::Nil);
    for flow_type in ["jobs-fail", "jobs-cancel"] {
        assert_eq!(claim_one(&node, flow_type, 1_500)?, Vec::<Term>::new());
    }
    Ok(())
}

#[test]
fn a_batch_retry_that_exhausts_settles_the_parent_group() -> TestResult {
    // Elixir: "flow_retry_many terminal exhaustion updates colocated parent
    // child group".
    let node = open(&directory("audit-guards-retry-many")?, 4)?;
    let parent = create(&node, "parent", "parent", "dispatch", 1_000)?;
    spawn(
        &node,
        "parent",
        vec![child("kid")],
        &[
            ("group_id", text("retry-many-fanout")),
            ("on_parent_closed", text("abandon_children")),
            ("fencing_token", field(&parent, "fencing_token")),
            (
                "exhaust_to",
                map(&[
                    ("success", text("children_done")),
                    ("failure", text("children_failed")),
                ]),
            ),
        ],
    )??;
    let lease = claim_child(&node, "kid", "worker-retry-many-cross")?;
    ok(
        &node,
        "FLOW.RETRY_MANY",
        &[
            (
                "items",
                Term::List(vec![map(&[
                    ("id", text("kid")),
                    ("partition_key", text(P)),
                    ("lease_token", field(&lease, "lease_token")),
                    ("fencing_token", field(&lease, "fencing_token")),
                ])]),
            ),
            ("now_ms", int(9_000_000_000_001)),
            (
                "retry",
                map(&[("max_retries", int(0)), ("exhausted_to", text("failed"))]),
            ),
        ],
    )?;
    assert_eq!(field(&get(&node, "kid", &in_p())?, "state"), text("failed"));
    let parent = get(&node, "parent", &in_p())?;
    assert_eq!(field(&parent, "state"), text("children_failed"));
    let group = ["child_groups", "retry-many-fanout"];
    assert_eq!(
        path(&parent, &[group[0], group[1], "children", "kid"]),
        text("failed")
    );
    assert_eq!(
        path(&parent, &[group[0], group[1], "summary", "failed"]),
        int(1)
    );
    Ok(())
}

#[test]
fn a_refused_batch_leaves_parents_and_children_as_they_were() -> TestResult {
    // The node's own check beside Elixir's rollback tests: a batch refused
    // after a child finished or a parent closed must not settle that
    // parent's group or close its children afterwards.
    let node = open(&directory("audit-guards-refused-family")?, 4)?;
    let parent = create(&node, "parent", "parent", "dispatch", 1_000)?;
    spawn(
        &node,
        "parent",
        vec![child("kid-a"), child("kid-b")],
        &[("fencing_token", field(&parent, "fencing_token"))],
    )??;
    let leases = claimed(
        &node,
        &[
            ("type", text("child")),
            ("partition_key", text(P)),
            ("worker", text("w")),
            ("limit", int(2)),
            ("now_ms", int(9_000_000_000_000)),
        ],
    )?;
    let mut items = Vec::new();
    for lease in &leases {
        let Term::Int(fence) = field(lease, "fencing_token") else {
            return Err("no fence".into());
        };
        let stale = field(lease, "id") == text("kid-b");
        items.push(map(&[
            ("id", field(lease, "id")),
            ("lease_token", field(lease, "lease_token")),
            (
                "fencing_token",
                int(if stale {
                    fence.saturating_add(1)
                } else {
                    fence
                }),
            ),
        ]));
    }
    let completed = run(
        &node,
        "FLOW.COMPLETE_MANY",
        &[
            ("partition_key", text(P)),
            ("items", Term::List(items)),
            ("now_ms", int(9_000_000_000_001)),
        ],
    )?;
    assert_eq!(refusal(completed)?, "ERR stale flow lease");
    let waiting = get(&node, "parent", &in_p())?;
    let group =
        |record: &Term, kid: &str| path(record, &["child_groups", "fanout", "children", kid]);
    assert_eq!(group(&waiting, "kid-a"), text("running"));

    let Term::Int(fence) = field(&waiting, "fencing_token") else {
        return Err("no parent fence".into());
    };
    let cancel = |id: &str, fence: i64| map(&[("id", text(id)), ("fencing_token", int(fence))]);
    let cancelled = run(
        &node,
        "FLOW.CANCEL_MANY",
        &[
            ("partition_key", text(P)),
            (
                "items",
                Term::List(vec![cancel("parent", fence), cancel("kid-a", 99)]),
            ),
            ("now_ms", int(9_000_000_000_002)),
        ],
    )?;
    assert_eq!(refusal(cancelled)?, "ERR stale flow lease");
    let waiting = get(&node, "parent", &in_p())?;
    assert_eq!(field(&waiting, "state"), text("waiting_children"));
    for kid in ["kid-a", "kid-b"] {
        assert_eq!(
            field(&get(&node, kid, &in_p())?, "state"),
            text("running"),
            "{kid}"
        );
        assert_eq!(group(&waiting, kid), text("running"), "{kid}");
    }
    Ok(())
}
