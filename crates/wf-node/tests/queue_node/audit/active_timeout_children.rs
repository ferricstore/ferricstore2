//! `flow_test/sections/flow_max_active_ms_times_out_flows.exs`, its child
//! cases: a child that times out (in cleanup, a claim or its completion)
//! fails its parent's group, and a refused cross-slot spawn leaves claims
//! alone. Elixir's "same shard, other partition" children share the
//! parent's partition here: children must share their parent's slot.

use wf_application::commands::flow::Term;
use wf_node::composition::QueueNode;

use super::children_rules::{P, in_p, path, spawn};
use super::{claimed, field, get, int, map, ok, refusal, run, text};
use crate::{TestResult, directory, open};

fn ahead() -> TestResult<i64> {
    let wall = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?;
    Ok(i64::try_from(wall.as_millis())? + 60_000)
}

/// A parent in `dispatch`, created at `now`, waiting on one child of
/// `child_type` limited to 100 ms.
fn parent_with_limited_child(
    node: &QueueNode,
    child_type: &str,
    now: i64,
    closed: &str,
) -> TestResult {
    ok(
        node,
        "FLOW.CREATE",
        &[
            ("id", text("parent")),
            ("type", text("timeout-parent")),
            ("state", text("dispatch")),
            ("partition_key", text(P)),
            ("run_at_ms", int(now + 10_000)),
            ("now_ms", int(now)),
        ],
    )?;
    let fence = field(&get(node, "parent", &in_p())?, "fencing_token");
    let kid = map(&[
        ("id", text("kid")),
        ("type", text(child_type)),
        ("max_active_ms", int(100)),
        ("run_at_ms", int(now)),
    ]);
    spawn(
        node,
        "parent",
        vec![kid],
        &[
            ("group_id", text("timeout-group")),
            ("on_parent_closed", text(closed)),
            (
                "exhaust_to",
                map(&[
                    ("success", text("children_done")),
                    ("failure", text("children_failed")),
                ]),
            ),
            ("fencing_token", fence),
            ("now_ms", int(now)),
        ],
    )??;
    Ok(())
}

fn assert_group_failed(node: &QueueNode) -> TestResult {
    assert_eq!(field(&get(node, "kid", &in_p())?, "state"), text("failed"));
    let parent = get(node, "parent", &in_p())?;
    assert_eq!(field(&parent, "state"), text("children_failed"));
    assert_eq!(
        path(
            &parent,
            &["child_groups", "timeout-group", "children", "kid"]
        ),
        text("failed")
    );
    Ok(())
}

#[test]
fn a_cleanup_timeout_of_a_child_fails_its_parent_s_group() -> TestResult {
    // Elixir: "timeout cleanup propagates a child failure to a colocated
    // parent".
    let node = open(&directory("audit-tchild-cleanup")?, 4)?;
    let now = ahead()?;
    parent_with_limited_child(&node, "timeout-child", now, "abandon_children")?;
    let pass = ok(
        &node,
        "FLOW.RETENTION_CLEANUP",
        &[("limit", int(10)), ("now_ms", int(now + 100))],
    )?;
    assert_eq!(field(&pass, "active_timeouts"), int(1));
    assert_group_failed(&node)
}

#[test]
fn a_claim_that_meets_an_overdue_child_fails_its_parent_s_group() -> TestResult {
    // Elixir: "claim_due applies an expired child to a same-shard parent in
    // another partition".
    let node = open(&directory("audit-tchild-claim")?, 4)?;
    let now = ahead()?;
    parent_with_limited_child(&node, "timeout-claim-child", now, "abandon_children")?;
    let records = claimed(
        &node,
        &[
            ("type", text("timeout-claim-child")),
            ("partition_key", text(P)),
            ("worker", text("worker-active-timeout")),
            ("now_ms", int(now + 100)),
        ],
    )?;
    assert_eq!(records, Vec::<Term>::new());
    assert_group_failed(&node)
}

#[test]
fn a_late_child_completion_fails_its_parent_s_group() -> TestResult {
    // Elixir: "complete enforces max_active_ms for a colocated child and
    // parent".
    let node = open(&directory("audit-tchild-complete")?, 4)?;
    let now = ahead()?;
    parent_with_limited_child(&node, "timeout-complete-child", now, "abandon_children")?;
    let records = claimed(
        &node,
        &[
            ("type", text("timeout-complete-child")),
            ("partition_key", text(P)),
            ("worker", text("worker-active-timeout")),
            ("lease_ms", int(1_000)),
            ("now_ms", int(now)),
        ],
    )?;
    let [lease] = records.as_slice() else {
        return Err("one claim".into());
    };
    let late = run(
        &node,
        "FLOW.COMPLETE",
        &[
            ("id", text("kid")),
            ("partition_key", text(P)),
            ("lease_token", field(lease, "lease_token")),
            ("fencing_token", field(lease, "fencing_token")),
            ("now_ms", int(now + 100)),
        ],
    )?;
    assert!(refusal(late)?.contains("max_active_ms"));
    assert_group_failed(&node)
}

#[test]
fn a_parent_and_child_timing_out_together_agree() -> TestResult {
    // Elixir: "timeout cleanup reloads lineage records changed earlier in
    // the same batch".
    let node = open(&directory("audit-tchild-both")?, 4)?;
    let now = ahead()?;
    ok(
        &node,
        "FLOW.CREATE",
        &[
            ("id", text("parent")),
            ("type", text("timeout-parent")),
            ("state", text("dispatch")),
            ("partition_key", text(P)),
            ("max_active_ms", int(100)),
            ("run_at_ms", int(now + 10_000)),
            ("now_ms", int(now)),
        ],
    )?;
    let fence = field(&get(&node, "parent", &in_p())?, "fencing_token");
    let kid = map(&[
        ("id", text("kid")),
        ("type", text("timeout-child")),
        ("max_active_ms", int(100)),
        ("run_at_ms", int(now + 10_000)),
    ]);
    spawn(
        &node,
        "parent",
        vec![kid],
        &[
            ("group_id", text("timeout-group")),
            ("fencing_token", fence),
            ("now_ms", int(now)),
        ],
    )??;
    assert_eq!(
        field(&get(&node, "kid", &in_p())?, "max_active_ms"),
        int(100)
    );
    let pass = ok(
        &node,
        "FLOW.RETENTION_CLEANUP",
        &[("limit", int(10)), ("now_ms", int(now + 100))],
    )?;
    assert!(matches!(field(&pass, "active_timeouts"), Term::Int(count) if count >= 1));
    let parent = get(&node, "parent", &in_p())?;
    let kid = get(&node, "kid", &in_p())?;
    assert_eq!(
        path(
            &parent,
            &["child_groups", "timeout-group", "children", "kid"]
        ),
        field(&kid, "state")
    );
    assert_eq!(field(&parent, "state"), text("failed"));
    assert!(
        matches!(field(&kid, "state"), Term::Bin(state) if state == b"failed" || state == b"cancelled")
    );
    Ok(())
}

#[test]
fn a_refused_cross_slot_spawn_leaves_later_claims_alone() -> TestResult {
    // Elixir: "spawn rejects max-active child dependencies across
    // independent groups" and "rejected cross-shard timeout dependencies do
    // not starve an eligible claim".
    let node = open(&directory("audit-tchild-cross")?, 4)?;
    let now = ahead()?;
    ok(
        &node,
        "FLOW.CREATE",
        &[
            ("id", text("parent")),
            ("type", text("timeout-parent")),
            ("state", text("dispatch")),
            ("partition_key", text(P)),
            ("run_at_ms", int(now)),
            ("now_ms", int(now)),
        ],
    )?;
    let fence = field(&get(&node, "parent", &in_p())?, "fencing_token");
    let far = |index: usize| {
        map(&[
            ("id", text(&format!("far-{index}"))),
            ("type", text("timeout-far")),
            ("partition_key", text("elsewhere")),
            ("max_active_ms", int(100)),
            ("run_at_ms", int(now)),
        ])
    };
    let refused = spawn(
        &node,
        "parent",
        (1..=32).map(far).collect(),
        &[("fencing_token", fence.clone())],
    )?;
    assert_eq!(
        refused,
        Err("CROSSSLOT Flow dependency keys must hash to the same shard".to_owned())
    );
    let unchanged = get(&node, "parent", &in_p())?;
    assert_eq!(field(&unchanged, "state"), text("dispatch"));
    assert_eq!(field(&unchanged, "fencing_token"), fence);
    let elsewhere = [("partition_key", text("elsewhere"))];
    assert_eq!(get(&node, "far-1", &elsewhere)?, Term::Nil);
    for (id, at) in [("eligible", now + 1), ("any-eligible", now + 2)] {
        ok(
            &node,
            "FLOW.CREATE",
            &[
                ("id", text(id)),
                ("type", text("timeout-far")),
                ("state", text("queued")),
                ("partition_key", text("elsewhere")),
                ("run_at_ms", int(at)),
                ("now_ms", int(now)),
            ],
        )?;
    }
    for (expected, state) in [("eligible", None), ("any-eligible", Some("any"))] {
        let mut entries = vec![
            ("type", text("timeout-far")),
            ("partition_key", text("elsewhere")),
            ("worker", text("worker-active-timeout-many")),
            ("limit", int(1)),
            ("now_ms", int(now + 100)),
        ];
        entries.extend(state.map(|state| ("state", text(state))));
        let records = claimed(&node, &entries)?;
        let [record] = records.as_slice() else {
            return Err(format!("one claim expected, got {records:?}").into());
        };
        assert_eq!(field(record, "id"), text(expected));
    }
    Ok(())
}
