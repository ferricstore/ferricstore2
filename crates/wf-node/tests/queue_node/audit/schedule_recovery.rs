//! `flow/schedule_recovery_test.exs`, `flow/scheduler_test.exs` and
//! `flow/schedule_retention_members_test.exs`, through the node: a
//! schedule, its due time, its catch-up and its queued backoff survive a
//! restart, a lease a lost fire left is reclaimed, the background poll
//! fires a future schedule and wakes a queued one when its target
//! finishes, and a schedule's history stays bounded however often it
//! fires or overlaps. (Elixir's dispatcher waves, scheduler settings and
//! keydir members are its own.)

use std::path::Path;
use std::time::Duration;

use wf_application::commands::flow::Term;
use wf_node::composition::{QueueNode, QueueNodeConfig};

use super::{claimed, events, field, get, int, map, ok, text};
use crate::{TestResult, directory, open_with};

fn node(path: &Path, poll: Option<Duration>) -> TestResult<QueueNode> {
    open_with(
        path,
        QueueNodeConfig {
            shards: 4,
            schedule_poll: poll,
            ..QueueNodeConfig::default()
        },
    )
}

fn create(node: &QueueNode, id: &str, options: &[(&str, Term)]) -> TestResult<Term> {
    let mut entries = vec![("id", text(id))];
    entries.extend(options.iter().cloned());
    ok(node, "FLOW.SCHEDULE.CREATE", &entries)
}

fn interval(id: &str, (start, every, now): (i64, i64, i64)) -> Vec<(&'static str, Term)> {
    vec![
        ("kind", text("interval")),
        ("every_ms", int(every)),
        ("start_at_ms", int(start)),
        ("now_ms", int(now)),
        (
            "target",
            map(&[("id_prefix", text(id)), ("type", text(id))]),
        ),
    ]
}

fn fire_due(node: &QueueNode, now: i64, extra: &[(&str, Term)]) -> TestResult<Term> {
    let mut entries = vec![("now_ms", int(now)), ("worker", text("restart-scheduler"))];
    entries.extend(extra.iter().cloned());
    ok(node, "FLOW.SCHEDULE.FIRE_DUE", &entries)
}

fn schedule(node: &QueueNode, id: &str) -> TestResult<Term> {
    ok(node, "FLOW.SCHEDULE.GET", &[("id", text(id))])
}

fn wall() -> TestResult<i64> {
    Ok(i64::try_from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_millis(),
    )?)
}

#[test]
fn schedules_their_catch_up_and_backoff_survive_a_restart() -> TestResult {
    // Elixir: "schedule record and due index survive application restart",
    // "overdue interval catch-up remains bounded after application restart"
    // and "queued overlap keeps its retry deadline and backoff across
    // restart".
    let path = directory("audit-schedule-restart")?;
    {
        let node = node(&path, None)?;
        create(
            &node,
            "once",
            &[
                ("kind", text("one_shot")),
                ("at_ms", int(100_000)),
                ("now_ms", int(9_999)),
                (
                    "target",
                    map(&[
                        ("id", text("once-target")),
                        ("type", text("restart-type")),
                        ("payload", map(&[("restart", Term::Bool(true))])),
                    ]),
                ),
            ],
        )?;
        create(
            &node,
            "catchup",
            &interval("catchup", (50_000, 100, 49_999)),
        )?;
        let mut queued = interval("queued", (18_000, 100, 18_000));
        queued.extend([
            ("overlap_policy", text("queue_after_previous")),
            ("overlap_retry_ms", int(50)),
        ]);
        create(&node, "queued", &queued)?;
        fire_due(&node, 18_000, &[])?;
        for at in [18_100, 18_150] {
            assert_eq!(field(&fire_due(&node, at, &[])?, "skipped"), int(1));
        }
    }
    let node = node(&path, None)?;
    let once = schedule(&node, "once")?;
    assert_eq!(
        (field(&once, "state"), field(&once, "next_run_at_ms")),
        (text("active"), int(100_000))
    );
    let catchup = schedule(&node, "catchup")?;
    assert_eq!(
        (
            field(&catchup, "catchup_policy"),
            field(&catchup, "next_run_at_ms")
        ),
        (text("fire_once"), int(50_000))
    );
    let queued = schedule(&node, "queued")?;
    assert_eq!(
        (
            field(&queued, "next_run_at_ms"),
            field(&queued, "overlap_queued_due_at_ms")
        ),
        (int(18_250), int(18_100))
    );
    assert_eq!(field(&fire_due(&node, 18_250, &[])?, "skipped"), int(1));
    let backed_off = schedule(&node, "queued")?;
    assert_eq!(
        [
            field(&backed_off, "next_run_at_ms"),
            field(&backed_off, "overlap_queued_due_at_ms"),
            field(&backed_off, "fire_count"),
        ],
        [int(18_450), int(18_100), int(1)]
    );
    fire_due(&node, 51_000, &[])?;
    let after = schedule(&node, "catchup")?;
    assert_eq!(
        (
            field(&after, "coalesced_count"),
            field(&after, "next_run_at_ms")
        ),
        (int(10), int(51_100))
    );
    fire_due(&node, 100_000, &[])?;
    let target = get(&node, "once-target", &[("payload", Term::Bool(true))])?;
    assert_eq!(
        field(&target, "payload"),
        map(&[("restart", Term::Bool(true))])
    );
    assert_eq!(field(&schedule(&node, "once")?, "state"), text("completed"));
    Ok(())
}

#[test]
fn a_lease_a_lost_fire_left_is_reclaimed_after_a_restart() -> TestResult {
    // Elixir: "claimed schedule is reclaimable after shard leader restart":
    // a fire that stopped part-way keeps its short lease, and the next
    // FIRE_DUE after it ran out claims the schedule again.
    let path = directory("audit-schedule-reclaim")?;
    {
        let node = node(&path, None)?;
        ok(
            &node,
            "FLOW.CREATE",
            &[
                ("id", text("taken-target")),
                ("type", text("someone-else")),
                ("now_ms", int(20_000)),
            ],
        )?;
        create(
            &node,
            "claimed",
            &[
                ("kind", text("one_shot")),
                ("at_ms", int(20_000)),
                ("now_ms", int(20_000)),
                (
                    "target",
                    map(&[("id", text("taken-target")), ("type", text("mine"))]),
                ),
            ],
        )?;
        let lost = fire_due(&node, 20_000, &[("lease_ms", int(1))])?;
        assert_eq!(field(&lost, "claimed"), int(1));
        assert_eq!(
            field(&schedule(&node, "claimed")?, "state"),
            text("running")
        );
    }
    let node = node(&path, None)?;
    let again = fire_due(&node, 20_100, &[])?;
    assert_eq!(field(&again, "claimed"), int(1));
    Ok(())
}

#[test]
fn the_poll_fires_a_future_schedule_and_a_woken_queue() -> TestResult {
    // Elixir: "a scheduler waiting on an empty store wakes for a
    // newly-created future schedule" and "a committed target completion
    // wakes a queued schedule before its fallback retry".
    let node = node(
        &directory("audit-schedule-poll-wake")?,
        Some(Duration::from_millis(20)),
    )?;
    let now = wall()?;
    create(
        &node,
        "future",
        &[
            ("kind", text("one_shot")),
            ("at_ms", int(now + 150)),
            ("now_ms", int(now)),
            (
                "target",
                map(&[("id", text("future-target")), ("type", text("future"))]),
            ),
        ],
    )?;
    let mut queued = interval("waking", (now - 100, 100, now - 100));
    queued.extend([
        ("overlap_policy", text("queue_after_previous")),
        ("overlap_retry_ms", int(10_000)),
    ]);
    create(&node, "waking", &queued)?;
    let held = |node: &QueueNode| -> TestResult<bool> {
        Ok(field(&schedule(node, "waking")?, "overlap_queued_due_at_ms") != Term::Nil)
    };
    let mut waited = 0;
    while !held(&node)? && waited < 250 {
        std::thread::sleep(Duration::from_millis(20));
        waited += 1;
    }
    assert!(held(&node)?, "the queued overlap was never recorded");
    let records = claimed(
        &node,
        &[
            ("type", text("waking")),
            ("worker", text("completion-worker")),
            ("limit", int(1)),
        ],
    )?;
    let job = records.first().ok_or("the first target was not claimed")?;
    ok(
        &node,
        "FLOW.COMPLETE",
        &[
            ("id", field(job, "id")),
            ("lease_token", field(job, "lease_token")),
            ("fencing_token", field(job, "fencing_token")),
        ],
    )?;
    let mut fired = Term::Nil;
    for _ in 0..250 {
        fired = schedule(&node, "waking")?;
        let target = get(&node, "future-target", &[])?;
        if field(&fired, "fire_count") == int(2) && target != Term::Nil {
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(field(&fired, "fire_count"), int(2));
    assert_eq!(
        field(&get(&node, "future-target", &[])?, "type"),
        text("future")
    );
    Ok(())
}

#[test]
fn a_schedule_s_history_stays_bounded() -> TestResult {
    // Elixir: "a recurring schedule keeps a bounded number of retention
    // cleanup members", "a new schedule's Flow keeps a short history" and
    // "a long-history schedule held back by overlap keeps a bounded number
    // of retention cleanup members": a schedule keeps at most 100 events.
    let node = node(&directory("audit-schedule-history-bound")?, None)?;
    create(&node, "busy", &interval("busy", (2_000, 500, 1_900)))?;
    let mut overlapping = interval("overlapping", (2_000, 500, 1_900));
    overlapping.push(("overlap_policy", text("queue_after_previous")));
    create(&node, "overlapping", &overlapping)?;
    for fire in 0..60 {
        fire_due(&node, 2_000 + fire * 60_000, &[])?;
    }
    let busy = schedule(&node, "busy")?;
    assert_eq!(field(&busy, "fire_count"), int(60));
    let held = schedule(&node, "overlapping")?;
    assert_eq!(field(&held, "fire_count"), int(1));
    for id in ["busy", "overlapping"] {
        let partition = super::schedule_namespace::internal_partition(id)?;
        let history = events(
            &node,
            &format!("__ferricstore_schedule__:{id}"),
            &[("partition_key", text(&partition)), ("count", int(1_000))],
        )?;
        assert!(
            !history.is_empty() && history.len() <= 100,
            "{id}: {} events",
            history.len()
        );
    }
    Ok(())
}
