//! `flow/schedule_test.exs`, overlaps and their edges: skip and
//! `fail_schedule` details, queued occurrences that a finished target wakes,
//! end and exact-integer bounds reached while overlapping or firing, and
//! the lifecycle signals a schedule writes to its own history.

use wf_application::commands::flow::Term;
use wf_node::composition::QueueNode;

use super::schedule_api::quiet;
use super::{claimed, events, field, int, map, ok, refusal, run, text};
use crate::TestResult;

const MAX_EXACT: i64 = 9_007_199_254_740_991;

fn interval(
    node: &QueueNode,
    id: &str,
    (start, every): (i64, i64),
    extra: &[(&str, Term)],
) -> TestResult<Term> {
    let mut entries = vec![
        ("id", text(id)),
        ("kind", text("interval")),
        ("every_ms", int(every)),
        ("start_at_ms", int(start)),
        ("now_ms", int(start)),
        (
            "target",
            map(&[("id_prefix", text(id)), ("type", text(id))]),
        ),
    ];
    entries.extend(extra.iter().cloned());
    ok(node, "FLOW.SCHEDULE.CREATE", &entries)
}

fn fire_due(node: &QueueNode, now: i64, lease_ms: i64) -> TestResult<Term> {
    ok(
        node,
        "FLOW.SCHEDULE.FIRE_DUE",
        &[
            ("now_ms", int(now)),
            ("worker", text("schedule-test")),
            ("lease_ms", int(lease_ms)),
        ],
    )
}

fn fire(node: &QueueNode, id: &str, at: i64) -> TestResult<Result<Term, String>> {
    Ok(run(
        node,
        "FLOW.SCHEDULE.FIRE",
        &[
            ("id", text(id)),
            ("fire_at_ms", int(at)),
            ("now_ms", int(at)),
        ],
    )?
    .map_err(|error| error.to_string()))
}

fn schedule(node: &QueueNode, id: &str) -> TestResult<Term> {
    ok(node, "FLOW.SCHEDULE.GET", &[("id", text(id))])
}

fn ended(view: &Term) -> [Term; 3] {
    [
        field(view, "state"),
        field(view, "end_reason"),
        field(view, "next_run_at_ms"),
    ]
}

fn wall() -> TestResult<i64> {
    Ok(i64::try_from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_millis(),
    )?)
}

#[test]
fn exact_integer_bounds_end_a_schedule_cleanly() -> TestResult {
    // Elixir: "recurring schedule completes when its next timestamp exceeds
    // the exact limit", "queued overlap terminates cleanly when its retry
    // timestamp would overflow", "manual queued overlap terminates cleanly
    // when its retry timestamp would overflow" and "manual overlap skip
    // terminates cleanly at the exact timestamp limit".
    let node = quiet("audit-schedule-exact-limit")?;
    let limit = [text("completed"), text("timestamp_limit"), Term::Nil];
    interval(&node, "edge", (MAX_EXACT - 1, 2), &[])?;
    assert_eq!(field(&fire_due(&node, MAX_EXACT - 1, 1)?, "fired"), int(1));
    let done = schedule(&node, "edge")?;
    assert_eq!(ended(&done), limit);
    assert_eq!(field(&done, "fire_count"), int(1));
    let queued = [
        ("overlap_policy", text("queue_after_previous")),
        ("overlap_retry_ms", int(3)),
    ];
    interval(&node, "queued-due", (MAX_EXACT - 3, 2), &queued)?;
    assert_eq!(field(&fire_due(&node, MAX_EXACT - 3, 1)?, "fired"), int(1));
    let held = fire_due(&node, MAX_EXACT - 1, 1)?;
    assert_eq!(
        (field(&held, "fired"), field(&held, "skipped")),
        (int(0), int(1))
    );
    assert_eq!(ended(&schedule(&node, "queued-due")?), limit);
    for (id, policy) in [
        ("queued-manual", queued.to_vec()),
        ("skip-manual", vec![("overlap_policy", text("skip"))]),
    ] {
        interval(&node, id, (MAX_EXACT - 3, 2), &policy)?;
        fire_due(&node, MAX_EXACT - 3, 1)?;
        let skipped = fire(&node, id, MAX_EXACT - 1)??;
        assert_eq!(
            (field(&skipped, "fired"), field(&skipped, "skipped")),
            (int(0), int(1)),
            "{id}"
        );
        assert_eq!(ended(&field(&skipped, "schedule")), limit, "{id}");
    }
    let skipped = schedule(&node, "skip-manual")?;
    assert_eq!(field(&skipped, "skipped_count"), int(1));
    Ok(())
}

#[test]
fn a_manual_skip_past_the_end_bound_completes() -> TestResult {
    // Elixir: "manual overlap skip completes when the next occurrence passes
    // the end bound" and "manual schedule fire targets one schedule
    // immediately".
    let node = quiet("audit-schedule-skip-end")?;
    interval(
        &node,
        "bounded",
        (4_400, 100),
        &[("end_at_ms", int(4_500)), ("overlap_policy", text("skip"))],
    )?;
    fire_due(&node, 4_400, 30_000)?;
    let skipped = fire(&node, "bounded", 4_500)??;
    let view = field(&skipped, "schedule");
    assert_eq!(
        ended(&view),
        [text("completed"), text("end_at_ms"), Term::Nil]
    );
    assert_eq!(field(&view, "skipped_count"), int(1));
    ok(
        &node,
        "FLOW.SCHEDULE.CREATE",
        &[
            ("id", text("manual")),
            ("kind", text("one_shot")),
            ("at_ms", int(64_030)),
            ("now_ms", int(4_030)),
            (
                "target",
                map(&[
                    ("id", text("manual-target")),
                    ("type", text("manual")),
                    ("payload", text("manual")),
                ]),
            ),
        ],
    )?;
    let fired = ok(
        &node,
        "FLOW.SCHEDULE.FIRE",
        &[("id", text("manual")), ("now_ms", int(4_031))],
    )?;
    assert_eq!(
        (
            field(&fired, "target_id"),
            field(&field(&fired, "schedule"), "state")
        ),
        (text("manual-target"), text("completed"))
    );
    let target = super::get(&node, "manual-target", &[("payload", Term::Bool(true))])?;
    assert_eq!(field(&target, "payload"), text("manual"));
    assert_eq!(field(&schedule(&node, "manual")?, "fire_count"), int(1));
    Ok(())
}

#[test]
fn fail_schedule_keeps_the_overlap_that_failed_it() -> TestResult {
    // Elixir: "overlap policy fail_schedule marks recurring schedule failed"
    // and "manual fail_schedule overlap persists the same terminal details".
    let node = quiet("audit-schedule-fail-details")?;
    let failing = [("overlap_policy", text("fail_schedule"))];
    interval(&node, "due-fail", (4_500, 100), &failing)?;
    fire_due(&node, 4_500, 30_000)?;
    let summary = fire_due(&node, 4_600, 30_000)?;
    let Term::List(errors) = field(&summary, "errors") else {
        return Err("errors expected".into());
    };
    let reason = match errors.first() {
        Some(Term::List(pair)) => pair.get(1).cloned().unwrap_or(Term::Nil),
        _ => Term::Nil,
    };
    interval(&node, "manual-fail", (4_700, 100), &failing)?;
    fire_due(&node, 4_700, 30_000)?;
    let manual = text(&fire(&node, "manual-fail", 4_800)?.err().unwrap_or_default());
    for (id, reason, first) in [
        ("due-fail", reason, "due-fail:4500:1"),
        ("manual-fail", manual, "manual-fail:4700:1"),
    ] {
        let Term::Bin(bytes) = &reason else {
            return Err(format!("{id}: a reason expected").into());
        };
        assert!(String::from_utf8_lossy(bytes).contains("previous target still active"));
        let failed = schedule(&node, id)?;
        assert_eq!(
            ended(&failed),
            [text("failed"), text("overlap_failed"), Term::Nil],
            "{id}"
        );
        assert_eq!(field(&failed, "last_overlap_target_id"), text(first));
        assert_eq!(field(&failed, "last_overlap_reason"), reason);
    }
    Ok(())
}

/// The signal names in schedule `id`'s own history.
fn signals(node: &QueueNode, id: &str) -> TestResult<Vec<Term>> {
    let partition = text(&super::schedule_namespace::internal_partition(id)?);
    let internal = format!("__ferricstore_schedule__:{id}");
    Ok(events(node, &internal, &[("partition_key", partition)])?
        .iter()
        .map(|event| field(event, "signal"))
        .filter(|signal| *signal != Term::Nil)
        .collect())
}

#[test]
fn a_schedule_records_its_lifecycle_as_signals() -> TestResult {
    // Elixir: "schedule lifecycle writes explicit signal history events" and
    // "schedule delete cancels future schedule", with the overlap signals.
    let node = quiet("audit-schedule-signals")?;
    ok(
        &node,
        "FLOW.SCHEDULE.CREATE",
        &[
            ("id", text("signals")),
            ("kind", text("one_shot")),
            ("at_ms", int(4_040)),
            ("now_ms", int(4_040)),
            (
                "target",
                map(&[("id", text("signals-target")), ("type", text("signals"))]),
            ),
        ],
    )?;
    fire_due(&node, 4_040, 30_000)?;
    let fired = signals(&node, "signals")?;
    assert!(fired.contains(&text("schedule_created")), "{fired:?}");
    assert!(fired.contains(&text("schedule_fired")), "{fired:?}");
    ok(
        &node,
        "FLOW.SCHEDULE.CREATE",
        &[
            ("id", text("deleted")),
            ("kind", text("one_shot")),
            ("at_ms", int(5_000)),
            ("now_ms", int(4_000)),
            (
                "target",
                map(&[("id", text("deleted-target")), ("type", text("deleted"))]),
            ),
        ],
    )?;
    ok(
        &node,
        "FLOW.SCHEDULE.DELETE",
        &[("id", text("deleted")), ("now_ms", int(4_001))],
    )?;
    let deleted = schedule(&node, "deleted")?;
    assert_eq!(
        (field(&deleted, "state"), field(&deleted, "next_run_at_ms")),
        (text("cancelled"), Term::Nil)
    );
    assert!(signals(&node, "deleted")?.contains(&text("schedule_deleted")));
    assert_eq!(field(&fire_due(&node, 5_000, 30_000)?, "claimed"), int(0));
    interval(
        &node,
        "skips",
        (6_000, 100),
        &[("overlap_policy", text("skip"))],
    )?;
    fire_due(&node, 6_000, 30_000)?;
    fire_due(&node, 6_100, 30_000)?;
    assert!(signals(&node, "skips")?.contains(&text("schedule_skipped_overlap")));
    interval(
        &node,
        "fails",
        (7_000, 100),
        &[("overlap_policy", text("fail_schedule"))],
    )?;
    fire_due(&node, 7_000, 30_000)?;
    fire_due(&node, 7_100, 30_000)?;
    assert!(signals(&node, "fails")?.contains(&text("schedule_failed_overlap")));
    Ok(())
}

#[test]
fn a_finished_target_wakes_its_backed_off_queued_schedule() -> TestResult {
    // Elixir: "target completion wakes a backed-off queued schedule without
    // changing its occurrence": while the target runs the backoff holds;
    // once it finishes, the queued occurrence is due at once.
    let node = quiet("audit-schedule-wake")?;
    let due = wall()?;
    interval(
        &node,
        "woken",
        (due, 10_000),
        &[
            ("overlap_policy", text("queue_after_previous")),
            ("overlap_retry_ms", int(5_000)),
        ],
    )?;
    assert_eq!(field(&fire_due(&node, due, 60_000)?, "fired"), int(1));
    for at in [due + 10_000, due + 15_000] {
        assert_eq!(field(&fire_due(&node, at, 60_000)?, "skipped"), int(1));
    }
    let waiting = schedule(&node, "woken")?;
    assert_eq!(field(&waiting, "next_run_at_ms"), int(due + 25_000));
    let records = claimed(
        &node,
        &[
            ("type", text("woken")),
            ("worker", text("overlap-worker")),
            ("limit", int(1)),
            ("now_ms", int(due + 15_000)),
        ],
    )?;
    let job = records.first().ok_or("the first target was not claimed")?;
    assert_eq!(field(job, "id"), text(&format!("woken:{due}:1")));
    std::thread::sleep(std::time::Duration::from_millis(100));
    assert_eq!(
        field(&schedule(&node, "woken")?, "next_run_at_ms"),
        int(due + 25_000)
    );
    ok(
        &node,
        "FLOW.COMPLETE",
        &[
            ("id", field(job, "id")),
            ("lease_token", field(job, "lease_token")),
            ("fencing_token", field(job, "fencing_token")),
        ],
    )?;
    let mut woken = Term::Nil;
    for _ in 0..100 {
        woken = schedule(&node, "woken")?;
        if field(&woken, "next_run_at_ms") != int(due + 25_000) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    let Term::Int(next) = field(&woken, "next_run_at_ms") else {
        return Err(format!("a next run expected in {woken:?}").into());
    };
    assert!(next < due + 25_000 && next <= wall()?, "{next}");
    assert_eq!(field(&woken, "overlap_queued_due_at_ms"), int(due + 10_000));
    assert_eq!(field(&fire_due(&node, next, 60_000)?, "fired"), int(1));
    let fired = schedule(&node, "woken")?;
    assert_eq!(
        (
            field(&fired, "fire_count"),
            field(&fired, "overlap_queued_due_at_ms")
        ),
        (int(2), Term::Nil)
    );
    assert_eq!(
        refusal(run(&node, "FLOW.SCHEDULE.GET", &[("id", int(1))])?)?,
        "ERR native field id must be binary"
    );
    Ok(())
}
