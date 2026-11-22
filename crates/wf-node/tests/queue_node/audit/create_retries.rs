//! `flow/create_running_validation_test.exs` and `flow/idempotent_create_
//! after_progress_test.exs`: a create refused for `running` reserves
//! nothing, and an idempotent create resent after the Flow moved on still
//! answers it, while a different request for that id conflicts.

use wf_application::commands::flow::Term;
use wf_node::composition::QueueNode;

use super::{claimed, field, get, int, ok, run, text};
use crate::{TestResult, directory, open};

const P: &str = "running-validation";

fn create(node: &QueueNode, id: &str, extra: &[(&str, Term)]) -> TestResult<Result<Term, String>> {
    let mut entries = vec![
        ("id", text(id)),
        ("type", text("running-validation")),
        ("partition_key", text(P)),
    ];
    entries.retain(|(key, _)| !extra.iter().any(|(given, _)| given == key));
    entries.extend(extra.iter().cloned());
    Ok(run(node, "FLOW.CREATE", &entries)?.map_err(|error| error.to_string()))
}

#[test]
fn a_create_refused_for_running_reserves_nothing() -> TestResult {
    // Elixir: "public create rejects running without reserving the identity
    // or disrupting writes" (its batch and start-and-claim cases are in
    // `audit::create_rules` and `audit::steps_contract`).
    let node = open(&directory("audit-create-running-identity")?, 4)?;
    let refused = create(&node, "flow-1", &[("state", text("running"))])?;
    assert_eq!(
        refused,
        Err("ERR flow running state is only entered by FLOW.CLAIM_DUE".to_owned())
    );
    let in_p = [("partition_key", text(P))];
    assert_eq!(get(&node, "flow-1", &in_p)?, Term::Nil);
    create(&node, "flow-1", &[("state", text("queued"))])??;
    let record = get(&node, "flow-1", &in_p)?;
    assert_eq!(field(&record, "state"), text("queued"));
    assert_eq!(field(&record, "lease_owner"), Term::Nil);
    Ok(())
}

fn idempotent(
    node: &QueueNode,
    id: &str,
    flow_type: &str,
    payload: &str,
) -> TestResult<Result<Term, String>> {
    create(
        node,
        id,
        &[
            ("type", text(flow_type)),
            ("state", text("queued")),
            ("payload", text(payload)),
            ("idempotent", Term::Bool(true)),
        ],
    )
}

fn claim(node: &QueueNode, flow_type: &str) -> TestResult<Term> {
    let records = claimed(
        node,
        &[
            ("type", text(flow_type)),
            ("state", text("queued")),
            ("partition_key", text(P)),
            ("limit", int(1)),
            ("worker", text("w")),
        ],
    )?;
    match records.as_slice() {
        [record] => Ok(record.clone()),
        _ => Err(format!("one claim expected, got {records:?}").into()),
    }
}

#[test]
fn an_idempotent_create_resent_after_progress_answers_the_flow() -> TestResult {
    // Elixir: "a retry after a worker claimed the Flow answers it", "a
    // retry after the Flow completed answers it" and "a different request
    // for a claimed Flow still conflicts".
    let node = open(&directory("audit-create-idempotent-progress")?, 4)?;
    idempotent(&node, "claimed", "claimed-type", "payload:claimed")??;
    claim(&node, "claimed-type")?;
    idempotent(&node, "claimed", "claimed-type", "payload:claimed")??;
    let conflict = Err("ERR flow idempotency conflict".to_owned());
    assert_eq!(
        idempotent(&node, "claimed", "claimed-type", "other")?,
        conflict
    );
    assert_eq!(
        idempotent(&node, "claimed", "other-type", "payload:claimed")?,
        conflict
    );

    idempotent(&node, "done", "done-type", "payload:done")??;
    let lease = claim(&node, "done-type")?;
    ok(
        &node,
        "FLOW.COMPLETE",
        &[
            ("id", text("done")),
            ("partition_key", text(P)),
            ("lease_token", field(&lease, "lease_token")),
            ("fencing_token", field(&lease, "fencing_token")),
            ("result", text("done")),
        ],
    )?;
    idempotent(&node, "done", "done-type", "payload:done")??;
    Ok(())
}
