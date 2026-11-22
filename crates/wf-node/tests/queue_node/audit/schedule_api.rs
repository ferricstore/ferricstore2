//! `flow/schedule_test.exs`, the schedule API's own rules over the wire:
//! each command takes only its own options, ids are non-empty texts,
//! operator changes keep to their `expected_state`/`expected_version`
//! guards, GET keeps its internal options, and a target must be a map that
//! can create a Flow, inline values stay small, and time inputs stay within
//! the exact-integer and calendar bounds. (Elixir's keyword-list and
//! duplicate-option checks have no wire form: a wire map carries each key
//! once.)

use wf_application::commands::flow::Term;
use wf_node::composition::{QueueNode, QueueNodeConfig};

use super::{field, int, map, ok, refusal, run, text};
use crate::{TestResult, directory, open_with};

const MAX_EXACT: i64 = 9_007_199_254_740_991;

/// A node with no background poll, so only the test fires schedules.
pub(super) fn quiet(tag: &str) -> TestResult<QueueNode> {
    open_with(
        &directory(tag)?,
        QueueNodeConfig {
            shards: 4,
            schedule_poll: None,
            ..QueueNodeConfig::default()
        },
    )
}

fn one_shot(target_id: &str, at: i64) -> Vec<(&'static str, Term)> {
    vec![
        ("kind", text("one_shot")),
        ("at_ms", int(at)),
        ("now_ms", int(0)),
        (
            "target",
            map(&[("id", text(target_id)), ("type", text("scheduled"))]),
        ),
    ]
}

fn create(node: &QueueNode, id: &str, options: &[(&str, Term)]) -> TestResult<Term> {
    let mut entries = vec![("id", text(id))];
    entries.extend(options.iter().cloned());
    ok(node, "FLOW.SCHEDULE.CREATE", &entries)
}

fn refused(node: &QueueNode, name: &str, entries: &[(&str, Term)]) -> TestResult<String> {
    refusal(run(node, name, entries)?)
}

#[test]
fn each_command_takes_only_its_own_options() -> TestResult {
    // Elixir: "schedule creation rejects unknown and duplicate options",
    // "schedule listing rejects unknown and duplicate options" and
    // "schedule operations reject unknown and duplicate options before
    // storage access". A name no command knows is the wire's own refusal; a
    // Flow option the command does not take is the schedule's.
    let node = quiet("audit-schedule-options")?;
    let mut typo = one_shot("typo-target", 1_000);
    typo.push(("catchup_polciy", text("fire_once")));
    typo.push(("id", text("typo")));
    assert_eq!(
        refused(&node, "FLOW.SCHEDULE.CREATE", &typo)?,
        "ERR native unknown flow option catchup_polciy"
    );
    let mut foreign = one_shot("foreign-target", 1_000);
    foreign.push(("worker", text("w")));
    foreign.push(("id", text("foreign")));
    assert_eq!(
        refused(&node, "FLOW.SCHEDULE.CREATE", &foreign)?,
        "ERR unsupported flow schedule option worker"
    );
    let id = ("id", text("operations"));
    for (name, entries) in [
        ("FLOW.SCHEDULE.GET", vec![id.clone(), ("now_ms", int(1))]),
        (
            "FLOW.SCHEDULE.FIRE",
            vec![id.clone(), ("worker", text("w"))],
        ),
        (
            "FLOW.SCHEDULE.PAUSE",
            vec![id.clone(), ("payload", text("p"))],
        ),
        (
            "FLOW.SCHEDULE.RESUME",
            vec![id.clone(), ("fire_at_ms", int(1))],
        ),
        ("FLOW.SCHEDULE.DELETE", vec![id.clone(), ("limit", int(1))]),
        ("FLOW.SCHEDULE.FIRE_DUE", vec![("payload", text("p"))]),
        ("FLOW.SCHEDULE.LIST", vec![("now_ms", int(1))]),
    ] {
        let (key, _) = entries.last().ok_or("no option")?;
        assert_eq!(
            refused(&node, name, &entries)?,
            format!("ERR unsupported flow schedule option {key}"),
            "{name}"
        );
        let mut unknown = entries.clone();
        unknown.push(("surprise", Term::Bool(true)));
        assert_eq!(
            refused(&node, name, &unknown)?,
            "ERR native unknown flow option surprise",
            "{name}"
        );
    }
    Ok(())
}

#[test]
fn schedule_ids_are_non_empty_texts() -> TestResult {
    // Elixir: "schedule APIs report invalid identifiers before storage
    // access".
    let node = quiet("audit-schedule-ids")?;
    for name in [
        "FLOW.SCHEDULE.CREATE",
        "FLOW.SCHEDULE.GET",
        "FLOW.SCHEDULE.FIRE",
        "FLOW.SCHEDULE.PAUSE",
        "FLOW.SCHEDULE.RESUME",
        "FLOW.SCHEDULE.DELETE",
    ] {
        let mut empty = one_shot("ids-target", 1_000);
        empty.push(("id", text("")));
        if name != "FLOW.SCHEDULE.CREATE" {
            empty = vec![("id", text(""))];
        }
        assert_eq!(
            refused(&node, name, &empty)?,
            "ERR flow schedule id must be a non-empty string",
            "{name}"
        );
        assert_eq!(
            refused(&node, name, &[("id", int(7))])?,
            "ERR native field id must be binary",
            "{name}"
        );
    }
    Ok(())
}

#[test]
fn operator_changes_keep_to_their_expected_state_and_version() -> TestResult {
    // Elixir: "operator schedule mutations reject stale expected state and
    // version".
    let node = quiet("audit-schedule-guards")?;
    create(
        &node,
        "guarded",
        &[
            ("kind", text("interval")),
            ("every_ms", int(60_000)),
            ("start_at_ms", int(61_000)),
            ("now_ms", int(1_000)),
            (
                "target",
                map(&[
                    ("id_prefix", text("guarded-target")),
                    ("type", text("guarded-schedule")),
                ]),
            ),
        ],
    )?;
    let created = ok(&node, "FLOW.SCHEDULE.GET", &[("id", text("guarded"))])?;
    let Term::Int(version) = field(&created, "version") else {
        return Err(format!("a version expected in {created:?}").into());
    };
    let guarded = |state: &str, version: i64, now: i64| {
        vec![
            ("id", text("guarded")),
            ("expected_state", text(state)),
            ("expected_version", int(version)),
            ("now_ms", int(now)),
        ]
    };
    let stale = refused(
        &node,
        "FLOW.SCHEDULE.PAUSE",
        &guarded("active", version + 1, 1_001),
    )?;
    assert!(stale.contains("changed"), "{stale}");
    let unchanged = ok(&node, "FLOW.SCHEDULE.GET", &[("id", text("guarded"))])?;
    assert_eq!(
        (field(&unchanged, "state"), field(&unchanged, "version")),
        (text("active"), int(version))
    );
    let wrong_state = refused(
        &node,
        "FLOW.SCHEDULE.PAUSE",
        &guarded("paused", version, 1_001),
    )?;
    assert!(wrong_state.contains("changed"), "{wrong_state}");
    let paused = ok(
        &node,
        "FLOW.SCHEDULE.PAUSE",
        &guarded("active", version, 1_002),
    )?;
    assert_eq!(field(&paused, "state"), text("paused"));
    for name in ["FLOW.SCHEDULE.FIRE", "FLOW.SCHEDULE.DELETE"] {
        let stale = refused(&node, name, &guarded("active", version, 1_002))?;
        assert!(stale.contains("changed"), "{name}: {stale}");
    }
    let after = ok(&node, "FLOW.SCHEDULE.GET", &[("id", text("guarded"))])?;
    assert_eq!(field(&after, "state"), text("paused"));
    let Term::Int(paused_version) = field(&after, "version") else {
        return Err("a version expected".into());
    };
    assert_eq!(
        refused(
            &node,
            "FLOW.SCHEDULE.RESUME",
            &[("id", text("guarded")), ("expected_version", int(-1))]
        )?,
        "ERR flow schedule expected_version must be a non-negative integer"
    );
    assert_eq!(
        refused(
            &node,
            "FLOW.SCHEDULE.RESUME",
            &[("id", text("guarded")), ("expected_state", text(""))]
        )?,
        "ERR flow schedule expected_state must be a non-empty string"
    );
    let resumed = ok(
        &node,
        "FLOW.SCHEDULE.RESUME",
        &guarded("paused", paused_version, 1_003),
    )?;
    assert_eq!(field(&resumed, "state"), text("active"));
    Ok(())
}

#[test]
fn a_guarded_overwrite_needs_the_schedule_as_expected() -> TestResult {
    // Elixir's create: with `overwrite` and a guard, the replacement holds
    // only for the schedule the caller read; without `overwrite` a duplicate
    // is still refused.
    let node = quiet("audit-schedule-overwrite-guard")?;
    create(&node, "replace", &one_shot("replace-first", 5_000))?;
    let read = ok(&node, "FLOW.SCHEDULE.GET", &[("id", text("replace"))])?;
    let Term::Int(version) = field(&read, "version") else {
        return Err("a version expected".into());
    };
    let mut stale = one_shot("replace-second", 6_000);
    stale.extend([
        ("id", text("replace")),
        ("overwrite", Term::Bool(true)),
        ("expected_version", int(version + 1)),
    ]);
    let refused_text = refused(&node, "FLOW.SCHEDULE.CREATE", &stale)?;
    assert!(refused_text.contains("changed"), "{refused_text}");
    let mut current = one_shot("replace-second", 6_000);
    current.extend([
        ("id", text("replace")),
        ("overwrite", Term::Bool(true)),
        ("expected_version", int(version)),
        ("expected_state", text("active")),
    ]);
    let replaced = ok(&node, "FLOW.SCHEDULE.CREATE", &current)?;
    assert_eq!(field(&replaced, "next_run_at_ms"), int(6_000));
    let mut missing = one_shot("missing-target", 6_000);
    missing.extend([
        ("id", text("missing")),
        ("overwrite", Term::Bool(true)),
        ("expected_version", int(1)),
    ]);
    assert_eq!(
        refused(&node, "FLOW.SCHEDULE.CREATE", &missing)?,
        "ERR flow schedule not found"
    );
    Ok(())
}

#[test]
fn get_keeps_its_internal_routing_and_payload_options() -> TestResult {
    // Elixir: "schedule get keeps internal routing and payload options
    // authoritative".
    let node = quiet("audit-schedule-get-options")?;
    let created = create(&node, "protected", &one_shot("protected-target", 1_000))?;
    let fetched = ok(
        &node,
        "FLOW.SCHEDULE.GET",
        &[
            ("id", text("protected")),
            ("partition_key", text("wrong-partition")),
            ("payload", Term::Bool(false)),
            ("payload_max_bytes", int(1)),
        ],
    )?;
    assert_eq!(field(&fetched, "id"), field(&created, "id"));
    assert_eq!(field(&fetched, "kind"), text("one_shot"));
    assert_eq!(field(&fetched, "target"), field(&created, "target"));
    Ok(())
}

#[test]
fn a_target_is_a_map_that_can_create_a_flow() -> TestResult {
    // Elixir: "schedule creation rejects malformed, unknown, and duplicate
    // target fields", "schedule rejects reserved target ids and prefixes"
    // and "schedule rejects target definitions that cannot create a flow".
    let node = quiet("audit-schedule-targets")?;
    let with_target = |id: &str, kind: &str, target: Term| {
        let mut entries = vec![("id", text(id)), ("kind", text(kind)), ("target", target)];
        if kind == "interval" {
            entries.push(("every_ms", int(1_000)));
        }
        entries
    };
    for (target, expected) in [
        (
            Term::List(vec![text("not_a_pair")]),
            "ERR flow schedule target must be a keyword list or map",
        ),
        (text("scheduled"), "ERR flow schedule target is required"),
        (
            map(&[("type", text("scheduled")), ("surprise", Term::Bool(true))]),
            "ERR unsupported flow schedule target field surprise",
        ),
        (
            map(&[
                ("id", text("__ferricstore_schedule__:target")),
                ("type", text("scheduled")),
            ]),
            "ERR scheduled target id is reserved for internal use",
        ),
        (
            map(&[
                ("id", text("reserved-type")),
                ("type", text("__ferricstore_schedule")),
            ]),
            "ERR flow type is reserved for internal use",
        ),
    ] {
        assert_eq!(
            refused(
                &node,
                "FLOW.SCHEDULE.CREATE",
                &with_target("shape", "one_shot", target)
            )?,
            expected
        );
    }
    for (target, expected) in [
        (
            map(&[
                ("id_prefix", text("__ferricstore_schedule__:target")),
                ("type", text("scheduled")),
            ]),
            "ERR scheduled target id_prefix is reserved for internal use",
        ),
        (
            map(&[
                ("id_prefix", text("priority")),
                ("type", text("scheduled")),
                ("priority", text("high")),
            ]),
            "ERR flow priority must be between 0 and 2",
        ),
    ] {
        assert_eq!(
            refused(
                &node,
                "FLOW.SCHEDULE.CREATE",
                &with_target("recurring", "interval", target)
            )?,
            expected
        );
    }
    Ok(())
}

#[test]
fn inline_target_values_stay_small() -> TestResult {
    // Elixir: "schedule rejects large inline target payloads" and "schedule
    // rejects large inline target named values": past 8 KiB a target names
    // its values by reference.
    let node = quiet("audit-schedule-inline")?;
    let large = text(&"x".repeat(9_000));
    for (key, value, expected) in [
        (
            "payload",
            large.clone(),
            "ERR flow schedule payload too large; use payload_ref/value_refs",
        ),
        (
            "values",
            map(&[("doc", large)]),
            "ERR flow schedule values too large; use values_ref/value_refs",
        ),
    ] {
        let target = map(&[
            ("id", text("inline-target")),
            ("type", text("scheduled")),
            (key, value),
        ]);
        assert_eq!(
            refused(
                &node,
                "FLOW.SCHEDULE.CREATE",
                &[
                    ("id", text("inline")),
                    ("kind", text("one_shot")),
                    ("target", target)
                ]
            )?,
            expected
        );
    }
    let fits = map(&[
        ("id", text("fits-target")),
        ("type", text("scheduled")),
        ("payload", text(&"x".repeat(4_000))),
    ]);
    create(
        &node,
        "fits",
        &[
            ("kind", text("one_shot")),
            ("at_ms", int(10_000)),
            ("now_ms", int(0)),
            ("target", fits),
        ],
    )?;
    Ok(())
}

#[test]
fn time_inputs_stay_within_exact_and_calendar_bounds() -> TestResult {
    // Elixir: "schedule time inputs are bounded and cron conversion never
    // raises".
    let node = quiet("audit-schedule-bounds")?;
    let recurring = map(&[("id_prefix", text("bounds")), ("type", text("scheduled"))]);
    let cron = |start: i64| {
        vec![
            ("id", text("bounded-cron")),
            ("kind", text("cron")),
            ("cron", text("* * * * *")),
            ("start_at_ms", int(start)),
            ("now_ms", int(0)),
            ("target", recurring.clone()),
        ]
    };
    assert_eq!(
        refused(&node, "FLOW.SCHEDULE.CREATE", &cron(MAX_EXACT + 1))?,
        format!("ERR flow schedule start_at_ms exceeds maximum {MAX_EXACT}")
    );
    let mut at = one_shot("bounded-target", MAX_EXACT + 1);
    at.push(("id", text("bounded-at")));
    assert_eq!(
        refused(&node, "FLOW.SCHEDULE.CREATE", &at)?,
        format!("ERR flow schedule at_ms exceeds maximum {MAX_EXACT}")
    );
    assert_eq!(
        refused(&node, "FLOW.SCHEDULE.CREATE", &cron(MAX_EXACT))?,
        "ERR flow schedule timestamp is outside supported calendar range"
    );
    Ok(())
}

#[test]
fn listing_names_its_states_and_keeps_ranges_ordered() -> TestResult {
    // Elixir: "schedule listing rejects invalid states and inverted time
    // ranges".
    let node = quiet("audit-schedule-list-options")?;
    let state = refused(&node, "FLOW.SCHEDULE.LIST", &[("state", text("unknown"))])?;
    assert!(
        state.contains("active, paused, running, completed, failed, cancelled, or all"),
        "{state}"
    );
    assert_eq!(
        refused(
            &node,
            "FLOW.SCHEDULE.LIST",
            &[("from_ms", int(2)), ("to_ms", int(1))]
        )?,
        "ERR flow schedule from_ms must be less than or equal to to_ms"
    );
    Ok(())
}
