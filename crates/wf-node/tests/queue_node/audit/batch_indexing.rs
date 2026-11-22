//! `flow_batch_indexed_attributes_test.exs` and
//! `flow_batch_waiter_correctness_test.exs`: a Flow closed by a batch is
//! indexed under its type's policy as a single command indexes it, and a
//! failed item of an independent create batch does not keep a parked claim
//! from waking for an item that was created. (Their feature levels and
//! waiter registry are Elixir's.)

use std::time::Duration;

use wf_application::commands::flow::Term;
use wf_node::composition::QueueNode;

use super::{claimed, field, int, map, ok, text, texts};
use crate::{TestResult, directory, open};

const P: &str = "tenant-batch-attrs";

fn acme() -> Term {
    map(&[("tenant", text("acme"))])
}

fn bytes(term: &Term) -> Vec<u8> {
    match term {
        Term::Bin(bytes) => bytes.clone(),
        _ => Vec::new(),
    }
}

fn searched(node: &QueueNode, flow_type: &str) -> TestResult<Vec<Term>> {
    let query = "FROM runs WHERE partition_key = @partition_key AND type = @type AND attribute['tenant'] = @attribute_0 ORDER BY updated_at_ms ASC LIMIT 10 RETURN RECORDS";
    let result = ok(
        node,
        "FLOW.QUERY",
        &[
            ("version", text("FQL1")),
            ("query", text(query)),
            (
                "params",
                map(&[
                    ("partition_key", text(P)),
                    ("type", text(flow_type)),
                    ("attribute_0", text("acme")),
                ]),
            ),
        ],
    )?;
    let Term::List(records) = field(&result, "records") else {
        return Err(format!("records expected, got {result:?}").into());
    };
    let mut ids: Vec<Term> = records.iter().map(|record| field(record, "id")).collect();
    ids.sort_by_key(bytes);
    Ok(ids)
}

fn item(lease: &Term, extra: &[(&str, Term)]) -> Term {
    let mut entries = vec![
        ("id", field(lease, "id")),
        ("lease_token", field(lease, "lease_token")),
        ("fencing_token", field(lease, "fencing_token")),
        ("attributes_merge", acme()),
    ];
    entries.extend(extra.iter().cloned());
    map(&entries)
}

#[test]
fn flows_closed_in_batches_are_indexed_as_single_commands_index_them() -> TestResult {
    // Elixir: "at level 5 Flows closed in batches are indexed as single
    // commands index them" (its below-level-5 behaviour is Elixir's older
    // releases).
    let node = open(&directory("audit-batch-indexing")?, 4)?;
    let flow_type = "batch-attrs";
    ok(
        &node,
        "FLOW.POLICY.SET",
        &[
            ("type", text(flow_type)),
            ("indexed_attributes", texts(&["tenant"])),
        ],
    )?;
    let names = [
        "a-single",
        "b-many",
        "c-independent",
        "d-failed",
        "e-cancelled",
    ];
    for id in names {
        ok(
            &node,
            "FLOW.CREATE",
            &[
                ("id", text(id)),
                ("type", text(flow_type)),
                ("state", text("queued")),
                ("partition_key", text(P)),
                ("run_at_ms", int(1_000)),
                ("now_ms", int(1_000)),
            ],
        )?;
    }
    let mut leases = claimed(
        &node,
        &[
            ("type", text(flow_type)),
            ("partition_key", text(P)),
            ("worker", text("w")),
            ("limit", int(5)),
            ("now_ms", int(1_001)),
        ],
    )?;
    leases.sort_by_key(|lease| bytes(&field(lease, "id")));
    let [single, many, independent, failed, cancelled] = leases.as_slice() else {
        return Err(format!("five claims expected, got {leases:?}").into());
    };
    ok(
        &node,
        "FLOW.COMPLETE",
        &[
            ("id", field(single, "id")),
            ("partition_key", text(P)),
            ("lease_token", field(single, "lease_token")),
            ("fencing_token", field(single, "fencing_token")),
            ("attributes_merge", acme()),
            ("now_ms", int(1_002)),
        ],
    )?;
    let batch = |command: &str, items: Vec<Term>, extra: &[(&str, Term)]| {
        let mut entries = vec![
            ("partition_key", text(P)),
            ("items", Term::List(items)),
            ("now_ms", int(1_003)),
        ];
        entries.extend(extra.iter().cloned());
        ok(&node, command, &entries)
    };
    batch("FLOW.COMPLETE_MANY", vec![item(many, &[])], &[])?;
    batch(
        "FLOW.COMPLETE_MANY",
        vec![item(independent, &[])],
        &[("independent", Term::Bool(true))],
    )?;
    batch(
        "FLOW.FAIL_MANY",
        vec![item(failed, &[("error", text("boom"))])],
        &[],
    )?;
    batch("FLOW.CANCEL_MANY", vec![item(cancelled, &[])], &[])?;
    let expected: Vec<Term> = names.iter().map(|id| text(id)).collect();
    assert_eq!(searched(&node, flow_type)?, expected);
    Ok(())
}

#[test]
fn a_failed_independent_create_does_not_keep_a_claim_from_waking() -> TestResult {
    // Elixir: "a failed independent create does not suppress a committed
    // item's waiter wake".
    let node = open(&directory("audit-batch-waiter")?, 4)?;
    let flow_type = "partial-batch";
    let partition = "partial-batch-partition";
    ok(
        &node,
        "FLOW.CREATE",
        &[
            ("id", text("existing")),
            ("type", text(flow_type)),
            ("state", text("held")),
            ("partition_key", text(partition)),
            ("run_at_ms", int(1_000)),
        ],
    )?;
    let woken = std::thread::scope(|scope| -> TestResult<Vec<Term>> {
        let waiter = scope.spawn(|| {
            claimed(
                &node,
                &[
                    ("type", text(flow_type)),
                    ("states", texts(&["queued"])),
                    ("partition_key", text(partition)),
                    ("worker", text("w")),
                    ("limit", int(1)),
                    ("block_ms", int(2_000)),
                ],
            )
            .map_err(|error| error.to_string())
        });
        std::thread::sleep(Duration::from_millis(100));
        let reply = ok(
            &node,
            "FLOW.CREATE_MANY",
            &[
                ("partition_key", text(partition)),
                ("type", text(flow_type)),
                ("state", text("queued")),
                ("run_at_ms", int(1_000)),
                ("independent", Term::Bool(true)),
                ("items", Term::List(vec![text("existing"), text("created")])),
            ],
        )?;
        assert_eq!(
            reply,
            Term::List(vec![
                Term::List(vec![text("error"), text("ERR flow already exists")]),
                text("ok"),
            ])
        );
        Ok(waiter.join().map_err(|_| "the waiter panicked")??)
    })?;
    let ids: Vec<Term> = woken.iter().map(|record| field(record, "id")).collect();
    assert_eq!(ids, [text("created")]);
    Ok(())
}
