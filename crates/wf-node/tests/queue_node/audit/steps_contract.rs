//! `flow_write_contract_test.exs`, second part: step commands and the
//! public limits that refuse a write without touching the Flow.

use wf_application::commands::flow::Term;

use super::write_contract::{int_of, lease, now, payload, with};
use super::{claimed, events, field, get, int, map, ok, refusal, run, text};
use crate::{TestResult, directory, open};

#[test]
fn step_continue_advances_the_logical_state_under_a_fresh_running_lease() -> TestResult {
    // Elixir: "flow step_continue atomically advances logical state and
    // keeps a fresh running lease".
    let node = open(&directory("audit-write-step")?, 4)?;
    let now = now()?;
    let partition = [("partition_key", text("step"))];
    ok(
        &node,
        "FLOW.CREATE",
        &[
            ("id", text("s")),
            ("type", text("step-type")),
            ("partition_key", text("step")),
            ("payload", payload("queued")),
            ("run_at_ms", int(now)),
        ],
    )?;
    let records = claimed(
        &node,
        &[
            ("type", text("step-type")),
            ("limit", int(1)),
            ("now_ms", int(now)),
            ("worker", text("contract-step-worker")),
            ("partition_key", text("step")),
        ],
    )?;
    let [claim] = records.as_slice() else {
        return Err("one claim".into());
    };
    assert_eq!(field(claim, "state"), text("running"));
    assert_eq!(field(claim, "run_state"), text("queued"));
    let fence = int_of(&field(claim, "fencing_token"))?;
    let continued = ok(
        &node,
        "FLOW.STEP_CONTINUE",
        &with(
            lease(claim),
            &[
                ("partition_key", text("step")),
                ("from_state", text("queued")),
                ("to_state", text("charge_card")),
                ("lease_ms", int(5_000)),
                ("now_ms", int(now + 1)),
                ("payload", payload("charge_card")),
            ],
        ),
    )?;
    assert_eq!(field(&continued, "id"), text("s"));
    assert_eq!(field(&continued, "state"), text("running"));
    assert_eq!(field(&continued, "run_state"), text("charge_card"));
    assert_ne!(
        field(&continued, "lease_token"),
        field(claim, "lease_token")
    );
    assert_eq!(field(&continued, "fencing_token"), int(fence + 1));
    assert_eq!(field(&continued, "lease_deadline_ms"), int(now + 5_001));
    let old_lease = with(lease(claim), &partition);
    assert_eq!(
        refusal(run(&node, "FLOW.COMPLETE", &old_lease)?)?,
        "ERR stale flow lease"
    );
    let retry = with(
        lease(&continued),
        &[
            ("partition_key", text("step")),
            ("run_at_ms", int(now + 3_000)),
            ("error", map(&[("reason", text("try-again"))])),
        ],
    );
    assert_eq!(ok(&node, "FLOW.RETRY", &retry)?, text("OK"));
    let retried = get(&node, "s", &partition)?;
    assert_eq!(field(&retried, "state"), text("charge_card"));
    assert_eq!(field(&retried, "run_state"), Term::Nil);
    assert_eq!(field(&retried, "next_run_at_ms"), int(now + 3_000));
    Ok(())
}

#[test]
fn start_and_claim_creates_the_first_step_under_a_running_lease() -> TestResult {
    // Elixir: "flow start_and_claim creates the first logical step with a
    // running lease".
    let node = open(&directory("audit-write-start")?, 4)?;
    let now = now()?;
    let partition = [("partition_key", text("start"))];
    let start = |lease_ms: Option<i64>| {
        let mut entries = vec![
            ("id", text("st")),
            ("type", text("start-type")),
            ("initial_state", text("reserve_inventory")),
            ("partition_key", text("start")),
            ("worker", text("contract-start-worker")),
            ("now_ms", int(now)),
            ("payload", map(&[("order_id", int(7))])),
        ];
        entries.extend(lease_ms.map(|ms| ("lease_ms", int(ms))));
        run(&node, "FLOW.START_AND_CLAIM", &entries)
    };
    let started = start(Some(7_000))??;
    assert_eq!(field(&started, "id"), text("st"));
    assert_eq!(field(&started, "type"), text("start-type"));
    assert_eq!(field(&started, "state"), text("running"));
    assert_eq!(field(&started, "run_state"), text("reserve_inventory"));
    assert_eq!(field(&started, "attempts"), int(1));
    assert_eq!(field(&started, "fencing_token"), int(1));
    assert_eq!(
        field(&started, "lease_owner"),
        text("contract-start-worker")
    );
    assert!(matches!(field(&started, "lease_token"), Term::Bin(_)));
    assert_eq!(field(&started, "lease_deadline_ms"), int(now + 7_000));
    assert_eq!(field(&started, "next_run_at_ms"), int(now + 7_000));
    assert_eq!(refusal(start(None)?)?, "ERR flow already exists");
    let retry = [
        ("id", text("st")),
        ("partition_key", text("start")),
        ("lease_token", field(&started, "lease_token")),
        ("fencing_token", field(&started, "fencing_token")),
        ("run_at_ms", int(now + 1_000)),
        ("error", map(&[("reason", text("try-again"))])),
    ];
    assert_eq!(ok(&node, "FLOW.RETRY", &retry)?, text("OK"));
    let retried = get(&node, "st", &partition)?;
    assert_eq!(field(&retried, "state"), text("reserve_inventory"));
    assert_eq!(field(&retried, "run_state"), Term::Nil);
    assert_eq!(field(&retried, "next_run_at_ms"), int(now + 1_000));
    Ok(())
}

fn step_items(ids: &[&str], partition: &str) -> Term {
    Term::List(
        ids.iter()
            .map(|id| map(&[("id", text(id)), ("partition_key", text(partition))]))
            .collect(),
    )
}

#[test]
fn run_steps_many_without_a_clock_stamps_the_server_time() -> TestResult {
    // Elixir: "flow run_steps_many defaults persisted timestamps to the
    // cluster clock". The node has no hybrid clock: without `now_ms` it
    // stamps its own wall clock, never earlier than the call. Elixir's test
    // names its one step `completed`, which only `RUN_STEPS_MANY` lets
    // through (it skips the step checks `START_AND_CLAIM` and
    // `STEP_CONTINUE` apply); the node keeps one rule and refuses it.
    let node = open(&directory("audit-write-steps-clock")?, 4)?;
    let steps = |id: &str, state: &str| {
        ok(
            &node,
            "FLOW.RUN_STEPS_MANY",
            &[
                ("items", step_items(&[id], "hlc-p")),
                ("type", text("hlc-type")),
                ("states", Term::List(vec![text(state)])),
                ("worker", text("hlc-worker")),
            ],
        )
    };
    let before = now()?;
    assert_eq!(steps("hlc", "charge")?, text("OK"));
    let record = get(&node, "hlc", &[("partition_key", text("hlc-p"))])?;
    assert!(int_of(&field(&record, "updated_at_ms"))? > before);
    let terminal =
        text("ERR terminal flow state requires FLOW.COMPLETE, FLOW.FAIL, or FLOW.CANCEL");
    assert_eq!(
        steps("named-completed", "completed")?,
        Term::List(vec![Term::List(vec![text("error"), terminal])])
    );
    Ok(())
}

#[test]
fn run_steps_many_runs_step_chains_in_one_command() -> TestResult {
    // Elixir: "flow run_steps_many runs deterministic step chains in one
    // public command".
    let node = open(&directory("audit-write-steps")?, 4)?;
    let now = now()?;
    let ids = ["rs-a", "rs-b"];
    let states = ["reserve_inventory", "charge_card", "send_email"];
    let reply = ok(
        &node,
        "FLOW.RUN_STEPS_MANY",
        &[
            ("items", step_items(&ids, "rs")),
            ("type", text("rs-type")),
            (
                "states",
                Term::List(states.iter().map(|state| text(state)).collect()),
            ),
            ("worker", text("contract-run-steps-worker")),
            ("lease_ms", int(7_000)),
            ("now_ms", int(now)),
            ("result", map(&[("ok", Term::Bool(true))])),
        ],
    )?;
    assert_eq!(reply, text("OK"));
    let partition = [("partition_key", text("rs")), ("count", int(10))];
    for id in ids {
        let record = get(&node, id, &partition[..1])?;
        assert_eq!(field(&record, "state"), text("completed"));
        assert_eq!(field(&record, "run_state"), text("send_email"));
        assert_eq!(field(&record, "fencing_token"), int(3));
        assert_eq!(field(&record, "lease_token"), Term::Nil);
        let names: Vec<Term> = events(&node, id, &partition)?
            .iter()
            .map(|event| field(event, "event"))
            .collect();
        assert_eq!(
            names,
            vec![
                text("created"),
                text("step_continued"),
                text("step_continued"),
                text("completed")
            ]
        );
    }
    Ok(())
}

#[test]
fn run_steps_many_refuses_duplicate_ids_without_creating_any() -> TestResult {
    // Elixir: "flow run_steps_many rejects duplicate ids without partial
    // creation".
    let node = open(&directory("audit-write-steps-dup")?, 4)?;
    let now = now()?;
    let reply = ok(
        &node,
        "FLOW.RUN_STEPS_MANY",
        &[
            ("items", step_items(&["dup", "dup", "other"], "dup-p")),
            ("type", text("dup-type")),
            ("steps", int(3)),
            ("worker", text("contract-run-steps-worker")),
            ("now_ms", int(now)),
        ],
    )?;
    let duplicate = Term::List(vec![text("error"), text("ERR flow duplicate id in batch")]);
    assert_eq!(
        reply,
        Term::List(vec![duplicate.clone(), duplicate.clone(), duplicate])
    );
    assert_eq!(
        get(&node, "other", &[("partition_key", text("dup-p"))])?,
        Term::Nil
    );
    Ok(())
}

#[test]
fn public_limits_refuse_long_types_and_states_and_leave_the_flow_alone() -> TestResult {
    // Elixir: "flow create rejects metadata that cannot fit its QueryRow
    // before routing" and "flow transition rejects an unencodable QueryRow
    // atomically": the public limits (type 128 bytes, state 64) refuse
    // first; the QueryRow bound behind them is Elixir's own.
    let node = open(&directory("audit-write-limits")?, 4)?;
    let partition = [("partition_key", text("limits"))];
    let long_type = "t".repeat(129);
    let created = run(
        &node,
        "FLOW.CREATE",
        &[
            ("id", text("lt")),
            ("type", text(&long_type)),
            ("state", text("queued")),
            ("partition_key", text("limits")),
            ("now_ms", int(1_000)),
        ],
    )?;
    assert_eq!(refusal(created)?, "ERR flow type is longer than 128 bytes");
    assert_eq!(get(&node, "lt", &partition)?, Term::Nil);
    ok(
        &node,
        "FLOW.CREATE",
        &[
            ("id", text("ls")),
            ("type", text("limits")),
            ("state", text("queued")),
            ("partition_key", text("limits")),
            ("run_at_ms", int(1_000)),
            ("now_ms", int(1_000)),
        ],
    )?;
    let moved = run(
        &node,
        "FLOW.TRANSITION",
        &[
            ("id", text("ls")),
            ("from_state", text("queued")),
            ("to_state", text(&"s".repeat(65))),
            ("fencing_token", int(0)),
            ("partition_key", text("limits")),
            ("run_at_ms", int(1_001)),
            ("now_ms", int(1_001)),
        ],
    )?;
    assert_eq!(refusal(moved)?, "ERR flow state is longer than 64 bytes");
    let record = get(&node, "ls", &partition)?;
    assert_eq!(field(&record, "state"), text("queued"));
    assert_eq!(field(&record, "version"), int(1));
    Ok(())
}
