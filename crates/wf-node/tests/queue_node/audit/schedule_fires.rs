//! `flow/schedule_test.exs`, how schedules fire: a one-shot creates its
//! target once with the target's own state and payload, an interval
//! reschedules without spending attempts, delays start from command time,
//! views keep the whole recurrence, `FIRE_DUE` drains every claim wave up
//! to its limit, and concurrent fires lease a schedule once. (Elixir's
//! resource-limit probe and hybrid-clock hook are its own.)

use wf_application::commands::flow::Term;
use wf_node::composition::QueueNode;

use super::schedule_api::quiet;
use super::{field, get, int, map, ok, run, text};
use crate::TestResult;

fn create(node: &QueueNode, id: &str, options: &[(&str, Term)]) -> TestResult<Term> {
    let mut entries = vec![("id", text(id))];
    entries.extend(options.iter().cloned());
    ok(node, "FLOW.SCHEDULE.CREATE", &entries)
}

fn fire_due(node: &QueueNode, now: i64, extra: &[(&str, Term)]) -> TestResult<Term> {
    let mut entries = vec![("now_ms", int(now)), ("worker", text("schedule-test"))];
    entries.extend(extra.iter().cloned());
    ok(node, "FLOW.SCHEDULE.FIRE_DUE", &entries)
}

fn schedule(node: &QueueNode, id: &str) -> TestResult<Term> {
    ok(node, "FLOW.SCHEDULE.GET", &[("id", text(id))])
}

fn counts(summary: &Term) -> (Term, Term, Term) {
    (
        field(summary, "claimed"),
        field(summary, "fired"),
        field(summary, "errors"),
    )
}

fn wall() -> TestResult<i64> {
    Ok(i64::try_from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_millis(),
    )?)
}

#[test]
fn a_one_shot_creates_its_target_once() -> TestResult {
    // Elixir: "one-shot schedule fires target flow once and completes
    // schedule".
    let node = quiet("audit-schedule-once")?;
    let payload = map(&[("source", text("schedule"))]);
    let created = create(
        &node,
        "once",
        &[
            ("kind", text("one_shot")),
            ("at_ms", int(1_100)),
            ("now_ms", int(1_000)),
            (
                "target",
                map(&[
                    ("id", text("once-target")),
                    ("type", text("once-type")),
                    ("state", text("queued")),
                    ("partition_key", text("once-partition")),
                    ("payload", payload.clone()),
                ]),
            ),
        ],
    )?;
    assert_eq!(field(&created, "next_run_at_ms"), int(1_100));
    let empty = Term::List(Vec::new());
    assert_eq!(
        counts(&fire_due(&node, 1_099, &[])?),
        (int(0), int(0), empty.clone())
    );
    let partition = [("partition_key", text("once-partition"))];
    assert_eq!(get(&node, "once-target", &partition)?, Term::Nil);
    assert_eq!(
        counts(&fire_due(&node, 1_100, &[])?),
        (int(1), int(1), empty.clone())
    );
    let target = get(
        &node,
        "once-target",
        &[
            ("partition_key", text("once-partition")),
            ("payload", Term::Bool(true)),
        ],
    )?;
    assert_eq!(
        (
            field(&target, "type"),
            field(&target, "state"),
            field(&target, "payload")
        ),
        (text("once-type"), text("queued"), payload)
    );
    assert_eq!(field(&schedule(&node, "once")?, "state"), text("completed"));
    assert_eq!(
        counts(&fire_due(&node, 1_200, &[])?),
        (int(0), int(0), empty)
    );
    Ok(())
}

#[test]
fn an_interval_reschedules_without_spending_attempts() -> TestResult {
    // Elixir: "interval schedule reschedules without consuming retry
    // attempts".
    let node = quiet("audit-schedule-interval")?;
    let created = create(
        &node,
        "tick",
        &[
            ("kind", text("interval")),
            ("start_at_ms", int(2_000)),
            ("every_ms", int(500)),
            ("now_ms", int(1_900)),
            (
                "target",
                map(&[
                    ("id_prefix", text("tick")),
                    ("type", text("tick-type")),
                    ("partition_key", text("tick-partition")),
                    ("payload", text("tick")),
                ]),
            ),
        ],
    )?;
    assert_eq!(field(&created, "next_run_at_ms"), int(2_000));
    let read = |id: &str| {
        get(
            &node,
            id,
            &[
                ("partition_key", text("tick-partition")),
                ("payload", Term::Bool(true)),
            ],
        )
    };
    assert_eq!(field(&fire_due(&node, 2_000, &[])?, "fired"), int(1));
    assert_eq!(field(&read("tick:2000:1")?, "payload"), text("tick"));
    let first = schedule(&node, "tick")?;
    assert_eq!(
        [
            field(&first, "state"),
            field(&first, "fire_count"),
            field(&first, "next_run_at_ms"),
            field(&first, "attempts"),
        ],
        [text("active"), int(1), int(2_500), int(0)]
    );
    assert_eq!(field(&fire_due(&node, 2_499, &[])?, "fired"), int(0));
    assert_eq!(field(&fire_due(&node, 2_500, &[])?, "fired"), int(1));
    assert_eq!(field(&read("tick:2500:2")?, "payload"), text("tick"));
    let second = schedule(&node, "tick")?;
    assert_eq!(
        (
            field(&second, "fire_count"),
            field(&second, "next_run_at_ms")
        ),
        (int(2), int(3_000))
    );
    Ok(())
}

#[test]
fn a_delay_starts_from_the_command_time() -> TestResult {
    // Elixir: "delay schedule defaults are based on the cluster-adjusted
    // clock" (this node's command clock is its wall clock).
    let node = quiet("audit-schedule-delay")?;
    let before = wall()?;
    let created = create(
        &node,
        "delayed",
        &[
            ("kind", text("delay")),
            ("delay_ms", int(30_000)),
            (
                "target",
                map(&[("id", text("delayed-target")), ("type", text("delayed"))]),
            ),
        ],
    )?;
    let Term::Int(next) = field(&created, "next_run_at_ms") else {
        return Err(format!("a next run expected in {created:?}").into());
    };
    assert!(next >= before + 30_000, "{next} < {before} + 30000");
    assert!(next <= wall()? + 30_000);
    Ok(())
}

#[test]
fn views_keep_the_whole_recurrence() -> TestResult {
    // Elixir: "schedule views preserve the complete recurrence
    // configuration".
    let node = quiet("audit-schedule-views")?;
    let interval = create(
        &node,
        "view-interval",
        &[
            ("kind", text("interval")),
            ("every_ms", int(250)),
            ("start_at_ms", int(1_250)),
            ("now_ms", int(1_200)),
            ("overlap_policy", text("queue_after_previous")),
            ("overlap_retry_ms", int(75)),
            (
                "target",
                map(&[
                    ("id_prefix", text("view-interval:target")),
                    ("type", text("view-type")),
                ]),
            ),
        ],
    )?;
    let recurrence = |view: &Term| {
        [
            field(view, "created_at_ms"),
            field(view, "every_ms"),
            field(view, "cron"),
            field(view, "timezone"),
            field(view, "overlap_retry_ms"),
        ]
    };
    assert_eq!(
        recurrence(&interval),
        [int(1_200), int(250), Term::Nil, Term::Nil, int(75)]
    );
    assert_eq!(
        recurrence(&schedule(&node, "view-interval")?),
        recurrence(&interval)
    );
    let cron = create(
        &node,
        "view-cron",
        &[
            ("kind", text("cron")),
            ("cron", text("*/5 * * * *")),
            ("timezone", text("UTC")),
            ("now_ms", int(1_200)),
            (
                "target",
                map(&[
                    ("id_prefix", text("view-cron:target")),
                    ("type", text("view-type")),
                ]),
            ),
        ],
    )?;
    assert_eq!(
        recurrence(&cron),
        [
            int(1_200),
            Term::Nil,
            text("*/5 * * * *"),
            text("Etc/UTC"),
            Term::Nil
        ]
    );
    let Term::List(listed) = ok(
        &node,
        "FLOW.SCHEDULE.LIST",
        &[
            ("state", text("all")),
            ("target_type", text("view-type")),
            ("count", int(10)),
        ],
    )?
    else {
        return Err("list is not a list".into());
    };
    assert_eq!(listed.len(), 2);
    for view in &listed {
        assert_eq!(field(view, "created_at_ms"), int(1_200));
        let expected = if field(view, "id") == text("view-interval") {
            recurrence(&interval)
        } else {
            recurrence(&cron)
        };
        assert_eq!(recurrence(view), expected);
    }
    Ok(())
}

fn due_one_shots(node: &QueueNode, prefix: &str, count: usize, now: i64) -> TestResult {
    for index in 1..=count {
        create(
            node,
            &format!("{prefix}-{index}"),
            &[
                ("kind", text("one_shot")),
                ("at_ms", int(now)),
                ("now_ms", int(now)),
                (
                    "target",
                    map(&[
                        ("id", text(&format!("{prefix}-target-{index}"))),
                        ("type", text(prefix)),
                    ]),
                ),
            ],
        )?;
    }
    Ok(())
}

#[test]
fn fire_due_drains_every_claim_wave_up_to_its_limit() -> TestResult {
    // Elixir: "fire_due drains multiple bounded claim waves without losing
    // schedules" and "fire_due returns after an exact full dispatch wave
    // below the requested limit".
    let node = quiet("audit-schedule-waves")?;
    due_one_shots(&node, "wave", 18, 1_500)?;
    let summary = fire_due(&node, 1_500, &[("limit", int(18))])?;
    assert_eq!(counts(&summary), (int(18), int(18), Term::List(Vec::new())));
    assert_eq!(field(&summary, "skipped"), int(0));
    for index in 1..=18 {
        let done = schedule(&node, &format!("wave-{index}"))?;
        assert_eq!(field(&done, "state"), text("completed"));
        let target = get(&node, &format!("wave-target-{index}"), &[])?;
        assert_eq!(field(&target, "type"), text("wave"));
    }
    due_one_shots(&node, "full", 16, 1_750)?;
    let summary = fire_due(&node, 1_750, &[("limit", int(17))])?;
    assert_eq!(counts(&summary), (int(16), int(16), Term::List(Vec::new())));
    Ok(())
}

#[test]
fn concurrent_fire_due_leases_a_schedule_once() -> TestResult {
    // Elixir: "concurrent schedule firing leases a due schedule once".
    let node = quiet("audit-schedule-concurrent")?;
    create(
        &node,
        "contended",
        &[
            ("kind", text("one_shot")),
            ("at_ms", int(2_500)),
            ("now_ms", int(2_500)),
            (
                "target",
                map(&[
                    ("id", text("contended-target")),
                    ("type", text("contended")),
                    ("payload", text("fire-once")),
                ]),
            ),
        ],
    )?;
    let summaries = std::thread::scope(|scope| -> TestResult<Vec<Term>> {
        let handles: Vec<_> = (1..=8)
            .map(|worker| {
                let node = &node;
                scope.spawn(move || {
                    run(
                        node,
                        "FLOW.SCHEDULE.FIRE_DUE",
                        &[
                            ("now_ms", int(2_500)),
                            ("worker", text(&format!("schedule-concurrent-{worker}"))),
                        ],
                    )
                    .map_err(|error| error.to_string())
                })
            })
            .collect();
        let mut summaries = Vec::new();
        for handle in handles {
            summaries.push(handle.join().map_err(|_| "a firing thread panicked")???);
        }
        Ok(summaries)
    })?;
    let total = |key: &str| {
        summaries
            .iter()
            .map(|summary| match field(summary, key) {
                Term::Int(count) => count,
                _ => 0,
            })
            .sum::<i64>()
    };
    assert!(
        summaries
            .iter()
            .all(|summary| field(summary, "errors") == Term::List(Vec::new()))
    );
    assert_eq!((total("fired"), total("claimed")), (1, 1));
    let target = get(&node, "contended-target", &[("payload", Term::Bool(true))])?;
    assert_eq!(field(&target, "payload"), text("fire-once"));
    assert_eq!(
        field(&schedule(&node, "contended")?, "state"),
        text("completed")
    );
    Ok(())
}

#[test]
fn concurrent_manual_fires_number_their_targets_apart() -> TestResult {
    // Elixir: "concurrent manual fires fence before creating distinct
    // targets": however the two fires interleave, each target the schedule
    // counts was created once, under its own fire number.
    let node = quiet("audit-schedule-manual-race")?;
    let now = wall()?;
    create(
        &node,
        "manual-race",
        &[
            ("kind", text("interval")),
            ("start_at_ms", int(now + 60_000)),
            ("every_ms", int(1_000)),
            ("now_ms", int(now)),
            (
                "target",
                map(&[("id_prefix", text("race")), ("type", text("race"))]),
            ),
        ],
    )?;
    let fire_times = [now + 1, now + 2];
    let results = std::thread::scope(|scope| -> TestResult<Vec<bool>> {
        let handles: Vec<_> = fire_times
            .iter()
            .map(|at| {
                let node = &node;
                let at = *at;
                scope.spawn(move || {
                    run(
                        node,
                        "FLOW.SCHEDULE.FIRE",
                        &[
                            ("id", text("manual-race")),
                            ("now_ms", int(at)),
                            ("fire_at_ms", int(at)),
                        ],
                    )
                    .map(|reply| reply.is_ok())
                    .map_err(|error| error.to_string())
                })
            })
            .collect();
        let mut results = Vec::new();
        for handle in handles {
            results.push(handle.join().map_err(|_| "a firing thread panicked")??);
        }
        Ok(results)
    })?;
    let fired = results.iter().filter(|fired| **fired).count();
    assert!(fired >= 1, "no manual fire succeeded");
    let mut created = Vec::new();
    for at in fire_times {
        for number in 1..=2 {
            let id = format!("race:{at}:{number}");
            if get(&node, &id, &[])? != Term::Nil {
                created.push(number);
            }
        }
    }
    created.sort_unstable();
    let expected: Vec<i64> = (1..=i64::try_from(fired)?).collect();
    assert_eq!(created, expected);
    assert_eq!(
        field(&schedule(&node, "manual-race")?, "fire_count"),
        int(i64::try_from(fired)?)
    );
    Ok(())
}

#[test]
fn a_manual_fire_without_a_time_uses_the_command_time() -> TestResult {
    // Elixir: "manual fire without a timestamp uses the current command
    // time".
    let node = quiet("audit-schedule-manual-now")?;
    create(
        &node,
        "manual-now",
        &[
            ("kind", text("one_shot")),
            ("at_ms", int(0)),
            ("now_ms", int(0)),
            (
                "target",
                map(&[("id_prefix", text("manual-now")), ("type", text("now"))]),
            ),
        ],
    )?;
    let before = wall()?;
    let fired = ok(&node, "FLOW.SCHEDULE.FIRE", &[("id", text("manual-now"))])?;
    assert_eq!(field(&fired, "fired"), int(1));
    let Term::Bin(target_id) = field(&fired, "target_id") else {
        return Err(format!("a target id expected in {fired:?}").into());
    };
    let target_id = String::from_utf8(target_id)?;
    let due: i64 = target_id
        .strip_prefix("manual-now:")
        .and_then(|rest| rest.split(':').next())
        .ok_or("a prefixed target id expected")?
        .parse()?;
    assert!(due >= before && due <= wall()?, "{target_id}");
    Ok(())
}
