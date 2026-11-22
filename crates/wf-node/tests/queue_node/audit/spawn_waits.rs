//! `flow_test/sections/flow_create_stores_debug_lineage_metadata_indexes_
//! it.exs`, its spawns: a group that waits for none advances its parent at
//! once and its children are full Flows under the parent's lineage; one
//! that waits for all advances only after every child finishes, counting
//! each and keeping its result reference. (Its lane sequence numbers are
//! Elixir's; the node's lanes are in `flow_fifo`.)

use wf_application::commands::flow::Term;

use super::children_rules::{P, claim_child, create, in_p, path, spawn};
use super::{field, get, int, map, ok, text};
use crate::{TestResult, directory, open};

fn kid(id: &str, n: i64) -> Term {
    map(&[
        ("id", text(id)),
        ("type", text("child")),
        ("payload", map(&[("n", int(n))])),
    ])
}

#[test]
fn a_group_waiting_for_none_advances_its_parent_at_once() -> TestResult {
    // Elixir: "flow_spawn_children wait none creates full child flows and
    // advances parent".
    let node = open(&directory("audit-spawn-wait-none")?, 4)?;
    let parent = create(&node, "parent", "parent", "dispatch", 1_000)?;
    spawn(
        &node,
        "parent",
        vec![kid("child-a", 1), kid("child-b", 2)],
        &[
            ("group_id", text("fanout-1")),
            ("wait", text("none")),
            ("wait_state", Term::Nil),
            ("on_child_failed", text("ignore")),
            ("on_parent_closed", text("abandon_children")),
            (
                "exhaust_to",
                map(&[
                    ("success", text("dispatched")),
                    ("failure", text("dispatch_failed")),
                ]),
            ),
            ("fencing_token", field(&parent, "fencing_token")),
        ],
    )??;
    let advanced = get(&node, "parent", &in_p())?;
    assert_eq!(field(&advanced, "state"), text("dispatched"));
    assert_eq!(
        path(&advanced, &["child_groups", "fanout-1", "resolved"]),
        text("success")
    );
    let query = "FROM runs WHERE partition_key = @partition_key AND parent_flow_id = @lineage_id ORDER BY updated_at_ms ASC LIMIT 10 RETURN RECORDS";
    let found = ok(
        &node,
        "FLOW.QUERY",
        &[
            ("version", text("FQL1")),
            ("query", text(query)),
            (
                "params",
                map(&[("partition_key", text(P)), ("lineage_id", text("parent"))]),
            ),
        ],
    )?;
    let Term::List(children) = field(&found, "records") else {
        return Err(format!("records expected, got {found:?}").into());
    };
    let mut ids: Vec<Term> = children.iter().map(|child| field(child, "id")).collect();
    ids.sort_by_key(|id| format!("{id:?}"));
    assert_eq!(ids, [text("child-a"), text("child-b")]);
    assert!(
        children
            .iter()
            .all(|child| field(child, "root_flow_id") == text("parent"))
    );
    Ok(())
}

#[test]
fn a_group_waiting_for_all_advances_after_every_child() -> TestResult {
    // Elixir: "flow_spawn_children wait all advances parent only after
    // direct children finish".
    let node = open(&directory("audit-spawn-wait-all")?, 4)?;
    let parent = create(&node, "parent", "parent", "dispatch", 2_000)?;
    let queued = |id: &str| {
        map(&[
            ("id", text(id)),
            ("type", text("child")),
            ("state", text("queued")),
        ])
    };
    spawn(
        &node,
        "parent",
        vec![queued("child-a"), queued("child-b")],
        &[
            ("group_id", text("fanout-1")),
            ("fencing_token", field(&parent, "fencing_token")),
            (
                "exhaust_to",
                map(&[
                    ("success", text("children_done")),
                    ("failure", text("failed")),
                ]),
            ),
        ],
    )??;
    let group = |record: &Term, keys: &[&str]| {
        let mut all = vec!["child_groups", "fanout-1"];
        all.extend_from_slice(keys);
        path(record, &all)
    };
    let waiting = get(&node, "parent", &in_p())?;
    assert_eq!(field(&waiting, "state"), text("waiting_children"));
    assert_eq!(group(&waiting, &["summary", "completed"]), int(0));
    let finish = |id: &str, worker: &str, result: Option<&str>| -> TestResult {
        let lease = claim_child(&node, id, worker)?;
        let mut entries = vec![
            ("id", text(id)),
            ("partition_key", text(P)),
            ("lease_token", field(&lease, "lease_token")),
            ("fencing_token", field(&lease, "fencing_token")),
            ("now_ms", int(9_000_000_000_001)),
        ];
        entries.extend(result.map(|value| ("result", text(value))));
        ok(&node, "FLOW.COMPLETE", &entries)?;
        Ok(())
    };
    finish("child-a", "worker-a", Some("child-a-result"))?;
    let still = get(&node, "parent", &in_p())?;
    assert_eq!(field(&still, "state"), text("waiting_children"));
    assert_eq!(group(&still, &["summary", "completed"]), int(1));
    assert_eq!(
        group(&still, &["results", "child-a", "status"]),
        text("completed")
    );
    assert!(matches!(
        group(&still, &["results", "child-a", "result_ref"]),
        Term::Bin(_)
    ));
    finish("child-b", "worker-b", None)?;
    let done = get(&node, "parent", &in_p())?;
    assert_eq!(field(&done, "state"), text("children_done"));
    assert_eq!(group(&done, &["resolved"]), text("success"));
    assert_eq!(group(&done, &["summary", "completed"]), int(2));
    Ok(())
}
