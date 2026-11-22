//! `flow/schedule_catchup_test.exs`: an overdue interval fires once and
//! coalesces what it missed (after downtime, a pause or a queued overlap),
//! keeps its cadence when less than a period late, counts only periods
//! within its end bound, and its counts stay within the exact-integer
//! contract. (`Catchup.next_interval/3` itself is `schedule::tests`.)

use wf_application::commands::flow::Term;
use wf_node::composition::QueueNode;

use super::schedule_api::quiet;
use super::{claimed, field, get, int, map, ok, refusal, run, text};
use crate::TestResult;

const MAX_EXACT: i64 = 9_007_199_254_740_991;

fn interval(
    node: &QueueNode,
    id: &str,
    (start, every, now): (i64, i64, i64),
    extra: &[(&str, Term)],
) -> TestResult<Result<Term, String>> {
    let mut entries = vec![
        ("id", text(id)),
        ("kind", text("interval")),
        ("every_ms", int(every)),
        ("start_at_ms", int(start)),
        ("now_ms", int(now)),
        (
            "target",
            map(&[("id_prefix", text(id)), ("type", text(id))]),
        ),
    ];
    entries.extend(extra.iter().cloned());
    Ok(run(node, "FLOW.SCHEDULE.CREATE", &entries)?.map_err(|error| error.to_string()))
}

fn fire_due(node: &QueueNode, now: i64, extra: &[(&str, Term)]) -> TestResult<Term> {
    let mut entries = vec![("now_ms", int(now)), ("worker", text("catchup-test"))];
    entries.extend(extra.iter().cloned());
    ok(node, "FLOW.SCHEDULE.FIRE_DUE", &entries)
}

/// The summary's claimed, fired, skipped and coalesced counts.
fn counts(summary: &Term) -> [Term; 4] {
    [
        field(summary, "claimed"),
        field(summary, "fired"),
        field(summary, "skipped"),
        field(summary, "coalesced"),
    ]
}

fn schedule(node: &QueueNode, id: &str) -> TestResult<Term> {
    ok(node, "FLOW.SCHEDULE.GET", &[("id", text(id))])
}

#[test]
fn only_intervals_take_the_fire_once_catch_up_policy() -> TestResult {
    // Elixir: "interval schedules expose the bounded catch-up policy" and
    // "catchup_policy accepts only fire_once on interval schedules".
    let node = quiet("audit-catchup-policy")?;
    let created = interval(&node, "policy", (10_000, 1_000, 10_000), &[])??;
    assert_eq!(
        [
            field(&created, "catchup_policy"),
            field(&created, "coalesced_count"),
            field(&created, "last_catchup_at_ms"),
            field(&created, "last_coalesced_count"),
        ],
        [text("fire_once"), int(0), Term::Nil, int(0)]
    );
    let explicit = interval(
        &node,
        "explicit",
        (10_000, 1_000, 10_000),
        &[("catchup_policy", text("fire_once"))],
    )??;
    assert_eq!(field(&explicit, "catchup_policy"), text("fire_once"));
    assert_eq!(
        interval(
            &node,
            "buffered",
            (10_000, 1_000, 10_000),
            &[("catchup_policy", text("buffer_one"))]
        )?,
        Err("ERR flow schedule catchup_policy must be :fire_once".to_owned())
    );
    let cron = run(
        &node,
        "FLOW.SCHEDULE.CREATE",
        &[
            ("id", text("cron-catchup")),
            ("kind", text("cron")),
            ("cron", text("*/5 * * * *")),
            ("catchup_policy", text("fire_once")),
            (
                "target",
                map(&[("id_prefix", text("cron-catchup")), ("type", text("c"))]),
            ),
        ],
    )?;
    assert_eq!(
        refusal(cron)?,
        "ERR flow schedule catchup_policy is only supported for interval schedules"
    );
    Ok(())
}

#[test]
fn an_overdue_interval_fires_once_for_every_missed_period() -> TestResult {
    // Elixir: "an overdue interval coalesces every missed occurrence into one
    // fire" and "an interval less than one period late keeps its original
    // cadence".
    let node = quiet("audit-catchup-overdue")?;
    let (due, every) = (20_000, 1_000);
    let recovery = due + 1_000_000 * every + 500;
    interval(&node, "overdue", (due, every, due - 1), &[])??;
    assert_eq!(
        counts(&fire_due(&node, recovery, &[])?),
        [int(1), int(1), int(0), int(1_000_000)]
    );
    assert_ne!(get(&node, &format!("overdue:{due}:1"), &[])?, Term::Nil);
    let caught = schedule(&node, "overdue")?;
    assert_eq!(
        [
            field(&caught, "fire_count"),
            field(&caught, "coalesced_count"),
            field(&caught, "last_coalesced_count"),
            field(&caught, "last_catchup_at_ms"),
            field(&caught, "next_run_at_ms"),
        ],
        [
            int(1),
            int(1_000_000),
            int(1_000_000),
            int(recovery),
            int(recovery + every)
        ]
    );
    for now in [recovery, recovery + every - 1] {
        assert_eq!(field(&fire_due(&node, now, &[])?, "claimed"), int(0));
    }
    assert_eq!(
        counts(&fire_due(&node, recovery + every, &[])?),
        [int(1), int(1), int(0), int(0)]
    );
    let second = format!("overdue:{}:2", recovery + every);
    assert_ne!(get(&node, &second, &[])?, Term::Nil);
    interval(&node, "on-time", (30_000, every, 29_999), &[])??;
    assert_eq!(
        counts(&fire_due(&node, 30_000 + every - 1, &[])?),
        [int(1), int(1), int(0), int(0)]
    );
    let kept = schedule(&node, "on-time")?;
    assert_eq!(
        (
            field(&kept, "next_run_at_ms"),
            field(&kept, "coalesced_count")
        ),
        (int(30_000 + every), int(0))
    );
    Ok(())
}

#[test]
fn catch_up_counts_only_periods_within_the_end_bound() -> TestResult {
    // Elixir: "catch-up counts only occurrences within an interval schedule
    // end bound", "max_fires keeps precedence when the end bound is reached
    // by the same fire" and "an overlap skip completes when catch-up passes
    // the interval end bound".
    let node = quiet("audit-catchup-end")?;
    interval(
        &node,
        "bounded",
        (40_000, 100, 39_999),
        &[("end_at_ms", int(40_300))],
    )??;
    assert_eq!(
        counts(&fire_due(&node, 50_000, &[])?),
        [int(1), int(1), int(0), int(3)]
    );
    let ended = schedule(&node, "bounded")?;
    assert_eq!(
        [
            field(&ended, "state"),
            field(&ended, "end_reason"),
            field(&ended, "fire_count"),
            field(&ended, "coalesced_count"),
            field(&ended, "next_run_at_ms"),
        ],
        [
            text("completed"),
            text("end_at_ms"),
            int(1),
            int(3),
            Term::Nil
        ]
    );
    interval(
        &node,
        "both",
        (65_000, 100, 64_999),
        &[("end_at_ms", int(65_000)), ("max_fires", int(1))],
    )??;
    assert_eq!(field(&fire_due(&node, 65_000, &[])?, "fired"), int(1));
    assert_eq!(
        field(&schedule(&node, "both")?, "end_reason"),
        text("max_fires")
    );
    interval(
        &node,
        "skip-end",
        (60_000, 100, 60_000),
        &[("end_at_ms", int(60_500)), ("overlap_policy", text("skip"))],
    )??;
    fire_due(&node, 60_000, &[])?;
    assert_eq!(
        counts(&fire_due(&node, 61_000, &[])?),
        [int(1), int(0), int(1), int(4)]
    );
    let skipped = schedule(&node, "skip-end")?;
    assert_eq!(
        [
            field(&skipped, "state"),
            field(&skipped, "end_reason"),
            field(&skipped, "skipped_count"),
            field(&skipped, "coalesced_count"),
            field(&skipped, "next_run_at_ms"),
        ],
        [
            text("completed"),
            text("end_at_ms"),
            int(1),
            int(4),
            Term::Nil
        ]
    );
    Ok(())
}

#[test]
fn catch_up_runs_alongside_overlap_skips() -> TestResult {
    // Elixir: "catch-up remains independent from overlap skipping" and "an
    // overlap skip completes safely at the exact timestamp limit".
    let node = quiet("audit-catchup-skip")?;
    interval(
        &node,
        "skipper",
        (50_000, 100, 50_000),
        &[("overlap_policy", text("skip"))],
    )??;
    fire_due(&node, 50_000, &[])?;
    assert_eq!(
        counts(&fire_due(&node, 50_500, &[])?),
        [int(1), int(0), int(1), int(4)]
    );
    let after = schedule(&node, "skipper")?;
    assert_eq!(
        [
            field(&after, "fire_count"),
            field(&after, "skipped_count"),
            field(&after, "coalesced_count"),
            field(&after, "next_run_at_ms"),
        ],
        [int(1), int(1), int(4), int(50_600)]
    );
    let due = MAX_EXACT - 10;
    interval(
        &node,
        "limit-skip",
        (due, 6, due),
        &[("overlap_policy", text("skip"))],
    )??;
    let short = [("lease_ms", int(1))];
    assert_eq!(field(&fire_due(&node, due, &short)?, "fired"), int(1));
    let held = fire_due(&node, due + 6, &short)?;
    assert_eq!(
        (field(&held, "fired"), field(&held, "skipped")),
        (int(0), int(1))
    );
    let ended = schedule(&node, "limit-skip")?;
    assert_eq!(
        [
            field(&ended, "state"),
            field(&ended, "end_reason"),
            field(&ended, "fire_count"),
            field(&ended, "skipped_count"),
            field(&ended, "next_run_at_ms"),
        ],
        [
            text("completed"),
            text("timestamp_limit"),
            int(1),
            int(1),
            Term::Nil
        ]
    );
    Ok(())
}

#[test]
fn a_queued_or_paused_interval_catches_up_in_one_fire() -> TestResult {
    // Elixir: "a queued overlap fires once and coalesces intervals missed
    // while waiting" and "resuming an overdue interval coalesces the paused
    // periods into one fire".
    let node = quiet("audit-catchup-queued")?;
    let (due, every) = (55_000, 100);
    let recovery = due + 10 * every;
    interval(
        &node,
        "queued",
        (due, every, due),
        &[
            ("overlap_policy", text("queue_after_previous")),
            ("overlap_retry_ms", int(every)),
        ],
    )??;
    fire_due(&node, due, &[])?;
    assert_eq!(
        counts(&fire_due(&node, due + every, &[])?),
        [int(1), int(0), int(1), int(0)]
    );
    let records = claimed(
        &node,
        &[
            ("type", text("queued")),
            ("worker", text("catchup-target-worker")),
            ("limit", int(1)),
            ("now_ms", int(recovery)),
        ],
    )?;
    let job = records.first().ok_or("the first target was not claimed")?;
    assert_eq!(field(job, "id"), text(&format!("queued:{due}:1")));
    ok(
        &node,
        "FLOW.COMPLETE",
        &[
            ("id", field(job, "id")),
            ("lease_token", field(job, "lease_token")),
            ("fencing_token", field(job, "fencing_token")),
            ("now_ms", int(recovery)),
        ],
    )?;
    assert_eq!(
        counts(&fire_due(&node, recovery, &[])?),
        [int(1), int(1), int(0), int(9)]
    );
    let second = format!("queued:{}:2", due + every);
    assert_ne!(get(&node, &second, &[])?, Term::Nil);
    let fired = schedule(&node, "queued")?;
    assert_eq!(
        [
            field(&fired, "fire_count"),
            field(&fired, "coalesced_count"),
            field(&fired, "overlap_queued_due_at_ms"),
            field(&fired, "next_run_at_ms"),
        ],
        [int(2), int(9), Term::Nil, int(recovery + every)]
    );
    // A node of its own: the queued schedule above is due again.
    let node = quiet("audit-catchup-paused")?;
    let (due, recovery) = (57_000, 57_000 + 10 * every);
    interval(&node, "paused", (due, every, due - 1), &[])??;
    let paused = ok(
        &node,
        "FLOW.SCHEDULE.PAUSE",
        &[("id", text("paused")), ("now_ms", int(due))],
    )?;
    assert_eq!(field(&paused, "state"), text("paused"));
    assert_eq!(field(&fire_due(&node, recovery, &[])?, "claimed"), int(0));
    let resumed = ok(
        &node,
        "FLOW.SCHEDULE.RESUME",
        &[("id", text("paused")), ("now_ms", int(recovery))],
    )?;
    assert_eq!(
        (field(&resumed, "state"), field(&resumed, "next_run_at_ms")),
        (text("active"), int(due))
    );
    assert_eq!(
        counts(&fire_due(&node, recovery, &[])?),
        [int(1), int(1), int(0), int(10)]
    );
    let caught = schedule(&node, "paused")?;
    assert_eq!(
        [
            field(&caught, "fire_count"),
            field(&caught, "coalesced_count"),
            field(&caught, "next_run_at_ms"),
        ],
        [int(1), int(10), int(recovery + every)]
    );
    Ok(())
}

#[test]
fn a_batch_catch_up_total_stays_an_exact_integer() -> TestResult {
    // Elixir: "a batch catch-up total stays within the public exact-integer
    // contract".
    let node = quiet("audit-catchup-batch-total")?;
    for id in ["total-a", "total-b"] {
        interval(&node, id, (0, 1, 0), &[])??;
    }
    let summary = fire_due(
        &node,
        MAX_EXACT - 1,
        &[("lease_ms", int(1)), ("limit", int(2))],
    )?;
    assert_eq!(
        [
            field(&summary, "claimed"),
            field(&summary, "fired"),
            field(&summary, "coalesced"),
            field(&summary, "errors"),
        ],
        [int(2), int(2), int(MAX_EXACT), Term::List(Vec::new())]
    );
    Ok(())
}
