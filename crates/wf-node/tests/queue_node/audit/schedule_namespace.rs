//! `flow/schedule_test.exs`, the schedule namespace: public Flow commands
//! may not read or change a schedule's internal Flow, by id (GET, every
//! mutation, rewind, batch items, a spawn's parent, start-and-claim) or by
//! type (create, claim); its history stays readable.

use wf_application::commands::flow::Term;
use wf_node::composition::QueueNode;

use super::schedule_api::quiet;
use super::{field, int, map, ok, refusal, run, text};
use crate::TestResult;

fn refused(node: &QueueNode, name: &str, entries: &[(&str, Term)]) -> TestResult<String> {
    refusal(run(node, name, entries)?)
}

/// The internal partition name of schedule `id`.
pub(super) fn internal_partition(id: &str) -> TestResult<String> {
    let wf_application::commands::queue::Partition::Named(name) =
        wf_application::commands::flow::schedule::partition(id.as_bytes())
    else {
        return Err("a named partition expected".into());
    };
    Ok(String::from_utf8(name)?)
}

#[test]
fn public_flow_commands_keep_out_of_the_schedule_namespace() -> TestResult {
    // Elixir: "public flow APIs reject internal schedule namespace" and
    // "public callers cannot forge the internal schedule capability", with
    // every command that guards `reject_reserved_id/2`; the schedule's
    // history stays readable.
    let node = quiet("audit-schedule-namespace")?;
    ok(
        &node,
        "FLOW.SCHEDULE.CREATE",
        &[
            ("id", text("kept")),
            ("kind", text("one_shot")),
            ("at_ms", int(9_000)),
            ("now_ms", int(0)),
            (
                "target",
                map(&[("id", text("kept-target")), ("type", text("scheduled"))]),
            ),
        ],
    )?;
    let internal = "__ferricstore_schedule__:kept";
    let partition = text(&internal_partition("kept")?);
    let reserved_id = "ERR flow id is reserved for internal use";
    let reserved_type = "ERR flow type is reserved for internal use";
    assert_eq!(
        refused(
            &node,
            "FLOW.CREATE",
            &[("id", text("x")), ("type", text("__ferricstore_schedule"))]
        )?,
        reserved_type
    );
    assert_eq!(
        refused(
            &node,
            "FLOW.CREATE",
            &[("id", text(internal)), ("type", text("regular"))]
        )?,
        reserved_id
    );
    let at = |extra: &[(&'static str, Term)]| {
        let mut entries = vec![("id", text(internal)), ("partition_key", partition.clone())];
        entries.extend(extra.iter().cloned());
        entries
    };
    for (name, entries) in [
        ("FLOW.GET", at(&[])),
        (
            "FLOW.GET",
            at(&[("__ferricstore_internal__", Term::Bool(true))]),
        ),
        ("FLOW.CANCEL", at(&[("fencing_token", int(0))])),
        (
            "FLOW.TRANSITION",
            at(&[
                ("from_state", text("active")),
                ("to_state", text("paused")),
                ("fencing_token", int(0)),
            ]),
        ),
        ("FLOW.REWIND", at(&[("to_event", text("1-0"))])),
        (
            "FLOW.COMPLETE",
            at(&[("lease_token", text("t")), ("fencing_token", int(1))]),
        ),
    ] {
        let refused_text = refused(&node, name, &entries)?;
        assert!(
            refused_text == reserved_id
                || refused_text.starts_with("ERR native unknown flow option"),
            "{name}: {refused_text}"
        );
    }
    batch_spawn_and_start(&node, internal, &partition)?;
    let claim = super::claim(
        &node,
        "FLOW.CLAIM_DUE",
        &[
            ("type", text("__ferricstore_schedule")),
            ("worker", text("public-worker")),
        ],
    )?;
    assert_eq!(refusal(claim)?, reserved_type);
    let kept = ok(&node, "FLOW.SCHEDULE.GET", &[("id", text("kept"))])?;
    assert_eq!(field(&kept, "state"), text("active"));
    let history = ok(
        &node,
        "FLOW.HISTORY",
        &[("id", text(internal)), ("partition_key", partition)],
    )?;
    assert!(matches!(history, Term::List(ref events) if !events.is_empty()));
    Ok(())
}

/// A batch item, a spawn's parent and a start's id in the schedule
/// namespace are refused by name.
fn batch_spawn_and_start(node: &QueueNode, internal: &str, partition: &Term) -> TestResult {
    let reserved_id = "ERR flow id is reserved for internal use";
    let at = |extra: &[(&'static str, Term)]| {
        let mut entries = vec![("id", text(internal)), ("partition_key", partition.clone())];
        entries.extend(extra.iter().cloned());
        entries
    };
    let batch = refused(
        node,
        "FLOW.CANCEL_MANY",
        &[(
            "items",
            Term::List(vec![map(&at(&[("fencing_token", int(0))]))]),
        )],
    )?;
    assert_eq!(batch, reserved_id);
    let spawned = super::children_rules::spawn(
        node,
        internal,
        vec![super::children_rules::child("internal-child")],
        &[
            ("partition_key", partition.clone()),
            ("lease_token", text("t")),
            ("fencing_token", int(1)),
        ],
    )?;
    assert_eq!(spawned, Err(reserved_id.to_owned()));
    let started = refused(
        node,
        "FLOW.START_AND_CLAIM",
        &[
            ("id", text(internal)),
            ("type", text("regular")),
            ("initial_state", text("queued")),
            ("worker", text("w")),
        ],
    )?;
    assert_eq!(started, reserved_id);
    Ok(())
}
