//! `flow_test/sections/flow_history_hot_max_rejects_values_above_configured_
//! maximum.exs`, first part: request limits, rewinds and terminal retention
//! times. Elixir's limits are settings its tests lower to 2; the node's are
//! Elixir's defaults (history and claims) or its largest setting (batches).

use wf_application::commands::flow::Term;
use wf_node::composition::QueueNode;

use super::{claim, claimed, events, field, get, int, map, ok, refusal, run, text};
use crate::{TestResult, directory, open};

/// Creates `id` of `flow_type` with `extra` options.
fn create(node: &QueueNode, id: &str, flow_type: &str, extra: &[(&str, Term)]) -> TestResult<Term> {
    let mut entries = vec![("id", text(id)), ("type", text(flow_type))];
    entries.extend(extra.iter().cloned());
    ok(node, "FLOW.CREATE", &entries)?;
    let partition: Vec<(&str, Term)> = extra
        .iter()
        .filter(|(key, _)| *key == "partition_key")
        .cloned()
        .collect();
    get(node, id, &partition)
}

/// Claims the one due Flow of `flow_type` at `now`.
fn claim_one(
    node: &QueueNode,
    flow_type: &str,
    now: i64,
    extra: &[(&str, Term)],
) -> TestResult<Term> {
    let mut entries = vec![
        ("type", text(flow_type)),
        ("worker", text("worker-a")),
        ("lease_ms", int(30_000)),
        ("limit", int(1)),
        ("now_ms", int(now)),
    ];
    entries.extend(extra.iter().cloned());
    let records = claimed(node, &entries)?;
    let [record] = records.as_slice() else {
        return Err(format!("one claim expected, got {records:?}").into());
    };
    Ok(record.clone())
}

fn complete(
    node: &QueueNode,
    claim: &Term,
    now: Option<i64>,
    extra: &[(&str, Term)],
) -> TestResult<Term> {
    let mut entries = vec![
        ("id", field(claim, "id")),
        ("lease_token", field(claim, "lease_token")),
        ("fencing_token", field(claim, "fencing_token")),
    ];
    entries.extend(now.map(|now| ("now_ms", int(now))));
    entries.extend(extra.iter().cloned());
    ok(node, "FLOW.COMPLETE", &entries)?;
    let partition: Vec<(&str, Term)> = extra
        .iter()
        .filter(|(key, _)| *key == "partition_key")
        .cloned()
        .collect();
    get(node, &text_of(&field(claim, "id"))?, &partition)
}

fn text_of(term: &Term) -> TestResult<String> {
    match term {
        Term::Bin(bytes) => Ok(String::from_utf8(bytes.clone())?),
        other => Err(format!("not text: {other:?}").into()),
    }
}

fn int_of(term: &Term) -> TestResult<i64> {
    match term {
        Term::Int(value) => Ok(*value),
        other => Err(format!("not an integer: {other:?}").into()),
    }
}

/// The history of `id` as `(event id, fields)` pairs, oldest first.
fn history(node: &QueueNode, id: &str, extra: &[(&str, Term)]) -> TestResult<Vec<(Term, Term)>> {
    let mut entries = vec![("id", text(id)), ("count", int(10))];
    entries.extend(extra.iter().cloned());
    let Term::List(entries) = ok(node, "FLOW.HISTORY", &entries)? else {
        return Err("history is not a list".into());
    };
    Ok(entries
        .into_iter()
        .filter_map(|entry| match entry {
            Term::List(mut parts) if parts.len() == 2 => {
                let fields = parts.pop()?;
                Some((parts.pop()?, fields))
            }
            _ => None,
        })
        .collect())
}

fn first_event(node: &QueueNode, id: &str, extra: &[(&str, Term)]) -> TestResult<Term> {
    let events = history(node, id, extra)?;
    Ok(events.first().ok_or("no history")?.0.clone())
}

fn rewind(
    node: &QueueNode,
    id: &str,
    entries: &[(&str, Term)],
) -> TestResult<Result<Term, String>> {
    let mut all = vec![("id", text(id))];
    all.extend(entries.iter().cloned());
    Ok(run(node, "FLOW.REWIND", &all)?.map_err(|error| error.to_string()))
}

#[test]
fn requests_past_the_node_s_limits_are_refused() -> TestResult {
    // Elixir: "flow history hot max rejects values above configured
    // maximum", "flow create_many and transition_many reject oversized
    // batches" and "flow claim_due rejects oversized limit".
    let node = open(&directory("audit-hist-limits")?, 4)?;
    let hot = run(
        &node,
        "FLOW.CREATE",
        &[
            ("id", text("h")),
            ("type", text("audit-history-max")),
            ("history_hot_max_events", int(10_001)),
        ],
    )?;
    assert_eq!(
        refusal(hot)?,
        "ERR flow history_hot_max_events exceeds maximum 10000"
    );
    let too_many: Vec<Term> = (0..100_001)
        .map(|index| text(&format!("f{index}")))
        .collect();
    let created = run(
        &node,
        "FLOW.CREATE_MANY",
        &[
            ("partition_key", text("tenant-batch-cap")),
            ("type", text("batch-cap")),
            ("items", Term::List(too_many)),
        ],
    )?;
    let capped = "ERR flow batch item count exceeds maximum 100000";
    assert_eq!(refusal(created)?, capped);
    let items: Vec<Term> = (0..100_001)
        .map(|index| {
            map(&[
                ("id", text(&format!("f{index}"))),
                ("fencing_token", int(0)),
            ])
        })
        .collect();
    let moved = run(
        &node,
        "FLOW.TRANSITION_MANY",
        &[
            ("partition_key", text("tenant-batch-cap")),
            ("from_state", text("queued")),
            ("to_state", text("waiting")),
            ("items", Term::List(items)),
        ],
    )?;
    assert_eq!(refusal(moved)?, capped);
    for name in ["FLOW.CLAIM_DUE", "FLOW.RECLAIM"] {
        let limited = claim(
            &node,
            name,
            &[
                ("type", text("claim-limit-cap")),
                ("worker", text("worker-a")),
                ("limit", int(10_001)),
            ],
        )?;
        assert_eq!(
            refusal(limited)?,
            "ERR flow limit exceeds maximum 10000",
            "{name}"
        );
    }
    Ok(())
}

#[test]
fn a_rewind_to_an_event_trimmed_from_history_is_refused() -> TestResult {
    // Elixir: "flow_rewind rejects trimmed target event with stale stream
    // index".
    let node = open(&directory("audit-hist-trimmed")?, 4)?;
    let limits = [
        ("run_at_ms", int(1_000)),
        ("history_hot_max_events", int(2)),
        ("history_max_events", int(2)),
        ("now_ms", int(1_000)),
    ];
    create(&node, "trim", "rewind-trimmed", &limits)?;
    let created_event = first_event(&node, "trim", &[])?;
    let claim = claim_one(&node, "rewind-trimmed", 1_000, &[])?;
    complete(&node, &claim, Some(2_000), &[])?;
    let names: Vec<Term> = events(&node, "trim", &[])?
        .iter()
        .map(|fields| field(fields, "event"))
        .collect();
    assert_eq!(names, [text("claimed"), text("completed")]);
    let target = [
        ("to_event", created_event),
        ("expect_state", text("completed")),
        ("now_ms", int(3_000)),
    ];
    assert_eq!(
        rewind(&node, "trim", &target)?,
        Err("ERR flow rewind target event not found".to_owned())
    );
    Ok(())
}

fn info_counts(node: &QueueNode, flow_type: &str) -> TestResult<(Term, Term)> {
    let info = ok(node, "FLOW.INFO", &[("type", text(flow_type))])?;
    Ok((field(&info, "queued"), field(&info, "completed")))
}

#[test]
fn a_rewind_restores_an_earlier_state_and_its_counts() -> TestResult {
    // Elixir: "flow_rewind restores a previous history state and reindexes
    // atomically".
    let node = open(&directory("audit-hist-rewind")?, 4)?;
    create(
        &node,
        "r",
        "rewind",
        &[("run_at_ms", int(1_000)), ("now_ms", int(1_000))],
    )?;
    let events_before = history(&node, "r", &[])?;
    let (created_event, created) = events_before.first().ok_or("no history")?.clone();
    assert_eq!(field(&created, "event"), text("created"));
    assert_eq!(field(&created, "state"), text("queued"));
    let claim = claim_one(&node, "rewind", 1_000, &[])?;
    let completed = complete(&node, &claim, Some(2_000), &[])?;
    assert_eq!(field(&completed, "state"), text("completed"));
    assert_eq!(info_counts(&node, "rewind")?, (int(0), int(1)));
    let target = [
        ("to_event", created_event.clone()),
        ("run_at_ms", int(5_000)),
        ("expect_state", text("completed")),
        ("now_ms", int(3_000)),
    ];
    assert_eq!(rewind(&node, "r", &target)?, Ok(text("OK")));
    let rewound = get(&node, "r", &[])?;
    assert_eq!(field(&rewound, "state"), text("queued"));
    assert_eq!(field(&rewound, "next_run_at_ms"), int(5_000));
    assert_eq!(field(&rewound, "lease_token"), Term::Nil);
    assert_eq!(field(&rewound, "lease_owner"), Term::Nil);
    let fence = int_of(&field(&completed, "fencing_token"))?;
    assert_eq!(field(&rewound, "fencing_token"), int(fence + 1));
    assert_eq!(info_counts(&node, "rewind")?, (int(1), int(0)));
    let again = claim_one(&node, "rewind", 5_000, &[("worker", text("worker-b"))])?;
    assert_eq!(field(&again, "id"), text("r"));
    assert_eq!(field(&again, "state"), text("running"));
    let rewound_event = history(&node, "r", &[])?.into_iter().any(|(_, fields)| {
        field(&fields, "event") == text("rewound")
            && field(&fields, "rewound_to_event_id") == created_event
    });
    assert!(rewound_event);
    Ok(())
}

fn mget(node: &QueueNode, refs: Vec<Term>) -> TestResult<Term> {
    ok(node, "FLOW.VALUE.MGET", &[("refs", Term::List(refs))])
}

#[test]
fn a_rewind_keeps_its_reason_and_a_later_rewind_can_restore_it() -> TestResult {
    // Elixir: "flow_rewind persists the supplied reason and its history
    // reference" and "flow_rewind reads back a large binary reason".
    let node = open(&directory("audit-hist-reason")?, 4)?;
    let partition = [("partition_key", text("reason-p"))];
    let mut options = partition.to_vec();
    options.extend([("now_ms", int(1_000)), ("run_at_ms", int(1_000))]);
    create(&node, "why", "rewind-reason", &options)?;
    let created_event = first_event(&node, "why", &partition)?;
    let mut target = partition.to_vec();
    target.extend([
        ("to_event", created_event.clone()),
        ("expect_state", text("queued")),
        ("reason", text("operator corrected the input")),
        ("now_ms", int(2_000)),
    ]);
    assert_eq!(rewind(&node, "why", &target)?, Ok(text("OK")));
    let rewound = get(&node, "why", &partition)?;
    let error_ref = field(&rewound, "error_ref");
    assert!(matches!(error_ref, Term::Bin(_)));
    let reason = Term::List(vec![text("operator corrected the input")]);
    assert_eq!(mget(&node, vec![error_ref.clone()])?, reason);
    let events = history(&node, "why", &partition)?;
    let (rewound_event, fields) = events
        .iter()
        .find(|(_, fields)| field(fields, "event") == text("rewound"))
        .ok_or("no rewound event")?
        .clone();
    assert_eq!(field(&fields, "error_ref"), error_ref);
    let mut back = partition.to_vec();
    back.extend([("to_event", rewound_event), ("now_ms", int(3_000))]);
    assert_eq!(rewind(&node, "why", &back)?, Ok(text("OK")));
    let restored = get(&node, "why", &partition)?;
    assert_eq!(field(&restored, "error_ref"), error_ref);
    assert_eq!(mget(&node, vec![error_ref])?, reason);
    let mut refused = partition.to_vec();
    refused.extend([
        ("to_event", created_event),
        ("expect_state", text("completed")),
        ("reason", text("must not replace the reason")),
        ("now_ms", int(4_000)),
    ]);
    assert!(rewind(&node, "why", &refused)?.is_err());
    assert_eq!(get(&node, "why", &partition)?, restored);

    let large: Vec<u8> = [0_u8, 255, 17].repeat(110_000);
    create(&node, "large", "rewind-large", &[("now_ms", int(1_000))])?;
    let created_event = first_event(&node, "large", &[])?;
    let target = [
        ("to_event", created_event),
        ("reason", Term::Bin(large.clone())),
        ("now_ms", int(2_000)),
    ];
    assert_eq!(rewind(&node, "large", &target)?, Ok(text("OK")));
    let error_ref = field(&get(&node, "large", &[])?, "error_ref");
    let read = ok(
        &node,
        "FLOW.VALUE.MGET",
        &[
            ("refs", Term::List(vec![error_ref])),
            ("max_bytes", int(1_000_000)),
        ],
    )?;
    assert_eq!(read, Term::List(vec![Term::Bin(large)]));
    Ok(())
}

#[test]
fn a_rewind_checks_its_target_expected_state_and_lease() -> TestResult {
    // Elixir: "flow_rewind validates target, expected state, and active
    // leases" and "flow_rewind refuses an event recorded while the Flow was
    // claimed".
    let node = open(&directory("audit-hist-guard")?, 4)?;
    create(
        &node,
        "g",
        "rewind-guard",
        &[("run_at_ms", int(1_000)), ("now_ms", int(500))],
    )?;
    let created_event = first_event(&node, "g", &[])?;
    let wrong = [
        ("to_event", created_event.clone()),
        ("expect_state", text("completed")),
    ];
    assert_eq!(
        rewind(&node, "g", &wrong)?,
        Err("ERR flow wrong state".to_owned())
    );
    let claim = claim_one(&node, "rewind-guard", 1_000, &[])?;
    assert_eq!(
        rewind(&node, "g", &[("to_event", created_event)])?,
        Err("ERR flow cannot rewind leased flow".to_owned())
    );
    complete(&node, &claim, Some(2_000), &[])?;
    assert_eq!(
        rewind(&node, "g", &[("to_event", text("999999-0"))])?,
        Err("ERR flow rewind target event not found".to_owned())
    );

    create(
        &node,
        "c",
        "rewind-claimed",
        &[("run_at_ms", int(1_000)), ("now_ms", int(500))],
    )?;
    let claim = claim_one(&node, "rewind-claimed", 1_000, &[])?;
    ok(
        &node,
        "FLOW.TRANSITION",
        &[
            ("id", text("c")),
            ("from_state", text("running")),
            ("to_state", text("queued")),
            ("lease_token", field(&claim, "lease_token")),
            ("fencing_token", field(&claim, "fencing_token")),
            ("run_at_ms", int(5_000)),
            ("now_ms", int(1_001)),
        ],
    )?;
    let claim_events: Vec<Term> = history(&node, "c", &[])?
        .into_iter()
        .filter(|(_, fields)| field(fields, "event") == text("claimed"))
        .map(|(id, _)| id)
        .collect();
    let [claimed_event] = claim_events.as_slice() else {
        return Err(format!("one claim event expected, got {claim_events:?}").into());
    };
    let target = [("to_event", claimed_event.clone()), ("now_ms", int(1_002))];
    assert_eq!(
        rewind(&node, "c", &target)?,
        Err("ERR flow running state is only entered by FLOW.CLAIM_DUE".to_owned())
    );
    assert_eq!(field(&get(&node, "c", &[])?, "state"), text("queued"));
    Ok(())
}

#[test]
fn parents_and_children_are_not_rewound() -> TestResult {
    // Elixir: "flow_rewind rejects parent and child flows".
    let node = open(&directory("audit-hist-family")?, 4)?;
    let partition = [("partition_key", text("family"))];
    let mut options = partition.to_vec();
    options.extend([("state", text("dispatch")), ("now_ms", int(1_000))]);
    let parent = create(&node, "parent", "parent", &options)?;
    let parent_event = first_event(&node, "parent", &partition)?;
    let child = map(&[
        ("id", text("child")),
        ("type", text("child")),
        ("partition_key", text("family")),
    ]);
    ok(
        &node,
        "FLOW.SPAWN_CHILDREN",
        &[
            ("id", text("parent")),
            ("partition_key", text("family")),
            ("children", Term::List(vec![child])),
            ("group_id", text("rewind-fanout")),
            ("wait", text("all")),
            ("wait_state", text("waiting_children")),
            ("on_child_failed", text("ignore")),
            ("on_parent_closed", text("abandon_children")),
            (
                "exhaust_to",
                map(&[
                    ("success", text("children_done")),
                    ("failure", text("children_failed")),
                ]),
            ),
            ("from_state", text("dispatch")),
            ("fencing_token", field(&parent, "fencing_token")),
            ("now_ms", int(2_000)),
        ],
    )?;
    let child_event = first_event(&node, "child", &partition)?;
    let refused = Err("ERR flow cannot rewind parent or child flow".to_owned());
    let mut target = partition.to_vec();
    target.extend([
        ("to_event", parent_event),
        ("expect_state", text("waiting_children")),
        ("now_ms", int(3_000)),
    ]);
    assert_eq!(rewind(&node, "parent", &target)?, refused);
    let mut target = partition.to_vec();
    target.extend([
        ("to_event", child_event),
        ("expect_state", text("running")),
        ("now_ms", int(3_000)),
    ]);
    assert_eq!(rewind(&node, "child", &target)?, refused);
    Ok(())
}

#[test]
fn terminal_retention_starts_at_the_terminal_command() -> TestResult {
    // Elixir: "terminal retention from create expires flow state record",
    // "terminal retention uses wall-valid command time" and "retention
    // cleanup revalidates stale LMDB terminal candidate after rewind".
    let node = open(&directory("audit-hist-ttl")?, 4)?;
    let options = [
        ("run_at_ms", int(1_000)),
        ("retention_ttl_ms", int(20)),
        ("now_ms", int(1_000)),
    ];
    let created = create(&node, "ttl", "ttl", &options)?;
    assert_eq!(field(&created, "retention_ttl_ms"), int(20));
    assert_eq!(field(&created, "terminal_retention_until_ms"), Term::Nil);
    let claim = claim_one(&node, "ttl", 1_000, &[])?;
    complete(&node, &claim, None, &[])?;
    std::thread::sleep(std::time::Duration::from_millis(40));
    assert_eq!(get(&node, "ttl", &[])?, Term::Nil);

    // Elixir's command times: a minute past the wall clock.
    let wall = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?;
    let start = i64::try_from(wall.as_millis())? + 60_000;
    let options = [
        ("run_at_ms", int(start)),
        ("retention_ttl_ms", int(5_000)),
        ("now_ms", int(start)),
    ];
    create(&node, "later", "ttl-command-time", &options)?;
    let claim = claim_one(&node, "ttl-command-time", start, &[])?;
    let completed = complete(&node, &claim, Some(start + 10_000), &[])?;
    assert_eq!(
        field(&completed, "terminal_retention_until_ms"),
        int(start + 15_000)
    );

    let options = [
        ("run_at_ms", int(start)),
        ("retention_ttl_ms", int(20)),
        ("now_ms", int(start)),
    ];
    create(&node, "back", "retention-rewind", &options)?;
    let created_event = first_event(&node, "back", &[])?;
    let claim = claim_one(&node, "retention-rewind", start, &[])?;
    let completed = complete(&node, &claim, Some(start + 1_000), &[])?;
    assert_eq!(
        field(&completed, "terminal_retention_until_ms"),
        int(start + 1_020)
    );
    let target = [
        ("to_event", created_event),
        ("run_at_ms", int(start + 2_000)),
        ("expect_state", text("completed")),
        ("now_ms", int(start + 1_010)),
    ];
    assert_eq!(rewind(&node, "back", &target)?, Ok(text("OK")));
    let cleaned = ok(
        &node,
        "FLOW.RETENTION_CLEANUP",
        &[("limit", int(10)), ("now_ms", int(start + 2_000))],
    )?;
    assert_eq!(field(&cleaned, "flows"), int(0));
    assert_eq!(field(&get(&node, "back", &[])?, "state"), text("queued"));
    Ok(())
}
