//! Batch items as Elixir's native server reads them (audit gaps G3, G4;
//! probe `scenario_many_items.py`, 21 Nov): an item map carries any option
//! of its single command, and a list item's order depends on the command.

use wf_application::commands::flow::Term;
use wf_node::composition::QueueNode;

use super::{claimed, field, get, int, map, ok, text, texts};
use crate::{TestResult, directory, open};

const P: &str = "p";

fn with<'a>(entries: &[(&'a str, Term)]) -> Vec<(&'a str, Term)> {
    let mut all = vec![("partition_key", text(P))];
    all.extend(entries.iter().cloned());
    all
}

/// Creates and claims Flows `ids`; returns each one's lease token and fence.
fn claimed_flows(node: &QueueNode, ids: &[&str]) -> TestResult<Vec<(Term, Term, Term)>> {
    for id in ids {
        ok(
            node,
            "FLOW.CREATE",
            &with(&[
                ("id", text(id)),
                ("type", text("batch")),
                ("run_at_ms", int(1_000)),
                ("now_ms", int(1_000)),
            ]),
        )?;
    }
    let records = claimed(
        node,
        &with(&[
            ("type", text("batch")),
            ("worker", text("w")),
            ("limit", int(10)),
            ("now_ms", int(1_001)),
            ("payload", Term::Bool(false)),
        ]),
    )?;
    let mut leases: Vec<(Term, Term, Term)> = records
        .iter()
        .map(|record| {
            (
                field(record, "id"),
                field(record, "lease_token"),
                field(record, "fencing_token"),
            )
        })
        .collect();
    leases.sort_by(|a, b| match (&a.0, &b.0) {
        (Term::Bin(a), Term::Bin(b)) => a.cmp(b),
        _ => std::cmp::Ordering::Equal,
    });
    Ok(leases)
}

fn full(node: &QueueNode, id: &str) -> TestResult<Term> {
    get(
        node,
        id,
        &with(&[("full", Term::Bool(true)), ("values", texts(&["k"]))]),
    )
}

#[test]
fn transition_items_read_fence_before_token_and_maps_carry_their_own_options() -> TestResult {
    let node = open(&directory("audit-batch-transition")?, 4)?;
    let leases = claimed_flows(&node, &["a", "b"])?;
    let [(a, a_token, a_fence), (b, b_token, b_fence)] = leases.as_slice() else {
        return Err("two leases".into());
    };
    ok(
        &node,
        "FLOW.TRANSITION_MANY",
        &with(&[
            ("from_state", text("running")),
            ("to_state", text("waiting")),
            ("run_at_ms", int(5_000)),
            ("now_ms", int(1_002)),
            (
                "items",
                Term::List(vec![
                    Term::List(vec![a.clone(), a_fence.clone(), a_token.clone()]),
                    map(&[
                        ("id", b.clone()),
                        ("lease_token", b_token.clone()),
                        ("fencing_token", b_fence.clone()),
                        ("values", map(&[("k", text("vb"))])),
                        ("attributes", map(&[("x", text("1"))])),
                    ]),
                ]),
            ),
        ]),
    )?;
    assert_eq!(field(&full(&node, "a")?, "state"), text("waiting"));
    let b_record = full(&node, "b")?;
    assert_eq!(field(&b_record, "state"), text("waiting"));
    assert_eq!(field(&b_record, "values"), map(&[("k", text("vb"))]));
    assert_eq!(field(&b_record, "attributes"), map(&[("x", text("1"))]));
    Ok(())
}

#[test]
fn cancel_items_name_only_the_fence_and_complete_items_carry_results() -> TestResult {
    let node = open(&directory("audit-batch-cancel")?, 4)?;
    let leases = claimed_flows(&node, &["c", "d", "e"])?;
    let [
        (c, c_token, c_fence),
        (d, d_token, d_fence),
        (e, e_token, e_fence),
    ] = leases.as_slice()
    else {
        return Err("three leases".into());
    };
    // A running Flow's cancel needs its lease too: given in a map item.
    ok(
        &node,
        "FLOW.CANCEL_MANY",
        &[
            ("now_ms", int(1_003)),
            (
                "items",
                Term::List(vec![map(&[
                    ("id", c.clone()),
                    ("partition_key", text(P)),
                    ("lease_token", c_token.clone()),
                    ("fencing_token", c_fence.clone()),
                ])]),
            ),
        ],
    )?;
    assert_eq!(field(&full(&node, "c")?, "state"), text("cancelled"));
    // List items: `[id, partition, fence]` and `[id, fence]` reach the
    // cancel with the fence read as a fence, so a running Flow is refused
    // for its missing lease rather than read as unknown.
    let independent = |items: Vec<Term>, partition: Option<&str>| {
        let mut entries = vec![
            ("now_ms", int(1_004)),
            ("independent", Term::Bool(true)),
            ("items", Term::List(items)),
        ];
        entries.extend(partition.map(|partition| ("partition_key", text(partition))));
        ok(&node, "FLOW.CANCEL_MANY", &entries)
    };
    let stale = |reply: Term| match reply {
        Term::List(items) => items
            .iter()
            .all(|item| *item == Term::List(vec![text("error"), text("ERR stale flow lease")])),
        _ => false,
    };
    assert!(stale(independent(
        vec![Term::List(vec![d.clone(), text(P), d_fence.clone()])],
        None
    )?));
    assert!(stale(independent(
        vec![Term::List(vec![e.clone(), e_fence.clone()])],
        Some(P)
    )?));
    ok(
        &node,
        "FLOW.COMPLETE_MANY",
        &with(&[
            ("now_ms", int(1_005)),
            (
                "items",
                Term::List(vec![
                    map(&[
                        ("id", d.clone()),
                        ("lease_token", d_token.clone()),
                        ("fencing_token", d_fence.clone()),
                        ("result", map(&[("ok", Term::Bool(true))])),
                    ]),
                    Term::List(vec![e.clone(), e_token.clone(), e_fence.clone()]),
                ]),
            ),
        ]),
    )?;
    assert_eq!(
        field(&full(&node, "d")?, "result"),
        map(&[("ok", Term::Bool(true))])
    );
    assert_eq!(field(&full(&node, "e")?, "state"), text("completed"));
    Ok(())
}
