//! `flow_test/sections/flow_spawn_children_wait_any_resolves_on_first_
//! successful_child_across_s.exs`: wait-any groups, batch failures that
//! close a parent, a create batch that refuses one partition's items, input
//! refusals, claims, and the lifecycle reads (the SDK's queries for Elixir's
//! retired `FLOW.LIST`, `FLOW.TERMINALS` and `FLOW.STUCK`).

use wf_application::commands::flow::Term;
use wf_node::composition::QueueNode;

use super::children_rules::{P, child, claim_child, create, finish, in_p, path, spawn};
use super::{claim, events, field, get, int, map, ok, refusal, run, text};
use crate::{TestResult, directory, open};

fn any_group(node: &QueueNode, closed: &str) -> TestResult {
    let parent = create(node, "parent", "parent", "dispatch", 1_000)?;
    spawn(
        node,
        "parent",
        vec![child("a"), child("b")],
        &[
            ("wait", text("any")),
            ("on_child_failed", text("ignore")),
            ("on_parent_closed", text(closed)),
            (
                "exhaust_to",
                map(&[
                    ("success", text("completed")),
                    ("failure", text("children_failed")),
                ]),
            ),
            ("fencing_token", field(&parent, "fencing_token")),
        ],
    )??;
    assert_eq!(
        field(&get(node, "parent", &in_p())?, "state"),
        text("waiting_children")
    );
    Ok(())
}

fn item(lease: &Term) -> Term {
    map(&[
        ("id", field(lease, "id")),
        ("partition_key", text(P)),
        ("lease_token", field(lease, "lease_token")),
        ("fencing_token", field(lease, "fencing_token")),
    ])
}

#[test]
fn wait_any_resolves_on_the_first_success_and_cancels_the_rest() -> TestResult {
    // Elixir: "flow_spawn_children wait any resolves on first successful
    // child in one Raft group".
    let node = open(&directory("audit-any-success")?, 4)?;
    any_group(&node, "cancel_children")?;
    let lease = claim_child(&node, "a", "worker-local-any")?;
    finish(&node, "FLOW.COMPLETE", &lease, &[])??;
    let done = get(&node, "parent", &in_p())?;
    assert_eq!(field(&done, "state"), text("completed"));
    let group = path(&done, &["child_groups", "fanout"]);
    assert_eq!(field(&group, "resolved"), text("success"));
    assert_eq!(path(&group, &["children", "a"]), text("completed"));
    assert_eq!(
        field(&get(&node, "b", &in_p())?, "state"),
        text("cancelled")
    );
    Ok(())
}

#[test]
fn wait_any_fails_when_every_child_fails() -> TestResult {
    // Elixir: "flow_spawn_children wait any resolves failure when every
    // local child fails".
    let node = open(&directory("audit-any-failure")?, 4)?;
    any_group(&node, "abandon_children")?;
    let a = claim_child(&node, "a", "worker-a")?;
    let b = claim_child(&node, "b", "worker-b")?;
    ok(
        &node,
        "FLOW.FAIL_MANY",
        &[
            ("items", Term::List(vec![item(&a), item(&b)])),
            ("error", text("all failed")),
        ],
    )?;
    for id in ["a", "b"] {
        assert_eq!(field(&get(&node, id, &in_p())?, "state"), text("failed"));
    }
    let failed = get(&node, "parent", &in_p())?;
    assert_eq!(field(&failed, "state"), text("children_failed"));
    let group = path(&failed, &["child_groups", "fanout"]);
    assert_eq!(field(&group, "resolved"), text("failure"));
    assert_eq!(path(&group, &["summary", "failed"]), int(2));
    Ok(())
}

#[test]
fn a_batch_failure_that_fails_the_parent_cancels_its_siblings() -> TestResult {
    // Elixir: "flow_fail_many fail_parent policy closes parent and cancels
    // local siblings".
    let node = open(&directory("audit-any-fail-parent")?, 4)?;
    let parent = create(&node, "parent", "parent", "dispatch", 1_000)?;
    let exhaust = map(&[
        ("success", text("children_done")),
        ("failure", text("children_failed")),
    ]);
    let options = [
        ("exhaust_to", exhaust),
        ("fencing_token", field(&parent, "fencing_token")),
    ];
    spawn(
        &node,
        "parent",
        vec![child("failing"), child("sibling")],
        &options,
    )??;
    let lease = claim_child(&node, "failing", "worker-local-fail")?;
    ok(
        &node,
        "FLOW.FAIL_MANY",
        &[
            ("items", Term::List(vec![item(&lease)])),
            ("error", text("boom")),
        ],
    )?;
    assert_eq!(
        field(&get(&node, "failing", &in_p())?, "state"),
        text("failed")
    );
    let failed = get(&node, "parent", &in_p())?;
    assert_eq!(field(&failed, "state"), text("children_failed"));
    let group = path(&failed, &["child_groups", "fanout"]);
    assert_eq!(field(&group, "resolved"), text("failure"));
    assert_eq!(path(&group, &["children", "failing"]), text("failed"));
    assert_eq!(path(&group, &["children", "sibling"]), text("cancelled"));
    assert_eq!(
        field(&get(&node, "sibling", &in_p())?, "state"),
        text("cancelled")
    );
    let names: Vec<Term> = events(&node, "parent", &in_p())?
        .iter()
        .map(|fields| field(fields, "event"))
        .collect();
    assert!(names.contains(&text("child_failed")), "{names:?}");
    assert!(names.contains(&text("children_cancelled")), "{names:?}");
    Ok(())
}

#[test]
fn a_create_batch_refuses_a_partition_s_items_together() -> TestResult {
    // Elixir: "flow_create_many spans shards and rolls back failing shard
    // group". Elixir's unit is a shard, which a client cannot see; the
    // node's is the partition: its items are created all or none.
    let node = open(&directory("audit-any-create-many")?, 4)?;
    let at = [
        ("partition_key", text("same")),
        ("run_at_ms", int(1_000)),
        ("now_ms", int(1_000)),
    ];
    ok(
        &node,
        "FLOW.CREATE",
        &[
            &[("id", text("existing")), ("type", text("bulk"))][..],
            &at[..],
        ]
        .concat(),
    )?;
    let entry =
        |id: &str, partition: &str| map(&[("id", text(id)), ("partition_key", text(partition))]);
    let reply = ok(
        &node,
        "FLOW.CREATE_MANY",
        &[
            ("type", text("bulk")),
            ("run_at_ms", int(1_000)),
            ("now_ms", int(1_000)),
            (
                "items",
                Term::List(vec![
                    entry("existing", "same"),
                    entry("same-new", "same"),
                    entry("other-new", "other"),
                ]),
            ),
        ],
    )?;
    let Term::List(results) = reply else {
        return Err(format!("per-item results expected, got {reply:?}").into());
    };
    let exists = Term::List(vec![text("error"), text("ERR flow already exists")]);
    assert_eq!(results.first(), Some(&exists));
    assert_eq!(results.get(1), Some(&exists));
    assert!(
        !matches!(results.get(2), Some(Term::List(_))),
        "{results:?}"
    );
    assert_eq!(
        get(&node, "same-new", &[("partition_key", text("same"))])?,
        Term::Nil
    );
    let other = [("partition_key", text("other"))];
    assert_eq!(
        field(&get(&node, "other-new", &other)?, "id"),
        text("other-new")
    );
    let names: Vec<Term> = events(&node, "other-new", &other)?
        .iter()
        .map(|fields| field(fields, "event"))
        .collect();
    assert_eq!(names, [text("created")]);
    Ok(())
}

#[test]
fn malformed_creates_are_refused_with_elixir_s_texts() -> TestResult {
    // Elixir: "flow APIs reject malformed inputs before raft apply" (its
    // keyword-list and key-size cases are its own API's).
    let node = open(&directory("audit-any-malformed")?, 4)?;
    let check = |name: &str, entries: &[(&str, Term)], expected: &str| -> TestResult {
        assert_eq!(
            refusal(run(&node, name, entries)?)?,
            expected,
            "{name} {entries:?}"
        );
        Ok(())
    };
    let id = |value: &str| ("id", text(value));
    check(
        "FLOW.CREATE",
        &[id(""), ("type", text("checkout"))],
        "ERR flow id must be a non-empty string",
    )?;
    check(
        "FLOW.CREATE",
        &[id("missing-type"), ("state", text("queued"))],
        "ERR flow type is required",
    )?;
    check(
        "FLOW.CREATE",
        &[id("empty-type"), ("type", text(""))],
        "ERR flow type must be a non-empty string",
    )?;
    let checkout = ("type", text("checkout"));
    check(
        "FLOW.CREATE",
        &[id("n"), checkout.clone(), ("now_ms", int(-1))],
        "ERR flow now_ms must be a non-negative integer",
    )?;
    check(
        "FLOW.CREATE",
        &[id("r"), checkout.clone(), ("run_at_ms", int(-1))],
        "ERR flow run_at_ms must be a non-negative integer",
    )?;
    check(
        "FLOW.CREATE",
        &[id("p"), checkout.clone(), ("priority", int(3))],
        "ERR flow priority must be between 0 and 2",
    )?;
    check(
        "FLOW.CREATE",
        &[id("k"), checkout.clone(), ("partition_key", text(""))],
        "ERR flow partition_key must be a non-empty string or :global",
    )?;
    check(
        "FLOW.CREATE",
        &[id(&"x".repeat(65_536)), checkout],
        "ERR flow id is longer than 256 bytes",
    )?;
    Ok(())
}

#[test]
fn malformed_claims_are_refused_with_elixir_s_texts() -> TestResult {
    // Elixir: "flow APIs reject malformed inputs before raft apply", its
    // claim cases.
    let node = open(&directory("audit-any-malformed-claims")?, 4)?;
    let claim_refusal = |entries: &[(&str, Term)], expected: &str| -> TestResult {
        assert_eq!(
            refusal(claim(&node, "FLOW.CLAIM_DUE", entries)?)?,
            expected,
            "{entries:?}"
        );
        Ok(())
    };
    let worker = ("worker", text("worker-a"));
    claim_refusal(
        &[("type", text("")), worker.clone()],
        "ERR flow type must be a non-empty string",
    )?;
    claim_refusal(&[("type", text("email"))], "ERR flow worker is required")?;
    claim_refusal(
        &[
            ("type", text("email")),
            worker.clone(),
            ("lease_ms", int(0)),
        ],
        "ERR flow lease_ms must be a positive integer",
    )?;
    claim_refusal(
        &[("type", text("email")), worker.clone(), ("limit", int(0))],
        "ERR flow limit must be a positive integer",
    )?;
    claim_refusal(
        &[("type", text("email")), worker, ("partition_key", text(""))],
        "ERR flow partition_key must be a non-empty string or :global",
    )?;
    Ok(())
}

#[test]
fn malformed_mutations_and_reads_are_refused_with_elixir_s_texts() -> TestResult {
    // Elixir: "flow APIs reject malformed inputs before raft apply", its
    // mutation, history and value-reference cases.
    let node = open(&directory("audit-any-malformed-mutations")?, 4)?;
    let check = |name: &str, entries: &[(&str, Term)], expected: &str| -> TestResult {
        assert_eq!(
            refusal(run(&node, name, entries)?)?,
            expected,
            "{name} {entries:?}"
        );
        Ok(())
    };
    let id = |value: &str| ("id", text(value));
    let flow = id("flow");
    let token = ("lease_token", text("token"));
    let fence = ("fencing_token", int(0));
    check(
        "FLOW.COMPLETE",
        &[flow.clone(), ("lease_token", text(""))],
        "ERR flow lease_token must be a non-empty string",
    )?;
    check(
        "FLOW.COMPLETE",
        &[flow.clone(), token.clone()],
        "ERR flow fencing_token is required",
    )?;
    check(
        "FLOW.RETRY",
        &[
            flow.clone(),
            token.clone(),
            fence.clone(),
            ("now_ms", int(-1)),
        ],
        "ERR flow now_ms must be a non-negative integer",
    )?;
    check(
        "FLOW.HISTORY",
        &[id("")],
        "ERR flow id must be a non-empty string",
    )?;
    check(
        "FLOW.HISTORY",
        &[flow.clone(), ("count", int(0))],
        "ERR flow count must be a positive integer",
    )?;
    check(
        "FLOW.HISTORY",
        &[flow.clone(), ("count", int(10_001))],
        "ERR flow count exceeds maximum 10000",
    )?;
    Ok(())
}

#[test]
fn malformed_transitions_failures_and_cancels_are_refused_with_elixir_s_texts() -> TestResult {
    // Elixir: "flow APIs reject malformed inputs before raft apply", its
    // transition, failure and cancel cases.
    let node = open(&directory("audit-any-malformed-transitions")?, 4)?;
    let check = |name: &str, entries: &[(&str, Term)], expected: &str| -> TestResult {
        assert_eq!(
            refusal(run(&node, name, entries)?)?,
            expected,
            "{name} {entries:?}"
        );
        Ok(())
    };
    let id = |value: &str| ("id", text(value));
    let flow = id("flow");
    let token = ("lease_token", text("token"));
    let fence = ("fencing_token", int(0));
    let moves = |from: &str, to: &str| [("from_state", text(from)), ("to_state", text(to))];
    check(
        "FLOW.TRANSITION",
        &[&[id("")][..], &moves("queued", "done")[..]].concat(),
        "ERR flow id must be a non-empty string",
    )?;
    check(
        "FLOW.TRANSITION",
        &[&[flow.clone()][..], &moves("", "done")[..]].concat(),
        "ERR flow from must be a non-empty string",
    )?;
    check(
        "FLOW.TRANSITION",
        &[&[flow.clone()][..], &moves("queued", "")[..]].concat(),
        "ERR flow to must be a non-empty string",
    )?;
    check(
        "FLOW.TRANSITION",
        &[
            &[flow.clone(), ("lease_token", text(""))][..],
            &moves("queued", "done")[..],
        ]
        .concat(),
        "ERR flow lease_token must be a non-empty string",
    )?;
    check(
        "FLOW.TRANSITION",
        &[&[flow.clone()][..], &moves("queued", "done")[..]].concat(),
        "ERR flow fencing_token is required",
    )?;
    check(
        "FLOW.FAIL",
        &[flow.clone(), ("lease_token", text(""))],
        "ERR flow lease_token must be a non-empty string",
    )?;
    check(
        "FLOW.FAIL",
        &[flow.clone(), token.clone()],
        "ERR flow fencing_token is required",
    )?;
    check(
        "FLOW.FAIL",
        &[
            flow.clone(),
            token.clone(),
            fence.clone(),
            ("now_ms", int(-1)),
        ],
        "ERR flow now_ms must be a non-negative integer",
    )?;
    check(
        "FLOW.CANCEL",
        &[id("")],
        "ERR flow id must be a non-empty string",
    )?;
    check(
        "FLOW.CANCEL",
        std::slice::from_ref(&flow),
        "ERR flow fencing_token is required",
    )?;
    Ok(())
}

#[test]
fn value_reference_inputs_are_refused_naming_the_value_to_send() -> TestResult {
    // Elixir: "flow APIs reject malformed inputs before raft apply", its
    // `result_ref`, `error_ref` and `reason_ref` cases (audit gap G22).
    let node = open(&directory("audit-any-malformed-refs")?, 4)?;
    let check = |name: &str, entries: &[(&str, Term)], expected: &str| -> TestResult {
        assert_eq!(
            refusal(run(&node, name, entries)?)?,
            expected,
            "{name} {entries:?}"
        );
        Ok(())
    };
    let id = |value: &str| ("id", text(value));
    let flow = id("flow");
    let token = ("lease_token", text("token"));
    let fence = ("fencing_token", int(0));
    check(
        "FLOW.COMPLETE",
        &[
            flow.clone(),
            token.clone(),
            fence.clone(),
            ("result_ref", text("r")),
        ],
        "ERR flow result_ref input is not supported; use result",
    )?;
    for name in ["FLOW.RETRY", "FLOW.FAIL"] {
        check(
            name,
            &[
                flow.clone(),
                token.clone(),
                fence.clone(),
                ("error_ref", text("e")),
            ],
            "ERR flow error_ref input is not supported; use error",
        )?;
    }
    let reason_ref = "ERR flow reason_ref input is not supported; use reason";
    check(
        "FLOW.CANCEL",
        &[
            flow.clone(),
            fence.clone(),
            ("reason_ref", text("external")),
        ],
        reason_ref,
    )?;
    check(
        "FLOW.CANCEL",
        &[
            flow.clone(),
            fence,
            ("reason", text("inline")),
            ("reason_ref", text("external")),
        ],
        reason_ref,
    )?;
    check(
        "FLOW.REWIND",
        &[
            flow,
            ("to_event", text("1-1")),
            ("reason_ref", text("external")),
        ],
        reason_ref,
    )?;
    Ok(())
}
