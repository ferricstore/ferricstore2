//! `flow_user_value_atoms_test.exs`: its accepting cases. Elixir refuses
//! user values holding atoms (other than true, false and nil) because its
//! Raft log decodes them safely; the protocol carries no atoms, so a client
//! cannot send one, and those refusals are Elixir's own. Values with
//! booleans and nil go through every path and read back as sent.

use wf_application::commands::flow::Term;

use super::{claimed, field, get, int, map, ok, sorted, text};
use crate::{TestResult, directory, open};

const P: &str = "tenant-user-values";

fn in_p() -> [(&'static str, Term); 1] {
    [("partition_key", text(P))]
}

#[test]
fn booleans_and_nil_read_back_from_a_payload() -> TestResult {
    // Elixir: "create accepts payloads whose only atoms are true, false and
    // nil".
    let node = open(&directory("audit-plain-payload")?, 4)?;
    let payload = map(&[
        ("order_id", int(1)),
        ("paid", Term::Bool(true)),
        ("refunded", Term::Bool(false)),
        ("note", Term::Nil),
    ]);
    ok(
        &node,
        "FLOW.CREATE",
        &[
            ("id", text("p")),
            ("type", text("plain")),
            ("state", text("queued")),
            ("partition_key", text(P)),
            ("payload", payload.clone()),
            ("run_at_ms", int(1_000)),
            ("now_ms", int(1_000)),
        ],
    )?;
    let records = claimed(
        &node,
        &[
            ("type", text("plain")),
            ("states", Term::List(vec![text("queued")])),
            ("partition_key", text(P)),
            ("worker", text("w")),
            ("limit", int(1)),
            ("now_ms", int(1_001)),
        ],
    )?;
    let [record] = records.as_slice() else {
        return Err("one claim".into());
    };
    assert_eq!(sorted(&field(record, "payload")), sorted(&payload));
    Ok(())
}

#[test]
fn booleans_and_nil_pass_batches_signals_values_and_cancels() -> TestResult {
    // Elixir: "batch, signal, value and cancel paths accept string keys with
    // true, false and nil".
    let node = open(&directory("audit-plain-paths")?, 4)?;
    let put = ok(
        &node,
        "FLOW.VALUE.PUT",
        &[
            ("value", map(&[("ok", Term::Bool(true)), ("no", Term::Bool(false)), ("none", Term::Nil)])),
            ("partition_key", text(P)),
        ],
    )?;
    assert!(matches!(field(&put, "ref"), Term::Bin(_)));
    let item = |id: &str, payload: Term| map(&[("id", text(id)), ("payload", payload)]);
    ok(
        &node,
        "FLOW.CREATE_MANY",
        &[
            ("partition_key", text(P)),
            ("type", text("plain-accept")),
            ("state", text("waiting")),
            ("run_at_ms", int(1_000)),
            ("now_ms", int(1_000)),
            (
                "items",
                Term::List(vec![
                    item("signalled", map(&[("paid", Term::Bool(true)), ("note", Term::Nil)])),
                    item("cancelled", map(&[("refunded", Term::Bool(false))])),
                ]),
            ),
        ],
    )?;
    ok(
        &node,
        "FLOW.SIGNAL",
        &[
            ("id", text("signalled")),
            ("partition_key", text(P)),
            ("signal", text("ready")),
            ("if_state", text("waiting")),
            ("transition_to", text("queued")),
            ("values", map(&[("event", map(&[("paid", Term::Bool(true)), ("note", Term::Nil)]))])),
            ("run_at_ms", int(1_000)),
            ("now_ms", int(1_100)),
        ],
    )?;
    let records = claimed(
        &node,
        &[
            ("type", text("plain-accept")),
            ("states", Term::List(vec![text("queued")])),
            ("partition_key", text(P)),
            ("worker", text("w")),
            ("limit", int(1)),
            ("now_ms", int(1_101)),
        ],
    )?;
    let [lease] = records.as_slice() else {
        return Err("one claim".into());
    };
    ok(
        &node,
        "FLOW.COMPLETE_MANY",
        &[
            ("partition_key", text(P)),
            (
                "items",
                Term::List(vec![map(&[
                    ("id", field(lease, "id")),
                    ("lease_token", field(lease, "lease_token")),
                    ("fencing_token", field(lease, "fencing_token")),
                    ("result", map(&[("ok", Term::Bool(true)), ("note", Term::Nil)])),
                ])]),
            ),
            ("now_ms", int(1_102)),
        ],
    )?;
    assert_eq!(field(&get(&node, "signalled", &in_p())?, "state"), text("completed"));
    ok(
        &node,
        "FLOW.CANCEL",
        &[
            ("id", text("cancelled")),
            ("partition_key", text(P)),
            ("fencing_token", int(0)),
            ("reason", map(&[("by", text("operator")), ("forced", Term::Bool(false)), ("note", Term::Nil)])),
            ("now_ms", int(1_103)),
        ],
    )?;
    assert_eq!(field(&get(&node, "cancelled", &in_p())?, "state"), text("cancelled"));
    Ok(())
}
