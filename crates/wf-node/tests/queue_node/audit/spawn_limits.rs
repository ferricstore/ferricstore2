//! `flow/child_policy_test.exs` and `flow_run_steps_many_validation_
//! test.exs`: a spawn's children, policies and exhaust states, and the
//! size of step and child batches. Batches are capped at Elixir's largest
//! setting, 100000 items (audit gap G15; Elixir's default is 1000 and
//! configurable), and a step batch's items times steps at the same bound.

use wf_application::commands::flow::Term;
use wf_node::composition::QueueNode;

use super::{int, map, ok, refusal, run, text, texts};
use crate::{TestResult, directory, open};

const P: &str = "partition";

fn parent(node: &QueueNode, id: &str) -> TestResult {
    ok(
        node,
        "FLOW.CREATE",
        &[
            ("id", text(id)),
            ("type", text("parent")),
            ("state", text("dispatch")),
            ("partition_key", text(P)),
        ],
    )?;
    Ok(())
}

fn spawn(
    node: &QueueNode,
    id: &str,
    children: Term,
    extra: &[(&str, Term)],
) -> TestResult<Result<Term, String>> {
    let mut entries = vec![
        ("id", text(id)),
        ("partition_key", text(P)),
        ("children", children),
        ("group_id", text("group")),
        ("wait_state", text("waiting")),
        ("from_state", text("dispatch")),
        ("fencing_token", int(0)),
        (
            "exhaust_to",
            map(&[("success", text("completed")), ("failure", text("failed"))]),
        ),
    ];
    entries.retain(|(key, _)| !extra.iter().any(|(given, _)| given == key));
    // A nil option is one left out.
    entries.extend(
        extra
            .iter()
            .filter(|(_, value)| *value != Term::Nil)
            .cloned(),
    );
    Ok(run(node, "FLOW.SPAWN_CHILDREN", &entries)?.map_err(|error| error.to_string()))
}

fn kids(count: usize) -> Term {
    Term::List(
        (1..=count)
            .map(|index| {
                map(&[
                    ("id", text(&format!("child-{index}"))),
                    ("type", text("child")),
                ])
            })
            .collect(),
    )
}

#[test]
fn a_spawn_checks_its_children_policies_and_exhaust_states() -> TestResult {
    // Elixir: "validate_children requires a non-empty list",
    // "validate_no_parent_child_id rejects parent id as child id",
    // "optional_policy normalizes binary values and rejects unsupported
    // values", "exhaust_to_opts accepts explicit map or success/failure
    // options" and "exhaust_to_opts rejects missing or empty states".
    let node = open(&directory("audit-spawn-policies")?, 4)?;
    for id in ["empty", "self", "bad", "hyphen", "flat", "missing", "blank"] {
        parent(&node, id)?;
    }
    let empty = spawn(&node, "empty", Term::List(Vec::new()), &[])?;
    assert_eq!(
        empty,
        Err("ERR flow children must be a non-empty list".to_owned())
    );
    let own = map(&[("id", text("self")), ("type", text("child"))]);
    let itself = spawn(&node, "self", Term::List(vec![own]), &[])?;
    assert_eq!(
        itself,
        Err("ERR flow child id must differ from parent id".to_owned())
    );
    let bad = spawn(&node, "bad", kids(1), &[("on_child_failed", text("bad"))])?;
    assert_eq!(
        bad,
        Err("ERR flow on_child_failed has unsupported value".to_owned())
    );
    let hyphen = map(&[("id", text("hyphen-kid")), ("type", text("child"))]);
    spawn(
        &node,
        "hyphen",
        Term::List(vec![hyphen]),
        &[("on_child_failed", text("fail-parent"))],
    )??;
    let flat = map(&[("id", text("flat-kid")), ("type", text("child"))]);
    spawn(
        &node,
        "flat",
        Term::List(vec![flat]),
        &[
            ("exhaust_to", Term::Nil),
            ("success", text("done")),
            ("failure", text("failed")),
        ],
    )??;
    let required = "ERR flow exhaust_to must include success and failure states".to_owned();
    let missing = map(&[("id", text("missing-kid")), ("type", text("child"))]);
    let no_states = spawn(
        &node,
        "missing",
        Term::List(vec![missing]),
        &[("exhaust_to", map(&[]))],
    )?;
    assert_eq!(no_states, Err(required.clone()));
    let blank_kid = map(&[("id", text("blank-kid")), ("type", text("child"))]);
    let blank = spawn(
        &node,
        "blank",
        Term::List(vec![blank_kid]),
        &[(
            "exhaust_to",
            map(&[("success", text("")), ("failure", text("failed"))]),
        )],
    )?;
    assert_eq!(blank, Err(required));
    Ok(())
}

#[test]
fn step_and_child_batches_stay_within_the_batch_cap() -> TestResult {
    // Elixir: "run_steps_many rejects batches above the configured Flow
    // item limit", "run_steps_many bounds the item-by-step work admitted
    // to one Raft apply" and "spawn_children rejects child batches above
    // the configured Flow item limit" (audit gap G38).
    let node = open(&directory("audit-spawn-batch-cap")?, 4)?;
    let items = |count: usize| {
        Term::List(
            (1..=count)
                .map(|index| {
                    map(&[
                        ("id", text(&format!("flow-{index}"))),
                        ("partition_key", text(P)),
                    ])
                })
                .collect(),
        )
    };
    let steps = |count: usize, states: &[&str]| {
        run(
            &node,
            "FLOW.RUN_STEPS_MANY",
            &[
                ("type", text("batch")),
                ("states", texts(states)),
                ("worker", text("worker")),
                ("items", items(count)),
            ],
        )
    };
    assert_eq!(
        refusal(steps(100_001, &["run"])?)?,
        "ERR flow batch item count exceeds maximum 100000"
    );
    assert_eq!(
        refusal(steps(50_001, &["prepare", "commit"])?)?,
        "ERR flow run step operation count exceeds maximum 100000"
    );
    parent(&node, "parent")?;
    let children = spawn(&node, "parent", kids(100_001), &[])?;
    assert_eq!(
        children,
        Err("ERR flow batch item count exceeds maximum 100000".to_owned())
    );
    Ok(())
}
