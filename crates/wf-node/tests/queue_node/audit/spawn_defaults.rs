//! `commands/flow_test` "dispatches Flow spawn children through Rust AST"
//! (rest-of-suite audit): a spawn's create options are its children's, as
//! Elixir builds children with `create_many_attrs(children, opts, ...)`;
//! a child's own option wins (audit gap G74).

use wf_application::commands::flow::Term;

use super::{field, get, int, map, ok, text};
use crate::{TestResult, directory, open};

#[test]
fn a_spawn_s_create_options_reach_every_child_that_sets_none() -> TestResult {
    let node = open(&directory("audit-rest-spawn-defaults")?, 4)?;
    ok(
        &node,
        "FLOW.CREATE",
        &[
            ("id", text("parent")),
            ("type", text("parent")),
            ("state", text("dispatch")),
            ("partition_key", text("p")),
        ],
    )?;
    let children = Term::List(vec![
        map(&[("id", text("c1")), ("type", text("child"))]),
        map(&[
            ("id", text("c2")),
            ("type", text("child")),
            ("history_max_events", int(9)),
            ("priority", int(1)),
        ]),
    ]);
    ok(
        &node,
        "FLOW.SPAWN_CHILDREN",
        &[
            ("id", text("parent")),
            ("partition_key", text("p")),
            ("group_id", text("g")),
            ("wait", text("none")),
            ("success", text("ok")),
            ("failure", text("bad")),
            ("from_state", text("dispatch")),
            ("fencing_token", int(0)),
            ("retention_ttl_ms", int(60_000)),
            ("history_hot_max_events", int(2)),
            ("history_max_events", int(5)),
            ("max_active_ms", int(40_000)),
            ("priority", int(2)),
            ("children", children),
        ],
    )?;
    let read = |id: &str| get(&node, id, &[("partition_key", text("p"))]);
    let c1 = read("c1")?;
    for (key, value) in [
        ("retention_ttl_ms", 60_000),
        ("history_hot_max_events", 2),
        ("history_max_events", 5),
        ("max_active_ms", 40_000),
        ("priority", 2),
    ] {
        assert_eq!(field(&c1, key), int(value), "c1 {key}");
    }
    let c2 = read("c2")?;
    for (key, value) in [
        ("retention_ttl_ms", 60_000),
        ("history_max_events", 9),
        ("priority", 1),
    ] {
        assert_eq!(field(&c2, key), int(value), "c2 {key}");
    }
    Ok(())
}
