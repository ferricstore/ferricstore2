//! `flow/invariant_test.exs`, continued (rest-of-suite audit): turning
//! FIFO on, lanes freed by a move out of the state, due order in mixed
//! claims, hidden entry sequences and reserved group ids.

use wf_application::commands::flow::Term;

use super::fifo_invariants::{KIND, claim, complete, create, fifo, ids, node, partition, texts};
use super::{field, int, map, ok, refusal, run, text};
use crate::TestResult;

#[test]
fn turning_fifo_on_keeps_the_existing_entry_order() -> TestResult {
    // Elixir: "enabling fifo on a populated state uses its existing
    // state-entry order".
    let node = node("audit-rest-fifo-activation")?;
    create(&node, "z-first", "p", 1_000, 2_000)?;
    create(&node, "a-second", "p", 1_001, 2_000)?;
    fifo(&node, &["queued"], &[])?;
    assert_eq!(
        ids(&claim(&node, &partition("p"), 2_000, "w")?),
        texts(&["z-first"])
    );
    assert_eq!(claim(&node, &partition("p"), 2_001, "w")?, []);
    Ok(())
}

#[test]
fn turning_fifo_on_waits_for_leased_members_to_finish() -> TestResult {
    // Elixir: "enabling fifo waits for already leased lane members to
    // drain".
    let node = node("audit-rest-fifo-activation-drain")?;
    create(&node, "first", "p", 1_000, 5_000)?;
    create(&node, "second", "p", 1_001, 2_000)?;
    let running = claim(
        &node,
        &[("partition_key", text("p")), ("limit", int(1))],
        2_000,
        "parallel",
    )?;
    assert_eq!(ids(&running), texts(&["second"]));
    fifo(&node, &["queued"], &[])?;
    assert_eq!(claim(&node, &partition("p"), 5_000, "w")?, []);
    complete(&node, running.first().ok_or("one claimed")?, 5_100)?;
    assert_eq!(
        ids(&claim(&node, &partition("p"), 5_101, "w")?),
        texts(&["first"])
    );
    Ok(())
}

#[test]
fn a_step_enters_its_next_state_afresh() -> TestResult {
    // Elixir: "step_continue records a fresh logical state entry order
    // before fifo activation": a Flow that moved into `charge` and retried
    // there entered it after one created there earlier.
    let node = node("audit-rest-fifo-step-entry")?;
    let made = |id: &str, state: &str, now: i64, due: i64| {
        ok(
            &node,
            "FLOW.CREATE",
            &[
                ("id", text(id)),
                ("type", text(KIND)),
                ("state", text(state)),
                ("partition_key", text("p")),
                ("now_ms", int(now)),
                ("run_at_ms", int(due)),
            ],
        )
    };
    made("moving", "reserve", 1_000, 2_000)?;
    made("waiting", "charge", 1_001, 2_200)?;
    let claimed = claim(
        &node,
        &[
            ("state", text("reserve")),
            ("partition_key", text("p")),
            ("limit", int(1)),
        ],
        2_000,
        "w",
    )?;
    let head = claimed.first().ok_or("one claimed")?;
    let continued = ok(
        &node,
        "FLOW.STEP_CONTINUE",
        &[
            ("id", text("moving")),
            ("partition_key", text("p")),
            ("lease_token", field(head, "lease_token")),
            ("fencing_token", field(head, "fencing_token")),
            ("from_state", text("reserve")),
            ("to_state", text("charge")),
            ("worker", text("w")),
            ("now_ms", int(2_100)),
        ],
    )?;
    ok(
        &node,
        "FLOW.RETRY",
        &[
            ("id", text("moving")),
            ("partition_key", text("p")),
            ("lease_token", field(&continued, "lease_token")),
            ("fencing_token", field(&continued, "fencing_token")),
            ("error", map(&[("reason", text("retry"))])),
            ("run_at_ms", int(2_200)),
            ("now_ms", int(2_101)),
        ],
    )?;
    fifo(&node, &["charge"], &[])?;
    let first = claim(
        &node,
        &[
            ("state", text("charge")),
            ("partition_key", text("p")),
            ("limit", int(1)),
        ],
        2_200,
        "w",
    )?;
    assert_eq!(ids(&first), texts(&["waiting"]));
    Ok(())
}

#[test]
fn a_lane_frees_when_its_running_flow_moves_out_of_the_state() -> TestResult {
    // Elixir: "fifo lane unblocks when the running flow transitions out of
    // the fifo state".
    let node = node("audit-rest-fifo-move-out")?;
    fifo(&node, &["queued"], &[])?;
    create(&node, "z-first", "p", 1_000, 2_000)?;
    create(&node, "a-second", "p", 1_001, 2_000)?;
    let first = claim(
        &node,
        &[("partition_key", text("p")), ("limit", int(1))],
        2_000,
        "w",
    )?;
    let head = first.first().ok_or("one claimed")?;
    ok(
        &node,
        "FLOW.TRANSITION",
        &[
            ("id", text("z-first")),
            ("partition_key", text("p")),
            ("lease_token", field(head, "lease_token")),
            ("fencing_token", field(head, "fencing_token")),
            ("from_state", text("running")),
            ("to_state", text("waiting")),
            ("run_at_ms", int(5_000)),
            ("now_ms", int(2_100)),
        ],
    )?;
    assert_eq!(
        ids(&claim(&node, &partition("p"), 2_101, "w")?),
        texts(&["a-second"])
    );
    Ok(())
}

#[test]
fn a_mixed_claim_takes_parallel_states_in_due_order() -> TestResult {
    // Elixir: "fifo planning preserves due order for parallel states in
    // mixed claims".
    let node = node("audit-rest-fifo-mixed")?;
    ok(
        &node,
        "FLOW.POLICY.SET",
        &[
            ("type", text(KIND)),
            (
                "states",
                map(&[
                    ("fifo_gate", map(&[("mode", text("fifo"))])),
                    ("retry", map(&[("mode", text("parallel"))])),
                ]),
            ),
        ],
    )?;
    for (id, now, due) in [("later", 1_000, 10_000), ("earlier", 1_001, 2_000)] {
        ok(
            &node,
            "FLOW.CREATE",
            &[
                ("id", text(id)),
                ("type", text(KIND)),
                ("state", text("retry")),
                ("partition_key", text("p")),
                ("now_ms", int(now)),
                ("run_at_ms", int(due)),
            ],
        )?;
    }
    let states = Term::List(vec![text("fifo_gate"), text("retry")]);
    let first = claim(
        &node,
        &[
            ("states", states),
            ("partition_key", text("p")),
            ("limit", int(1)),
        ],
        10_000,
        "w",
    )?;
    assert_eq!(ids(&first), texts(&["earlier"]));
    Ok(())
}

#[test]
fn records_never_show_the_state_entry_sequence() -> TestResult {
    // Elixir: "public record returns hide fifo state entry sequence" and
    // "pipeline get hides fifo state entry sequence".
    let node = node("audit-rest-fifo-hidden-sequence")?;
    fifo(&node, &["queued"], &[])?;
    let started = ok(
        &node,
        "FLOW.START_AND_CLAIM",
        &[
            ("id", text("started")),
            ("type", text(KIND)),
            ("initial_state", text("reserve")),
            ("partition_key", text("p")),
            ("worker", text("w")),
            ("now_ms", int(1_000)),
        ],
    )?;
    create(&node, "stepped", "p", 1_000, 1_000)?;
    let claimed = claim(
        &node,
        &[("partition_key", text("p")), ("limit", int(1))],
        1_000,
        "w",
    )?;
    let head = claimed.first().ok_or("one claimed")?;
    let continued = ok(
        &node,
        "FLOW.STEP_CONTINUE",
        &[
            ("id", text("stepped")),
            ("partition_key", text("p")),
            ("lease_token", field(head, "lease_token")),
            ("fencing_token", field(head, "fencing_token")),
            ("from_state", text("queued")),
            ("to_state", text("charge")),
            ("worker", text("w")),
            ("now_ms", int(1_100)),
        ],
    )?;
    let read = ok(
        &node,
        "FLOW.GET",
        &[("id", text("stepped")), ("partition_key", text("p"))],
    )?;
    for record in [&started, head, &continued, &read] {
        let Term::Map(entries) = record else {
            return Err(format!("a record expected, got {record:?}").into());
        };
        for (key, _) in entries {
            assert!(
                !String::from_utf8_lossy(key).contains("seq"),
                "{}",
                String::from_utf8_lossy(key)
            );
        }
    }
    Ok(())
}

#[test]
fn a_child_group_id_may_not_take_a_reserved_name() -> TestResult {
    // Elixir: "spawn_children rejects reserved child group ids".
    let node = node("audit-rest-fifo-reserved-group")?;
    let spawned = run(
        &node,
        "FLOW.SPAWN_CHILDREN",
        &[
            ("id", text("parent")),
            ("partition_key", text("p")),
            (
                "children",
                Term::List(vec![map(&[("id", text("child")), ("type", text("child"))])]),
            ),
            ("group_id", text("__state_enter_seq__")),
            ("wait", text("none")),
            ("fencing_token", int(0)),
            ("now_ms", int(1_000)),
        ],
    )?;
    assert_eq!(refusal(spawned)?, "ERR flow group_id is reserved");
    Ok(())
}
