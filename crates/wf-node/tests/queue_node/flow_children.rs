//! `FLOW.SPAWN_CHILDREN` and child groups (ADR-014 step 8), as the Elixir
//! scenario captured them: the parent waits without a lease, each finished
//! child updates its group, and the resolved group moves the parent on and
//! makes it claimable.

use wf_application::commands::flow::{self, FlowService, Term};
use wf_application::commands::queue::{QueueService, ServiceError};
use wf_node::composition::QueueNode;

use super::{TestResult, call, directory, open};

pub(super) fn options(entries: &[(&str, Term)]) -> Term {
    Term::Map(
        entries
            .iter()
            .map(|(key, value)| (key.as_bytes().to_vec(), value.clone()))
            .collect(),
    )
}

pub(super) fn text(value: &str) -> Term {
    Term::text(value)
}

pub(super) fn run(
    node: &QueueNode,
    name: &str,
    entries: &[(&str, Term)],
) -> TestResult<Result<Term, ServiceError>> {
    let command = flow::parse(name, &options(entries))?;
    call(|reply| node.execute(command, reply))
}

pub(super) fn field(record: &Term, key: &str) -> Term {
    record.get(key.as_bytes()).cloned().unwrap_or(Term::Nil)
}

/// Claims Flows of `flow_type` in partition `p`; returns id, token, fence.
pub(super) fn claim(
    node: &QueueNode,
    flow_type: &str,
    now: i64,
    state: Option<&str>,
) -> TestResult<Vec<(Term, Term, Term)>> {
    let mut entries = vec![
        ("type", text(flow_type)),
        ("worker", text("w")),
        ("now_ms", Term::Int(now)),
        ("limit", Term::Int(10)),
        ("partition_key", text("p")),
        ("return", text("jobs_compact")),
    ];
    entries.extend(state.map(|state| ("state", text(state))));
    let parsed = flow::claim::parse("FLOW.CLAIM_DUE", &options(&entries))?;
    let shape = parsed.shape;
    let claimed = call(|reply| node.claim(parsed.request, reply))??;
    let Term::List(jobs) = flow::claim::reply(shape, claimed) else {
        return Err("claim reply is not a list".into());
    };
    Ok(jobs
        .into_iter()
        .filter_map(|job| match job {
            Term::List(items) => Some((
                items.first()?.clone(),
                items.get(2)?.clone(),
                items.get(3)?.clone(),
            )),
            _ => None,
        })
        .collect())
}

fn spawn(node: &QueueNode, token: &Term, fence: &Term) -> TestResult<Result<Term, ServiceError>> {
    let child = |id: &str| {
        options(&[
            ("id", text(id)),
            ("type", text("tc")),
            ("payload", text("{\"i\":1}")),
        ])
    };
    run(
        node,
        "FLOW.SPAWN_CHILDREN",
        &[
            ("id", text("par")),
            ("partition_key", text("p")),
            ("lease_token", token.clone()),
            ("fencing_token", fence.clone()),
            ("group_id", text("g")),
            ("wait", text("all")),
            ("wait_state", text("waiting")),
            ("success", text("done")),
            ("failure", text("broken")),
            ("now_ms", Term::Int(1_002)),
            (
                "children",
                Term::List(vec![child("k0"), child("k1"), child("k2")]),
            ),
        ],
    )
}

fn parent_group(node: &QueueNode) -> TestResult<(Term, Term)> {
    let parent = run(
        node,
        "FLOW.GET",
        &[("id", text("par")), ("partition_key", text("p"))],
    )??;
    let group = field(&field(&parent, "child_groups"), "g");
    Ok((parent, group))
}

#[test]
fn a_parent_waits_for_its_children_and_resumes_when_they_finish() -> TestResult {
    let path = directory("flow-children")?;
    {
        let node = open(&path, 4)?;
        run(
            &node,
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
        let Some((_, token, fence)) = claim(&node, "tp", 1_001, None)?.pop() else {
            return Err("parent not claimed".into());
        };
        assert_eq!(spawn(&node, &token, &fence)??, text("OK"));
        // A retry of the same spawn is accepted and changes nothing.
        assert_eq!(spawn(&node, &token, &fence)??, text("OK"));
        let (parent, group) = parent_group(&node)?;
        assert_eq!(field(&parent, "state"), text("waiting"));
        assert_eq!(field(&parent, "run_state"), text("dispatch"));
        assert_eq!(field(&parent, "lease_token"), Term::Nil);
        assert_eq!(field(&parent, "next_run_at_ms"), Term::Nil);
        assert_eq!(field(&parent, "version"), Term::Int(3));
        assert_eq!(field(&group, "resolved"), Term::Nil);
        assert_eq!(field(&field(&group, "summary"), "total"), Term::Int(3));
        let child = run(
            &node,
            "FLOW.GET",
            &[("id", text("k0")), ("partition_key", text("p"))],
        )??;
        assert_eq!(field(&child, "parent_flow_id"), text("par"));
        assert_eq!(field(&child, "parent_partition_key"), text("p"));
        assert_eq!(field(&child, "state"), text("queued"));
        let kids = claim(&node, "tc", 1_003, None)?;
        assert_eq!(kids.len(), 3);
        for (offset, (id, token, fence)) in kids.iter().enumerate() {
            let now = 1_004 + i64::try_from(offset)?;
            run(
                &node,
                "FLOW.COMPLETE",
                &[
                    ("id", id.clone()),
                    ("partition_key", text("p")),
                    ("lease_token", token.clone()),
                    ("fencing_token", fence.clone()),
                    ("now_ms", Term::Int(now)),
                    ("result", text("r")),
                ],
            )??;
            let (_, group) = parent_group(&node)?;
            let completed = i64::try_from(offset)? + 1;
            assert_eq!(
                field(&field(&group, "summary"), "completed"),
                Term::Int(completed)
            );
        }
    }
    let node = open(&path, 3)?;
    let (parent, group) = parent_group(&node)?;
    assert_eq!(field(&group, "resolved"), text("success"));
    assert_eq!(field(&parent, "state"), text("done"));
    assert_eq!(field(&parent, "next_run_at_ms"), Term::Int(1_006));
    assert_eq!(field(&parent, "version"), Term::Int(6));
    let result = field(&field(&field(&group, "results"), "k1"), "result_ref");
    assert!(matches!(result, Term::Bin(name) if name.ends_with(b":v:r:k1:3")));
    let Term::List(events) = run(
        &node,
        "FLOW.HISTORY",
        &[("id", text("par")), ("partition_key", text("p"))],
    )??
    else {
        return Err("history is not a list".into());
    };
    let names: Vec<Term> = events
        .iter()
        .filter_map(|event| match event {
            Term::List(parts) => parts.get(1).map(|fields| field(fields, "event")),
            _ => None,
        })
        .collect();
    assert_eq!(
        names,
        vec![
            text("created"),
            text("claimed"),
            text("children_spawned"),
            text("child_completed"),
            text("child_completed"),
            text("child_completed"),
        ]
    );
    assert_eq!(claim(&node, "tp", 1_007, Some("done"))?.len(), 1);
    Ok(())
}

#[test]
fn children_must_share_their_parents_slot_whatever_the_shard_count() -> TestResult {
    let path = directory("flow-children-slot")?;
    // One shard holds every slot, so only the slot rule refuses here.
    let node = open(&path, 1)?;
    run(
        &node,
        "FLOW.CREATE",
        &[
            ("id", text("par")),
            ("type", text("tp")),
            ("state", text("dispatch")),
            ("now_ms", Term::Int(1_000)),
            ("partition_key", text("p")),
        ],
    )??;
    let Some((_, token, fence)) = claim(&node, "tp", 1_001, None)?.pop() else {
        return Err("parent not claimed".into());
    };
    let elsewhere = options(&[
        ("id", text("k9")),
        ("type", text("tc")),
        ("partition_key", text("other")),
    ]);
    let refused = run(
        &node,
        "FLOW.SPAWN_CHILDREN",
        &[
            ("id", text("par")),
            ("partition_key", text("p")),
            ("lease_token", token),
            ("fencing_token", fence),
            ("group_id", text("g")),
            ("wait", text("all")),
            ("wait_state", text("waiting")),
            ("success", text("done")),
            ("failure", text("broken")),
            ("children", Term::List(vec![elsewhere])),
        ],
    )?;
    assert_eq!(
        refused,
        Err(ServiceError::rejected(
            "CROSSSLOT Flow dependency keys must hash to the same shard".to_owned()
        ))
    );
    let child = run(
        &node,
        "FLOW.GET",
        &[("id", text("k9")), ("partition_key", text("other"))],
    )??;
    assert_eq!(child, Term::Nil);
    Ok(())
}
