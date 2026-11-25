//! `ferricstore_server/native/commands_test.exs`, the rows the audit of the
//! rest of the Elixir suite (`docs/reviews/workflow-flow-test-audit-rest.md`)
//! did not find covered by an earlier test.

use wf_application::commands::flow::Term;

use super::{field, get, int, map, ok, text};
use crate::{TestResult, directory, open};

#[test]
fn every_create_path_keeps_its_own_active_time_limit() -> TestResult {
    // Elixir: "native Flow creation opcodes accept max_active_ms": a
    // create, a batch item and a start-and-claim each keep the limit they
    // were given.
    let node = open(&directory("audit-rest-create-active")?, 4)?;
    let flow = |id: &str| [("type", text("native-active")), ("id", text(id))];
    let [kind, id] = flow("one");
    ok(
        &node,
        "FLOW.CREATE",
        &[
            kind,
            id,
            ("state", text("queued")),
            ("max_active_ms", int(10_000)),
        ],
    )?;
    let item = map(&[("id", text("many")), ("max_active_ms", int(20_000))]);
    ok(
        &node,
        "FLOW.CREATE_MANY",
        &[
            ("type", text("native-active")),
            ("state", text("queued")),
            ("partition_key", text("p")),
            ("items", Term::List(vec![item])),
        ],
    )?;
    let [kind, id] = flow("start");
    let started = ok(
        &node,
        "FLOW.START_AND_CLAIM",
        &[
            kind,
            id,
            ("initial_state", text("queued")),
            ("worker", text("w")),
            ("max_active_ms", int(30_000)),
        ],
    )?;
    assert_eq!(field(&started, "max_active_ms"), int(30_000));
    assert_eq!(
        field(&get(&node, "one", &[])?, "max_active_ms"),
        int(10_000)
    );
    let many = get(&node, "many", &[("partition_key", text("p"))])?;
    assert_eq!(field(&many, "max_active_ms"), int(20_000));
    assert_eq!(
        field(&get(&node, "start", &[])?, "max_active_ms"),
        int(30_000)
    );
    Ok(())
}
