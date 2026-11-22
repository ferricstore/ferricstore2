//! `flow/schedule_test.exs`, a schedule's lifecycle: the last cron fire
//! the calendar allows, target ownership a caller cannot forge, overwrite
//! (atomic, and never over a live lease), targets by reference, and the
//! listing's due ranges, reverse order and retention. (Elixir's catalog
//! pages, async projections and internal replace calls are its own.)

use wf_application::commands::flow::Term;
use wf_node::composition::QueueNode;

use super::schedule_api::quiet;
use super::{field, get, int, map, ok, refusal, run, text, texts};
use crate::TestResult;

fn create(node: &QueueNode, id: &str, options: &[(&str, Term)]) -> TestResult<Term> {
    let mut entries = vec![("id", text(id))];
    entries.extend(options.iter().cloned());
    ok(node, "FLOW.SCHEDULE.CREATE", &entries)
}

fn one_shot(at: i64, now: i64, target: &[(&str, Term)]) -> Vec<(&'static str, Term)> {
    vec![
        ("kind", text("one_shot")),
        ("at_ms", int(at)),
        ("now_ms", int(now)),
        ("target", map(target)),
    ]
}

fn fire_due(node: &QueueNode, now: i64) -> TestResult<Term> {
    ok(
        node,
        "FLOW.SCHEDULE.FIRE_DUE",
        &[("now_ms", int(now)), ("worker", text("schedule-test"))],
    )
}

fn schedule(node: &QueueNode, id: &str) -> TestResult<Term> {
    ok(node, "FLOW.SCHEDULE.GET", &[("id", text(id))])
}

fn listed(node: &QueueNode, options: &[(&str, Term)]) -> TestResult<Vec<Term>> {
    match ok(node, "FLOW.SCHEDULE.LIST", options)? {
        Term::List(views) => Ok(views.iter().map(|view| field(view, "id")).collect()),
        other => Err(format!("a list expected, got {other:?}").into()),
    }
}

fn wall() -> TestResult<i64> {
    Ok(i64::try_from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_millis(),
    )?)
}

#[test]
fn the_last_cron_fire_the_calendar_allows_completes_the_schedule() -> TestResult {
    // Elixir: "cron fire at the calendar ceiling completes after emitting
    // exactly one target".
    let node = quiet("audit-schedule-ceiling")?;
    let due: i64 = 253_402_300_740_000;
    let created = create(
        &node,
        "ceiling",
        &[
            ("kind", text("cron")),
            ("cron", text("59 23 31 DEC *")),
            ("start_at_ms", int(due - 60_000)),
            ("now_ms", int(due - 60_000)),
            (
                "target",
                map(&[
                    ("id_prefix", text("ceiling")),
                    ("type", text("ceiling")),
                    ("partition_key", text("ceiling-partition")),
                    ("payload", text("calendar-ceiling")),
                ]),
            ),
        ],
    )?;
    assert_eq!(field(&created, "next_run_at_ms"), int(due));
    let fired = fire_due(&node, due)?;
    assert_eq!(
        (field(&fired, "claimed"), field(&fired, "fired")),
        (int(1), int(1))
    );
    let target = get(
        &node,
        &format!("ceiling:{due}:1"),
        &[
            ("partition_key", text("ceiling-partition")),
            ("payload", Term::Bool(true)),
        ],
    )?;
    assert_eq!(field(&target, "payload"), text("calendar-ceiling"));
    let done = schedule(&node, "ceiling")?;
    assert_eq!(
        [
            field(&done, "state"),
            field(&done, "fire_count"),
            field(&done, "end_reason"),
            field(&done, "next_run_at_ms"),
        ],
        [
            text("completed"),
            int(1),
            text("timestamp_limit"),
            Term::Nil
        ]
    );
    assert_eq!(field(&fire_due(&node, due)?, "claimed"), int(0));
    Ok(())
}

#[test]
fn the_scheduler_correlation_does_not_forge_ownership() -> TestResult {
    // Elixir: "predictable scheduler correlation cannot forge target
    // ownership": the fire fails, the existing Flow is untouched and the
    // schedule keeps its lease without counting a fire.
    let node = quiet("audit-schedule-forged-owner")?;
    let correlation = "__ferricstore_schedule__:forged";
    ok(
        &node,
        "FLOW.CREATE",
        &[
            ("id", text("forged-target")),
            ("type", text("forged-type")),
            ("partition_key", text("forged-partition")),
            ("correlation_id", text(correlation)),
            ("payload", text("already-created")),
            ("now_ms", int(3_499)),
        ],
    )?;
    create(
        &node,
        "forged",
        &one_shot(
            3_500,
            3_499,
            &[
                ("id", text("forged-target")),
                ("type", text("forged-type")),
                ("partition_key", text("forged-partition")),
                ("payload", text("would-create")),
            ],
        ),
    )?;
    let fired = fire_due(&node, 3_500)?;
    assert_eq!(
        (field(&fired, "claimed"), field(&fired, "fired")),
        (int(1), int(0))
    );
    assert_eq!(
        field(&fired, "errors"),
        Term::List(vec![Term::List(vec![
            text("forged"),
            text("ERR scheduled target id already exists with different owner"),
        ])])
    );
    let target = get(
        &node,
        "forged-target",
        &[
            ("partition_key", text("forged-partition")),
            ("payload", Term::Bool(true)),
        ],
    )?;
    assert_eq!(
        (field(&target, "payload"), field(&target, "correlation_id")),
        (text("already-created"), text(correlation))
    );
    let held = schedule(&node, "forged")?;
    assert_eq!(
        (field(&held, "state"), field(&held, "fire_count")),
        (text("running"), int(0))
    );
    Ok(())
}

#[test]
fn an_overwrite_replaces_the_whole_definition_at_once() -> TestResult {
    // Elixir: "schedule overwrite atomically replaces active definition".
    let node = quiet("audit-schedule-overwrite")?;
    let target = |id: &str, flow_type: &str, partition: &str, payload: &str| {
        vec![
            ("id", text(id)),
            ("type", text(flow_type)),
            ("partition_key", text(partition)),
            ("payload", text(payload)),
        ]
    };
    create(
        &node,
        "replaced",
        &one_shot(
            3_600,
            3_500,
            &target("old-target", "old-type", "old-p", "old"),
        ),
    )?;
    let mut replacement = one_shot(
        4_000,
        3_501,
        &target("new-target", "new-type", "new-p", "new"),
    );
    replacement.push(("overwrite", Term::Bool(true)));
    let replaced = create(&node, "replaced", &replacement)?;
    assert_eq!(
        [
            field(&replaced, "state"),
            field(&replaced, "created_at_ms"),
            field(&replaced, "next_run_at_ms"),
            field(&replaced, "fire_count"),
        ],
        [text("active"), int(3_501), int(4_000), int(0)]
    );
    let by_type = |flow_type: &str| {
        listed(
            &node,
            &[
                ("state", text("all")),
                ("target_type", text(flow_type)),
                ("count", int(10)),
            ],
        )
    };
    assert_eq!(by_type("old-type")?, Vec::<Term>::new());
    assert_eq!(by_type("new-type")?, [text("replaced")]);
    assert_eq!(field(&fire_due(&node, 3_600)?, "claimed"), int(0));
    let old = get(&node, "old-target", &[("partition_key", text("old-p"))])?;
    assert_eq!(old, Term::Nil);
    assert_eq!(field(&fire_due(&node, 4_000)?, "fired"), int(1));
    let new = get(
        &node,
        "new-target",
        &[
            ("partition_key", text("new-p")),
            ("payload", Term::Bool(true)),
        ],
    )?;
    assert_eq!(field(&new, "payload"), text("new"));
    Ok(())
}

/// Leaves schedule `id` leased by a fire that failed (its target id is
/// another owner's), with a lease of `lease_ms` from `now`.
fn leased(node: &QueueNode, id: &str, now: i64, lease_ms: i64) -> TestResult {
    let target_id = format!("{id}-taken");
    ok(
        node,
        "FLOW.CREATE",
        &[
            ("id", text(&target_id)),
            ("type", text("manual-owner")),
            ("now_ms", int(now)),
        ],
    )?;
    create(
        node,
        id,
        &one_shot(
            now,
            now,
            &[("id", text(&target_id)), ("type", text("leased-type"))],
        ),
    )?;
    let fired = ok(
        node,
        "FLOW.SCHEDULE.FIRE_DUE",
        &[
            ("now_ms", int(now)),
            ("worker", text("leased-test")),
            ("lease_ms", int(lease_ms)),
        ],
    )?;
    assert_eq!(field(&fired, "claimed"), int(1));
    assert_eq!(field(&schedule(node, id)?, "state"), text("running"));
    Ok(())
}

#[test]
fn an_overwrite_waits_for_a_live_lease_on_the_node_clock() -> TestResult {
    // Elixir: "schedule overwrite rejects a currently leased schedule" and
    // "schedule overwrite cannot expire a live lease with caller time"; a
    // lease the node's clock has passed no longer holds the schedule.
    let node = quiet("audit-schedule-overwrite-leased")?;
    let now = wall()?;
    leased(&node, "live", now, 30_000)?;
    let replacement = |at: i64, caller: i64| {
        let mut entries = one_shot(
            at,
            caller,
            &[("id", text("replacement")), ("type", text("replacement"))],
        );
        entries.extend([("id", text("live")), ("overwrite", Term::Bool(true))]);
        entries
    };
    for (at, caller) in [(now + 1_000, now + 60_000), (now + 120_000, now + 120_000)] {
        assert_eq!(
            refusal(run(
                &node,
                "FLOW.SCHEDULE.CREATE",
                &replacement(at, caller)
            )?)?,
            "ERR flow schedule is currently leased"
        );
    }
    leased(&node, "lapsed", now, 1)?;
    std::thread::sleep(std::time::Duration::from_millis(20));
    let mut over = replacement(now + 1_000, now);
    over.retain(|(key, _)| *key != "id");
    over.push(("id", text("lapsed")));
    let replaced = ok(&node, "FLOW.SCHEDULE.CREATE", &over)?;
    assert_eq!(field(&replaced, "state"), text("active"));
    assert_eq!(field(&schedule(&node, "lapsed")?, "state"), text("active"));
    Ok(())
}

#[test]
fn a_target_can_take_its_payload_and_values_by_reference() -> TestResult {
    // Elixir: "schedule target can use payload_ref and value_refs".
    let node = quiet("audit-schedule-refs")?;
    let partition = ("partition_key", text("refs-partition"));
    let large = map(&[("large", text(&"p".repeat(512)))]);
    let doc = map(&[("doc", text("reservation"))]);
    let payload_ref = field(
        &ok(
            &node,
            "FLOW.VALUE.PUT",
            &[("value", large.clone()), partition.clone()],
        )?,
        "ref",
    );
    let doc_ref = field(
        &ok(
            &node,
            "FLOW.VALUE.PUT",
            &[("value", doc.clone()), partition.clone()],
        )?,
        "ref",
    );
    create(
        &node,
        "refs",
        &one_shot(
            3_750,
            3_750,
            &[
                ("id", text("refs-target")),
                ("type", text("refs-type")),
                partition.clone(),
                ("payload_ref", payload_ref.clone()),
                ("value_refs", map(&[("reservation", doc_ref.clone())])),
            ],
        ),
    )?;
    let fired = fire_due(&node, 3_750)?;
    assert_eq!(field(&fired, "errors"), Term::List(Vec::new()));
    let target = get(
        &node,
        "refs-target",
        &[
            partition,
            ("payload", Term::Bool(true)),
            ("values", texts(&["reservation"])),
        ],
    )?;
    assert_eq!(field(&target, "payload"), large);
    assert_eq!(field(&target, "values"), map(&[("reservation", doc)]));
    assert_eq!(field(&target, "payload_ref"), payload_ref);
    assert_eq!(
        field(&field(&field(&target, "value_refs"), "reservation"), "ref"),
        doc_ref
    );
    Ok(())
}

#[test]
fn listings_keep_finished_schedules_out_of_due_ranges_and_last_in_reverse() -> TestResult {
    // Elixir: "schedule list keeps terminal schedules last when reverse
    // ordering" and "schedule due ranges exclude terminal schedules without
    // a next run".
    let node = quiet("audit-schedule-list-terminal")?;
    let now = wall()?;
    for (id, at) in [("active", now + 200), ("completed", now + 100)] {
        create(
            &node,
            id,
            &one_shot(
                at,
                now,
                &[
                    ("id", text(&format!("{id}-target"))),
                    ("type", text("terminal-order")),
                ],
            ),
        )?;
    }
    let fired = ok(
        &node,
        "FLOW.SCHEDULE.FIRE",
        &[
            ("id", text("completed")),
            ("fire_at_ms", int(now + 100)),
            ("now_ms", int(now + 100)),
        ],
    )?;
    assert_eq!(field(&fired, "fired"), int(1));
    let all = |extra: &[(&str, Term)]| {
        let mut options = vec![
            ("state", text("all")),
            ("target_type", text("terminal-order")),
            ("count", int(10)),
        ];
        options.extend(extra.iter().cloned());
        listed(&node, &options)
    };
    assert_eq!(
        all(&[("rev", Term::Bool(true))])?,
        [text("active"), text("completed")]
    );
    assert_eq!(
        all(&[("from_ms", int(0)), ("to_ms", int(now + 150))])?,
        Vec::<Term>::new()
    );
    Ok(())
}

#[test]
fn retention_removes_a_deleted_schedule_from_listings() -> TestResult {
    // Elixir: "schedule exact catalog tracks retention without a second
    // membership index", through the listing.
    let node = quiet("audit-schedule-retention")?;
    for id in ["first", "second"] {
        create(
            &node,
            id,
            &one_shot(
                14_005,
                4_005,
                &[
                    ("id", text(&format!("{id}-target"))),
                    ("type", text("retained-type")),
                ],
            ),
        )?;
    }
    let cleanup = |node: &QueueNode| -> TestResult<Term> {
        ok(
            node,
            "FLOW.RETENTION_CLEANUP",
            &[("limit", int(100)), ("now_ms", int(9_007_199_254_740_991))],
        )
    };
    let all = |node: &QueueNode| {
        listed(
            node,
            &[
                ("state", text("all")),
                ("target_type", text("retained-type")),
                ("count", int(10)),
            ],
        )
    };
    ok(
        &node,
        "FLOW.SCHEDULE.DELETE",
        &[("id", text("first")), ("now_ms", int(4_006))],
    )?;
    let Term::Int(flows) = field(&cleanup(&node)?, "flows") else {
        return Err("a flow count expected".into());
    };
    assert!(flows >= 1);
    assert_eq!(all(&node)?, [text("second")]);
    ok(
        &node,
        "FLOW.SCHEDULE.DELETE",
        &[("id", text("second")), ("now_ms", int(4_007))],
    )?;
    cleanup(&node)?;
    assert_eq!(all(&node)?, Vec::<Term>::new());
    assert_eq!(schedule(&node, "first")?, Term::Nil);
    Ok(())
}

#[test]
fn a_cron_fires_on_matching_minutes_and_moves_to_the_next() -> TestResult {
    // Elixir: "cron schedule fires on matching UTC minutes and reschedules
    // to next match".
    let node = quiet("audit-schedule-cron-minutes")?;
    let created = create(
        &node,
        "minutes",
        &[
            ("kind", text("cron")),
            ("cron", text("*/5 * * * *")),
            ("start_at_ms", int(0)),
            ("now_ms", int(0)),
            (
                "target",
                map(&[
                    ("id_prefix", text("minutes")),
                    ("type", text("minutes")),
                    ("partition_key", text("minutes-partition")),
                    ("payload", text("cron")),
                ]),
            ),
        ],
    )?;
    assert_eq!(
        (field(&created, "kind"), field(&created, "next_run_at_ms")),
        (text("cron"), int(0))
    );
    let read = |id: &str| {
        get(
            &node,
            id,
            &[
                ("partition_key", text("minutes-partition")),
                ("payload", Term::Bool(true)),
            ],
        )
    };
    assert_eq!(field(&fire_due(&node, 0)?, "fired"), int(1));
    assert_eq!(field(&read("minutes:0:1")?, "payload"), text("cron"));
    let after = schedule(&node, "minutes")?;
    assert_eq!(
        [
            field(&after, "state"),
            field(&after, "fire_count"),
            field(&after, "next_run_at_ms"),
        ],
        [text("active"), int(1), int(300_000)]
    );
    assert_eq!(field(&fire_due(&node, 299_999)?, "claimed"), int(0));
    assert_eq!(field(&fire_due(&node, 300_000)?, "fired"), int(1));
    assert_eq!(field(&read("minutes:300000:2")?, "payload"), text("cron"));
    Ok(())
}
