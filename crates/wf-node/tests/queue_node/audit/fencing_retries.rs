//! `flow/fencing_retry_test.exs`: a client that resends a completion,
//! failure, transition or retry after losing the committed reply gets
//! `OK` and changes nothing; a resend after a later change, or with a
//! different terminal effect, is a stale lease.

use wf_application::commands::flow::Term;
use wf_node::composition::QueueNode;

use super::{claimed, events, field, int, ok, run, text};
use crate::{TestResult, directory, open};

/// Creates `id` of `flow_type` queued at `now` and claims it.
fn create_and_claim(node: &QueueNode, flow_type: &str, id: &str, now: i64) -> TestResult<Term> {
    ok(
        node,
        "FLOW.CREATE",
        &[
            ("id", text(id)),
            ("type", text(flow_type)),
            ("state", text("queued")),
            ("payload", text("payload")),
            ("now_ms", int(now)),
            ("run_at_ms", int(now)),
        ],
    )?;
    let records = claimed(
        node,
        &[
            ("type", text(flow_type)),
            ("state", text("queued")),
            ("limit", int(1)),
            ("worker", text("fencing-worker")),
            ("now_ms", int(now)),
        ],
    )?;
    match records.as_slice() {
        [record] => Ok(record.clone()),
        _ => Err(format!("one claim expected, got {records:?}").into()),
    }
}

/// Runs `command` on the claim with `extra` at `now`.
fn on_claim(
    node: &QueueNode,
    command: &str,
    lease: &Term,
    now: i64,
    extra: &[(&str, Term)],
) -> TestResult<Result<Term, String>> {
    let mut entries = vec![
        ("id", field(lease, "id")),
        ("partition_key", field(lease, "partition_key")),
        ("lease_token", field(lease, "lease_token")),
        ("fencing_token", field(lease, "fencing_token")),
        ("now_ms", int(now)),
    ];
    entries.extend(extra.iter().cloned());
    Ok(run(node, command, &entries)?.map_err(|error| error.to_string()))
}

fn count(node: &QueueNode, lease: &Term, event: &str) -> TestResult<usize> {
    let id = match field(lease, "id") {
        Term::Bin(id) => String::from_utf8(id)?,
        other => return Err(format!("an id expected, got {other:?}").into()),
    };
    let partition = [("partition_key", field(lease, "partition_key"))];
    Ok(events(node, &id, &partition)?
        .iter()
        .filter(|fields| field(fields, "event") == text(event))
        .count())
}

#[test]
fn a_resent_completion_failure_or_transition_is_a_no_op() -> TestResult {
    // Elixir: "complete retry after committed response loss is a no-op
    // success", "fail retry ..." and "transition retry ...".
    let node = open(&directory("audit-fence-resend")?, 4)?;
    for (command, flow_type, event, extra) in [
        (
            "FLOW.COMPLETE",
            "fence-complete",
            "completed",
            vec![("result", text("first-result"))],
        ),
        (
            "FLOW.FAIL",
            "fence-fail",
            "failed",
            vec![("error", text("first-error"))],
        ),
        (
            "FLOW.TRANSITION",
            "fence-transition",
            "transitioned",
            vec![
                ("from_state", text("running")),
                ("to_state", text("waiting")),
                ("payload", text("next-payload")),
            ],
        ),
    ] {
        let lease = create_and_claim(&node, flow_type, flow_type, 1_000)?;
        assert_eq!(
            on_claim(&node, command, &lease, 1_100, &extra)?,
            Ok(text("OK")),
            "{command}"
        );
        assert_eq!(
            on_claim(&node, command, &lease, 1_200, &extra)?,
            Ok(text("OK")),
            "{command}"
        );
        assert_eq!(count(&node, &lease, event)?, 1, "{command}");
    }
    Ok(())
}

#[test]
fn a_resent_retry_is_a_no_op_until_the_flow_moves_on() -> TestResult {
    // Elixir: "retry retry after committed response loss is guarded by
    // latest history event" and "retry retry without explicit run_at is a
    // no-op while scheduled in original state".
    let node = open(&directory("audit-fence-retry")?, 4)?;
    let lease = create_and_claim(&node, "fence-retry", "fence-retry", 4_000)?;
    let retry = [
        ("error", text("transient-error")),
        ("run_at_ms", int(5_000)),
    ];
    assert_eq!(
        on_claim(&node, "FLOW.RETRY", &lease, 4_100, &retry)?,
        Ok(text("OK"))
    );
    assert_eq!(
        on_claim(&node, "FLOW.RETRY", &lease, 4_200, &retry)?,
        Ok(text("OK"))
    );
    assert_eq!(count(&node, &lease, "retry")?, 1);
    let moved = run(
        &node,
        "FLOW.TRANSITION",
        &[
            ("id", field(&lease, "id")),
            ("partition_key", field(&lease, "partition_key")),
            ("from_state", text("queued")),
            ("to_state", text("waiting")),
            ("fencing_token", field(&lease, "fencing_token")),
            ("now_ms", int(4_300)),
        ],
    )?;
    moved?;
    assert_eq!(
        on_claim(&node, "FLOW.RETRY", &lease, 4_400, &retry)?,
        Err("ERR stale flow lease".to_owned())
    );

    let policy = create_and_claim(&node, "fence-policy-retry", "fence-policy-retry", 5_000)?;
    let error = [("error", text("transient-error"))];
    assert_eq!(
        on_claim(&node, "FLOW.RETRY", &policy, 5_100, &error)?,
        Ok(text("OK"))
    );
    assert_eq!(
        on_claim(&node, "FLOW.RETRY", &policy, 5_200, &error)?,
        Ok(text("OK"))
    );
    assert_eq!(count(&node, &policy, "retry")?, 1);
    Ok(())
}

#[test]
fn the_same_fence_does_not_turn_another_terminal_effect_into_success() -> TestResult {
    // Elixir: "same fencing token does not turn a different terminal effect
    // into success".
    let node = open(&directory("audit-fence-other-effect")?, 4)?;
    let lease = create_and_claim(&node, "fence-other", "fence-other", 6_000)?;
    let failed = on_claim(
        &node,
        "FLOW.FAIL",
        &lease,
        6_100,
        &[("error", text("failed"))],
    )?;
    assert_eq!(failed, Ok(text("OK")));
    let completed = on_claim(
        &node,
        "FLOW.COMPLETE",
        &lease,
        6_200,
        &[("result", text("done"))],
    )?;
    assert_eq!(completed, Err("ERR stale flow lease".to_owned()));
    Ok(())
}
