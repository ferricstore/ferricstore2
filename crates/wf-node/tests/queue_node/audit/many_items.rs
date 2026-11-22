//! `flow/many_item_opts_test.exs`, as wire items reach it: a cancel item's
//! own state metadata and attribute merge apply, and an item with an empty
//! id or an empty lease token is refused at the boundary. (Its tuple and
//! keyword-list shapes are Elixir's in-process forms; the wire's map and
//! list items are audit gaps G3 and G4.)

use wf_application::commands::flow::Term;
use wf_node::composition::QueueNode;

use super::{field, get, int, map, ok, refusal, run, text};
use crate::{TestResult, directory, open};

const P: &str = "p";

fn in_p() -> [(&'static str, Term); 1] {
    [("partition_key", text(P))]
}

fn create(node: &QueueNode, id: &str) -> TestResult {
    ok(
        node,
        "FLOW.CREATE",
        &[
            ("id", text(id)),
            ("type", text("email")),
            ("partition_key", text(P)),
            ("run_at_ms", int(1_000)),
        ],
    )?;
    Ok(())
}

#[test]
fn a_cancel_items_own_metadata_and_attributes_apply() -> TestResult {
    // Elixir: "cancel and transition preserve optional lease and partition
    // keys", its per-item state_meta and attributes_merge.
    let node = open(&directory("audit-items-cancel")?, 4)?;
    create(&node, "flow-1")?;
    ok(
        &node,
        "FLOW.CANCEL_MANY",
        &[
            ("partition_key", text(P)),
            (
                "items",
                Term::List(vec![map(&[
                    ("id", text("flow-1")),
                    ("fencing_token", int(0)),
                    ("state_meta", map(&[("version", int(4))])),
                    ("attributes_merge", map(&[("phase", text("cancelled"))])),
                ])]),
            ),
        ],
    )?;
    let record = get(&node, "flow-1", &in_p())?;
    assert_eq!(field(&record, "state"), text("cancelled"));
    assert_eq!(
        field(&field(&record, "state_meta"), "cancelled"),
        map(&[("version", int(4))])
    );
    assert_eq!(
        field(&field(&record, "attributes"), "phase"),
        text("cancelled")
    );
    Ok(())
}

#[test]
fn items_with_empty_ids_or_lease_tokens_are_refused() -> TestResult {
    // Elixir: "item parsers reject empty ids and required lease tokens at
    // the boundary" and "invalid item opts keep existing error messages".
    let node = open(&directory("audit-items-empty")?, 4)?;
    let empty_id = "ERR flow id must be a non-empty string";
    let empty_lease = "ERR flow lease_token must be a non-empty string";
    let batch = |command: &str, items: Vec<Term>| {
        run(
            &node,
            command,
            &[
                ("partition_key", text(P)),
                ("type", text("email")),
                ("items", Term::List(items)),
            ],
        )
    };
    assert_eq!(
        refusal(batch("FLOW.CREATE_MANY", vec![text("")])?)?,
        empty_id
    );
    let map_item = map(&[("id", text("")), ("type", text("email"))]);
    assert_eq!(
        refusal(batch("FLOW.CREATE_MANY", vec![map_item])?)?,
        empty_id
    );
    let lease = |token: &str| {
        map(&[
            ("id", text("flow-1")),
            ("lease_token", text(token)),
            ("fencing_token", int(1)),
        ])
    };
    assert_eq!(
        refusal(batch("FLOW.COMPLETE_MANY", vec![lease("")])?)?,
        empty_lease
    );
    assert_eq!(
        refusal(batch("FLOW.CANCEL_MANY", vec![lease("")])?)?,
        empty_lease
    );
    let missing = map(&[("id", text("flow-1")), ("fencing_token", int(1))]);
    assert_eq!(
        refusal(batch("FLOW.COMPLETE_MANY", vec![missing])?)?,
        empty_lease
    );
    let moved = run(
        &node,
        "FLOW.TRANSITION_MANY",
        &[
            ("partition_key", text(P)),
            ("from_state", text("queued")),
            ("to_state", text("ready")),
            (
                "items",
                Term::List(vec![Term::List(vec![text("flow-1"), int(1), text("")])]),
            ),
        ],
    )?;
    assert_eq!(refusal(moved)?, empty_lease);
    Ok(())
}
