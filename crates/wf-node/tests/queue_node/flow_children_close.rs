//! Closing a parent cancels the running children of its groups that ask for
//! it (`on_parent_closed: cancel_children`, the default), as Elixir's
//! `flow_maybe_cancel_children_on_parent_closed` does: when the parent
//! closes, or when such a group resolves while children still run. Ported
//! from Elixir's spawn-children tests (with the siblings in the parent's
//! partition: children share their parent's slot here).

use wf_application::commands::flow::Term;
use wf_node::composition::QueueNode;

use super::flow_children::{claim, field, options, run, text};
use super::{TestResult, directory, open};

struct Group<'a> {
    wait: &'a str,
    on_child_failed: &'a str,
    on_parent_closed: &'a str,
    success: &'a str,
    failure: &'a str,
}

/// Creates parent `par` in partition `p`, claims it and spawns children
/// `ka` and `kb` in group `g`.
fn spawned(node: &QueueNode, group: &Group<'_>) -> TestResult {
    run(
        node,
        "FLOW.CREATE",
        &[
            ("id", text("par")),
            ("type", text("tp")),
            ("state", text("dispatch")),
            ("now_ms", Term::Int(1_000)),
            ("run_at_ms", Term::Int(999)),
            ("partition_key", text("p")),
        ],
    )??;
    let Some((_, token, fence)) = claim(node, "tp", 1_001, None)?.pop() else {
        return Err("parent not claimed".into());
    };
    let child = |id: &str| options(&[("id", text(id)), ("type", text("tc"))]);
    run(
        node,
        "FLOW.SPAWN_CHILDREN",
        &[
            ("id", text("par")),
            ("partition_key", text("p")),
            ("lease_token", token),
            ("fencing_token", fence),
            ("group_id", text("g")),
            ("wait", text(group.wait)),
            ("wait_state", text("waiting")),
            ("on_child_failed", text(group.on_child_failed)),
            ("on_parent_closed", text(group.on_parent_closed)),
            ("success", text(group.success)),
            ("failure", text(group.failure)),
            ("now_ms", Term::Int(1_002)),
            ("children", Term::List(vec![child("ka"), child("kb")])),
        ],
    )??;
    Ok(())
}

fn get(node: &QueueNode, id: &str) -> TestResult<Term> {
    Ok(run(
        node,
        "FLOW.GET",
        &[("id", text(id)), ("partition_key", text("p"))],
    )??)
}

fn group(node: &QueueNode) -> TestResult<Term> {
    Ok(field(&field(&get(node, "par")?, "child_groups"), "g"))
}

fn parent_events(node: &QueueNode) -> TestResult<Vec<Term>> {
    let Term::List(events) = run(
        node,
        "FLOW.HISTORY",
        &[("id", text("par")), ("partition_key", text("p"))],
    )??
    else {
        return Err("history is not a list".into());
    };
    Ok(events
        .iter()
        .filter_map(|event| match event {
            Term::List(parts) => parts.get(1).map(|fields| field(fields, "event")),
            _ => None,
        })
        .collect())
}

fn cancel_parent(node: &QueueNode) -> TestResult {
    let fence = field(&get(node, "par")?, "fencing_token");
    run(
        node,
        "FLOW.CANCEL",
        &[
            ("id", text("par")),
            ("partition_key", text("p")),
            ("fencing_token", fence),
            ("now_ms", Term::Int(1_010)),
            ("reason", text("parent closed")),
        ],
    )??;
    Ok(())
}

const FANOUT: Group<'static> = Group {
    wait: "all",
    on_child_failed: "fail_parent",
    on_parent_closed: "cancel_children",
    success: "done",
    failure: "failed",
};

#[test]
fn cancelling_a_parent_cancels_its_running_children() -> TestResult {
    let path = directory("children-close")?;
    {
        let node = open(&path, 4)?;
        spawned(&node, &FANOUT)?;
        cancel_parent(&node)?;
    }
    // The cancellations are logged with the parent's change.
    let node = open(&path, 3)?;
    for id in ["ka", "kb"] {
        let child = get(&node, id)?;
        assert_eq!(field(&child, "state"), text("cancelled"), "{id}");
        assert_eq!(field(&child, "lease_token"), Term::Nil, "{id}");
        assert!(matches!(
            field(&child, "terminal_retention_until_ms"),
            Term::Int(_)
        ));
    }
    let group = group(&node)?;
    assert_eq!(field(&group, "resolved"), text("failure"));
    assert_eq!(field(&field(&group, "summary"), "cancelled"), Term::Int(2));
    for id in ["ka", "kb"] {
        let result = field(&field(&group, "results"), id);
        assert_eq!(result, options(&[("status", text("cancelled"))]), "{id}");
    }
    assert_eq!(field(&get(&node, "par")?, "state"), text("cancelled"));
    assert!(parent_events(&node)?.contains(&text("children_cancelled")));
    Ok(())
}

#[test]
fn abandoned_children_keep_running_when_their_parent_closes() -> TestResult {
    let path = directory("children-abandon")?;
    let node = open(&path, 4)?;
    spawned(
        &node,
        &Group {
            on_parent_closed: "abandon_children",
            ..FANOUT
        },
    )?;
    cancel_parent(&node)?;
    for id in ["ka", "kb"] {
        assert_eq!(field(&get(&node, id)?, "state"), text("queued"), "{id}");
    }
    assert_eq!(field(&group(&node)?, "resolved"), Term::Nil);
    Ok(())
}

#[test]
fn the_first_child_of_a_wait_any_group_cancels_its_siblings() -> TestResult {
    let path = directory("children-any")?;
    let node = open(&path, 4)?;
    spawned(
        &node,
        &Group {
            wait: "any",
            on_child_failed: "ignore",
            on_parent_closed: "cancel_children",
            success: "completed",
            failure: "children_failed",
        },
    )?;
    let kids = claim(&node, "tc", 1_003, None)?;
    let (id, token, fence) = kids.first().ok_or("no child claimed")?.clone();
    run(
        &node,
        "FLOW.COMPLETE",
        &[
            ("id", id.clone()),
            ("partition_key", text("p")),
            ("lease_token", token),
            ("fencing_token", fence),
            ("now_ms", Term::Int(1_004)),
        ],
    )??;
    let parent = get(&node, "par")?;
    assert_eq!(field(&parent, "state"), text("completed"));
    let group = group(&node)?;
    assert_eq!(field(&group, "resolved"), text("success"));
    let Term::Bin(done) = &id else {
        return Err("child id is not a binary".into());
    };
    let sibling = if done.as_slice() == b"ka" { "kb" } else { "ka" };
    let children = field(&group, "children");
    assert_eq!(children.get(done), Some(&text("completed")));
    assert_eq!(field(&children, sibling), text("cancelled"));
    assert_eq!(field(&get(&node, sibling)?, "state"), text("cancelled"));
    Ok(())
}

#[test]
fn a_failed_child_that_fails_its_parent_cancels_its_siblings() -> TestResult {
    let path = directory("children-fail")?;
    let node = open(&path, 4)?;
    spawned(
        &node,
        &Group {
            success: "children_done",
            failure: "children_failed",
            ..FANOUT
        },
    )?;
    let kids = claim(&node, "tc", 1_003, None)?;
    let (id, token, fence) = kids.first().ok_or("no child claimed")?.clone();
    run(
        &node,
        "FLOW.FAIL",
        &[
            ("id", id.clone()),
            ("partition_key", text("p")),
            ("lease_token", token),
            ("fencing_token", fence),
            ("now_ms", Term::Int(1_004)),
            ("error", text("boom")),
        ],
    )??;
    let parent = get(&node, "par")?;
    assert_eq!(field(&parent, "state"), text("children_failed"));
    let group = group(&node)?;
    assert_eq!(field(&group, "resolved"), text("failure"));
    let Term::Bin(failed) = &id else {
        return Err("child id is not a binary".into());
    };
    let sibling = if failed.as_slice() == b"ka" {
        "kb"
    } else {
        "ka"
    };
    let children = field(&group, "children");
    assert_eq!(children.get(failed), Some(&text("failed")));
    assert_eq!(field(&children, sibling), text("cancelled"));
    assert_eq!(field(&get(&node, sibling)?, "state"), text("cancelled"));
    let events = parent_events(&node)?;
    assert!(events.contains(&text("child_failed")), "{events:?}");
    assert!(events.contains(&text("children_cancelled")), "{events:?}");
    Ok(())
}
