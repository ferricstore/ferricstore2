//! `FLOW.SCHEDULE.*` (ADR-014 step 13) through the node, with the business
//! cases of Elixir's `schedule_test.exs`: one-shot, delay and interval
//! fires with catch-up and end conditions, manual fires, overlap policies,
//! pause, resume, delete, overwrite, target ownership, restart, and the
//! background poll.

use std::time::Duration;

use wf_application::commands::flow::{self, FlowService, Term};
use wf_application::commands::queue::{QueueService, ServiceError};
use wf_node::composition::{QueueNode, QueueNodeConfig};

use super::{TestResult, call, directory, open_with};

fn object(entries: Vec<(&str, Term)>) -> Term {
    Term::Map(
        entries
            .into_iter()
            .map(|(key, value)| (key.as_bytes().to_vec(), value))
            .collect(),
    )
}

fn text(value: &str) -> Term {
    Term::text(value)
}

fn node(path: &std::path::Path, poll: Option<Duration>) -> TestResult<QueueNode> {
    open_with(
        path,
        QueueNodeConfig {
            shards: 4,
            schedule_poll: poll,
            ..QueueNodeConfig::default()
        },
    )
}

fn run(
    node: &QueueNode,
    name: &str,
    entries: Vec<(&str, Term)>,
) -> TestResult<Result<Term, ServiceError>> {
    let command = flow::parse(name, &object(entries))?;
    call(|reply| node.execute(command, reply))
}

fn ok(node: &QueueNode, name: &str, entries: Vec<(&str, Term)>) -> TestResult<Term> {
    Ok(run(node, name, entries)??)
}

fn field(term: &Term, key: &str) -> Term {
    term.get(key.as_bytes()).cloned().unwrap_or(Term::Nil)
}

fn get(node: &QueueNode, id: &str) -> TestResult<Term> {
    ok(node, "FLOW.SCHEDULE.GET", vec![("id", text(id))])
}

fn fire_due(node: &QueueNode, now: i64) -> TestResult<Term> {
    ok(
        node,
        "FLOW.SCHEDULE.FIRE_DUE",
        vec![
            ("now_ms", Term::Int(now)),
            ("worker", text("schedule-test")),
        ],
    )
}

fn target(kind: &str, value: &str, flow_type: &str) -> Term {
    object(vec![
        (kind, text(value)),
        ("type", text(flow_type)),
        ("partition_key", text("tp")),
    ])
}

fn flow_get(node: &QueueNode, id: &str) -> TestResult<Term> {
    ok(
        node,
        "FLOW.GET",
        vec![("id", text(id)), ("partition_key", text("tp"))],
    )
}

/// Claims and completes one Flow of `flow_type` in `tp`.
fn finish_one(node: &QueueNode, flow_type: &str, now: i64) -> TestResult<String> {
    let parsed = flow::claim::parse(
        "FLOW.CLAIM_DUE",
        &object(vec![
            ("type", text(flow_type)),
            ("worker", text("w")),
            ("now_ms", Term::Int(now)),
            ("partition_key", text("tp")),
            ("limit", Term::Int(1)),
        ]),
    )?;
    let claimed = call(|reply| node.claim(parsed.request, reply))??;
    let job = claimed.first().ok_or("nothing claimed")?;
    ok(
        node,
        "FLOW.COMPLETE",
        vec![
            ("id", Term::Bin(job.id.clone())),
            ("partition_key", text("tp")),
            ("lease_token", Term::Bin(job.lease_token.clone())),
            ("fencing_token", Term::int(job.fence)),
            ("now_ms", Term::Int(now)),
        ],
    )?;
    Ok(String::from_utf8_lossy(&job.id).into_owned())
}

#[test]
fn schedules_fire_their_targets_until_they_end() -> TestResult {
    let path = directory("flow-schedule")?;
    {
        let node = node(&path, None)?;
        ok(
            &node,
            "FLOW.SCHEDULE.CREATE",
            vec![
                ("id", text("once")),
                ("target", target("id", "once-target", "ts")),
                ("at_ms", Term::Int(5_000)),
                ("now_ms", Term::Int(1_000)),
            ],
        )?;
        assert_eq!(field(&fire_due(&node, 4_000)?, "claimed"), Term::Int(0));
        let fired = fire_due(&node, 5_000)?;
        assert_eq!(field(&fired, "fired"), Term::Int(1));
        assert_eq!(field(&fired, "last_target_id"), text("once-target"));
        let done = get(&node, "once")?;
        assert_eq!(field(&done, "state"), text("completed"));
        assert_eq!(field(&done, "end_reason"), text("one_shot_fired"));
        assert_eq!(field(&done, "next_run_at_ms"), Term::Nil);
        let created = flow_get(&node, "once-target")?;
        assert_eq!(
            field(&created, "correlation_id"),
            text("__ferricstore_schedule__:once")
        );
        assert_eq!(
            field(&field(&created, "attributes"), "ferricstore.schedule.id.v1"),
            text("once")
        );
        // An interval catches up once over missed periods, then runs to max_fires.
        ok(
            &node,
            "FLOW.SCHEDULE.CREATE",
            vec![
                ("id", text("every")),
                ("target", target("id_prefix", "every", "ts")),
                ("every_ms", Term::Int(1_000)),
                ("start_at_ms", Term::Int(10_000)),
                ("now_ms", Term::Int(1_000)),
                ("max_fires", Term::Int(3)),
            ],
        )?;
        assert_eq!(field(&fire_due(&node, 13_500)?, "coalesced"), Term::Int(3));
        let caught_up = get(&node, "every")?;
        assert_eq!(field(&caught_up, "next_run_at_ms"), Term::Int(14_500));
        assert_eq!(field(&caught_up, "attempts"), Term::Int(0));
        assert_eq!(field(&caught_up, "last_target_id"), text("every:10000:1"));
    }
    // Schedules and their counts survive a restart.
    let node = node(&path, None)?;
    assert_eq!(field(&get(&node, "every")?, "fire_count"), Term::Int(1));
    let manual = ok(
        &node,
        "FLOW.SCHEDULE.FIRE",
        vec![("id", text("every")), ("now_ms", Term::Int(14_000))],
    )?;
    assert_eq!(field(&manual, "target_id"), text("every:14000:2"));
    assert_eq!(
        field(&field(&manual, "schedule"), "next_run_at_ms"),
        Term::Int(15_000)
    );
    ok(
        &node,
        "FLOW.SCHEDULE.FIRE",
        vec![("id", text("every")), ("now_ms", Term::Int(14_100))],
    )?;
    let ended = get(&node, "every")?;
    assert_eq!(field(&ended, "state"), text("completed"));
    assert_eq!(field(&ended, "end_reason"), text("max_fires"));
    assert_eq!(
        run(&node, "FLOW.SCHEDULE.FIRE", vec![("id", text("every"))])?,
        Err(ServiceError::rejected(
            "ERR flow schedule is not active".to_owned()
        ))
    );
    ends_before_end_at(&node)
}

/// A recurring schedule ends when its next fire would pass `end_at_ms`.
fn ends_before_end_at(node: &QueueNode) -> TestResult {
    ok(
        node,
        "FLOW.SCHEDULE.CREATE",
        vec![
            ("id", text("until")),
            ("target", target("id_prefix", "until", "ts")),
            ("every_ms", Term::Int(1_000)),
            ("start_at_ms", Term::Int(40_000)),
            ("end_at_ms", Term::Int(41_500)),
            ("now_ms", Term::Int(1_000)),
        ],
    )?;
    ok(
        node,
        "FLOW.SCHEDULE.FIRE",
        vec![("id", text("until")), ("now_ms", Term::Int(40_000))],
    )?;
    ok(
        node,
        "FLOW.SCHEDULE.FIRE",
        vec![("id", text("until")), ("now_ms", Term::Int(41_000))],
    )?;
    assert_eq!(field(&get(node, "until")?, "end_reason"), text("end_at_ms"));
    Ok(())
}

#[test]
fn overlap_policies_hold_back_a_schedule_while_its_target_runs() -> TestResult {
    let path = directory("flow-schedule-overlap")?;
    let node = node(&path, None)?;
    let create = |id: &str, policy: &str, start: i64, extra: Vec<(&str, Term)>| -> TestResult {
        let mut entries = vec![
            ("id", text(id)),
            ("target", target("id_prefix", id, id)),
            ("every_ms", Term::Int(100)),
            ("start_at_ms", Term::Int(start)),
            ("now_ms", Term::Int(start)),
            ("overlap_policy", text(policy)),
        ];
        entries.extend(extra);
        ok(&node, "FLOW.SCHEDULE.CREATE", entries)?;
        Ok(())
    };
    // queue_after_previous keeps the occurrence and retries it.
    create(
        "queued",
        "queue_after_previous",
        4_300,
        vec![("overlap_retry_ms", Term::Int(50))],
    )?;
    assert_eq!(field(&fire_due(&node, 4_300)?, "fired"), Term::Int(1));
    let held = fire_due(&node, 4_400)?;
    assert_eq!(
        (field(&held, "fired"), field(&held, "skipped")),
        (Term::Int(0), Term::Int(1))
    );
    let queued = get(&node, "queued")?;
    assert_eq!(field(&queued, "next_run_at_ms"), Term::Int(4_450));
    assert_eq!(field(&queued, "overlap_queued_due_at_ms"), Term::Int(4_400));
    assert_eq!(finish_one(&node, "queued", 4_400)?, "queued:4300:1");
    assert_eq!(field(&fire_due(&node, 4_450)?, "fired"), Term::Int(1));
    assert_eq!(
        field(&flow_get(&node, "queued:4400:2")?, "type"),
        text("queued")
    );
    // skip advances without a new target.
    create("skipping", "skip", 20_000, Vec::new())?;
    ok(
        &node,
        "FLOW.SCHEDULE.FIRE",
        vec![("id", text("skipping")), ("now_ms", Term::Int(20_000))],
    )?;
    let skipped = ok(
        &node,
        "FLOW.SCHEDULE.FIRE",
        vec![("id", text("skipping")), ("now_ms", Term::Int(20_100))],
    )?;
    assert_eq!(
        field(&skipped, "reason"),
        text("previous target still active: skipping:20000:1")
    );
    let after = get(&node, "skipping")?;
    assert_eq!(
        (
            field(&after, "skipped_count"),
            field(&after, "next_run_at_ms")
        ),
        (Term::Int(1), Term::Int(20_200))
    );
    // fail_schedule fails the schedule.
    create("failing", "fail_schedule", 30_000, Vec::new())?;
    ok(
        &node,
        "FLOW.SCHEDULE.FIRE",
        vec![("id", text("failing")), ("now_ms", Term::Int(30_000))],
    )?;
    assert_eq!(
        run(
            &node,
            "FLOW.SCHEDULE.FIRE",
            vec![("id", text("failing")), ("now_ms", Term::Int(30_100))]
        )?,
        Err(ServiceError::rejected(
            "previous target still active: failing:30000:1".to_owned()
        ))
    );
    let failed = get(&node, "failing")?;
    assert_eq!(
        (field(&failed, "state"), field(&failed, "end_reason")),
        (text("failed"), text("overlap_failed"))
    );
    Ok(())
}

#[test]
fn schedules_pause_resume_delete_and_overwrite() -> TestResult {
    let path = directory("flow-schedule-control")?;
    let node = node(&path, None)?;
    let one_shot = |id: &str,
                    target_id: &str,
                    at: i64,
                    extra: Vec<(&str, Term)>|
     -> TestResult<Result<Term, ServiceError>> {
        let mut entries = vec![
            ("id", text(id)),
            ("target", target("id", target_id, "tc")),
            ("at_ms", Term::Int(at)),
            ("now_ms", Term::Int(1_000)),
        ];
        entries.extend(extra);
        run(&node, "FLOW.SCHEDULE.CREATE", entries)
    };
    one_shot("p", "p-target", 2_000, Vec::new())??;
    ok(
        &node,
        "FLOW.SCHEDULE.PAUSE",
        vec![("id", text("p")), ("now_ms", Term::Int(1_100))],
    )?;
    assert_eq!(field(&fire_due(&node, 3_000)?, "claimed"), Term::Int(0));
    assert_eq!(
        run(
            &node,
            "FLOW.SCHEDULE.FIRE",
            vec![("id", text("p")), ("now_ms", Term::Int(3_000))]
        )?,
        Err(ServiceError::rejected(
            "ERR flow schedule is not active".to_owned()
        ))
    );
    assert_eq!(
        run(&node, "FLOW.SCHEDULE.PAUSE", vec![("id", text("p"))])?,
        Err(ServiceError::rejected(
            "ERR flow schedule is not active".to_owned()
        ))
    );
    ok(
        &node,
        "FLOW.SCHEDULE.RESUME",
        vec![("id", text("p")), ("now_ms", Term::Int(1_200))],
    )?;
    assert_eq!(field(&fire_due(&node, 3_000)?, "fired"), Term::Int(1));
    // A duplicate needs overwrite, which reactivates a completed one-shot.
    assert_eq!(
        one_shot("p", "p-second", 4_000, Vec::new())?,
        Err(ServiceError::rejected("ERR flow already exists".to_owned()))
    );
    let replaced = one_shot(
        "p",
        "p-second",
        4_000,
        vec![("overwrite", Term::Bool(true))],
    )??;
    assert_eq!(
        (
            field(&replaced, "state"),
            field(&replaced, "next_run_at_ms")
        ),
        (text("active"), Term::Int(4_000))
    );
    assert_eq!(
        field(&fire_due(&node, 4_000)?, "last_target_id"),
        text("p-second")
    );
    // Delete cancels a future schedule; a finished one is refused.
    one_shot("d", "d-target", 9_000, Vec::new())??;
    ok(&node, "FLOW.SCHEDULE.DELETE", vec![("id", text("d"))])?;
    assert_eq!(field(&get(&node, "d")?, "state"), text("cancelled"));
    assert_eq!(
        run(&node, "FLOW.SCHEDULE.DELETE", vec![("id", text("d"))])?,
        Err(ServiceError::rejected(
            "ERR flow is terminal; use FLOW.REWIND".to_owned()
        ))
    );
    assert_eq!(get(&node, "missing")?, Term::Nil);
    owned_targets_and_internal_names(&node)
}

/// A target id another Flow holds is not a schedule's to reuse, and the
/// schedule namespace is internal.
fn owned_targets_and_internal_names(node: &QueueNode) -> TestResult {
    ok(
        node,
        "FLOW.CREATE",
        vec![
            ("id", text("taken")),
            ("type", text("manual-owner")),
            ("partition_key", text("tp")),
            ("correlation_id", text("manual")),
            ("now_ms", Term::Int(1_000)),
        ],
    )?;
    ok(
        node,
        "FLOW.SCHEDULE.CREATE",
        vec![
            ("id", text("o")),
            ("target", target("id", "taken", "tc")),
            ("at_ms", Term::Int(5_000)),
            ("now_ms", Term::Int(1_000)),
        ],
    )?;
    let errors = field(&fire_due(node, 5_000)?, "errors");
    assert_eq!(
        errors,
        Term::List(vec![Term::List(vec![
            text("o"),
            text("ERR scheduled target id already exists with different owner")
        ])])
    );
    // The schedule namespace is internal.
    assert_eq!(
        flow::parse(
            "FLOW.CREATE",
            &object(vec![
                ("id", text("x")),
                ("type", text("__ferricstore_schedule"))
            ])
        )
        .err(),
        Some(ServiceError::rejected(
            "ERR flow type is reserved for internal use".to_owned()
        ))
    );
    Ok(())
}

#[test]
fn the_background_poll_fires_due_schedules() -> TestResult {
    let path = directory("flow-schedule-poll")?;
    let node = node(&path, Some(Duration::from_millis(20)))?;
    ok(
        &node,
        "FLOW.SCHEDULE.CREATE",
        vec![
            ("id", text("auto")),
            ("target", target("id", "auto-target", "ta")),
            ("delay_ms", Term::Int(0)),
        ],
    )?;
    for _ in 0..100 {
        if field(&get(&node, "auto")?, "state") == text("completed") {
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(field(&get(&node, "auto")?, "state"), text("completed"));
    assert_eq!(field(&flow_get(&node, "auto-target")?, "type"), text("ta"));
    Ok(())
}

fn listed(node: &QueueNode, entries: Vec<(&str, Term)>) -> TestResult<Vec<String>> {
    let Term::List(views) = ok(node, "FLOW.SCHEDULE.LIST", entries)? else {
        return Err("list is not a list".into());
    };
    Ok(views
        .iter()
        .filter_map(|view| match view.get(b"id") {
            Some(Term::Bin(id)) => Some(String::from_utf8_lossy(id).into_owned()),
            _ => None,
        })
        .collect())
}

#[test]
fn listing_filters_schedules_and_orders_them_by_due_time() -> TestResult {
    let path = directory("flow-schedule-list")?;
    let node = node(&path, None)?;
    for (id, kind, flow_type, at) in [
        ("a", "interval", "t1", 3_000),
        ("b", "one_shot", "t2", 1_000),
        ("c", "cron", "t1", 0),
    ] {
        let mut entries = vec![
            ("id", text(id)),
            (
                "target",
                target(
                    if kind == "one_shot" {
                        "id"
                    } else {
                        "id_prefix"
                    },
                    id,
                    flow_type,
                ),
            ),
            ("now_ms", Term::Int(0)),
        ];
        match kind {
            "interval" => entries.extend([
                ("every_ms", Term::Int(1_000)),
                ("start_at_ms", Term::Int(at)),
            ]),
            "cron" => entries.extend([("cron", text("0 12 * * *")), ("timezone", text("UTC"))]),
            _ => entries.push(("at_ms", Term::Int(at))),
        }
        ok(&node, "FLOW.SCHEDULE.CREATE", entries)?;
    }
    assert_eq!(listed(&node, Vec::new())?, ["b", "a", "c"]);
    assert_eq!(
        listed(
            &node,
            vec![("rev", Term::Bool(true)), ("count", Term::Int(2))]
        )?,
        ["c", "a"]
    );
    assert_eq!(listed(&node, vec![("kind", text("interval"))])?, ["a"]);
    assert_eq!(
        listed(&node, vec![("target_type", text("t1"))])?,
        ["a", "c"]
    );
    assert_eq!(listed(&node, vec![("timezone", text("Etc/UTC"))])?, ["c"]);
    assert_eq!(
        listed(
            &node,
            vec![("from_ms", Term::Int(500)), ("to_ms", Term::Int(5_000))]
        )?,
        ["b", "a"]
    );
    fire_due(&node, 1_000)?;
    assert_eq!(listed(&node, vec![("state", text("completed"))])?, ["b"]);
    assert_eq!(
        flow::parse(
            "FLOW.SCHEDULE.LIST",
            &object(vec![("from_ms", Term::Int(10)), ("to_ms", Term::Int(5))])
        )
        .err(),
        Some(ServiceError::rejected(
            "ERR flow schedule from_ms must be less than or equal to to_ms".to_owned()
        ))
    );
    Ok(())
}
