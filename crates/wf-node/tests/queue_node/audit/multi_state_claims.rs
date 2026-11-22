//! `flow_test/sections/claim_due_multi_state_claims_earliest_due_job_
//! instead_first_listed_state.exs`: claims over several states, partitions
//! and shards, claims without a clock, blocking claims woken by creates and
//! retries, and idempotent creates. Elixir's pipelined claims port as the
//! claims they batch: the node answers each one as it answers it alone.

use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use wf_application::commands::flow::Term;
use wf_node::composition::QueueNode;

use super::{claim, claimed, events, field, get, int, map, ok, refusal, run, text, texts};
use crate::{TestResult, directory, open};

fn wall() -> TestResult<i64> {
    Ok(i64::try_from(
        SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis(),
    )?)
}

fn create(node: &QueueNode, id: &str, flow_type: &str, extra: &[(&str, Term)]) -> TestResult {
    let mut entries = vec![
        ("id", text(id)),
        ("type", text(flow_type)),
        ("state", text("queued")),
    ];
    entries.retain(|(key, _)| !extra.iter().any(|(given, _)| given == key));
    entries.extend(extra.iter().cloned());
    ok(node, "FLOW.CREATE", &entries)?;
    Ok(())
}

fn claim_with(node: &QueueNode, flow_type: &str, extra: &[(&str, Term)]) -> TestResult<Vec<Term>> {
    let mut entries = vec![
        ("type", text(flow_type)),
        ("worker", text("worker-a")),
        ("limit", int(1)),
    ];
    entries.retain(|(key, _)| !extra.iter().any(|(given, _)| given == key));
    entries.extend(extra.iter().cloned());
    claimed(node, &entries)
}

fn sorted_ids(records: &[Term]) -> Vec<Term> {
    let mut ids: Vec<Term> = records.iter().map(|record| field(record, "id")).collect();
    ids.sort_by_key(|id| format!("{id:?}"));
    ids
}

#[test]
fn a_claim_over_several_states_takes_the_earliest_due() -> TestResult {
    // Elixir: "claim_due multi-state claims earliest due job instead of
    // first listed state".
    let node = open(&directory("audit-multi-earliest")?, 4)?;
    let at = |state: &str, due: i64| {
        [
            ("state", text(state)),
            ("partition_key", text("tenant")),
            ("now_ms", int(1)),
            ("run_at_ms", int(due)),
        ]
    };
    create(&node, "queued", "multi", &at("queued", 100))?;
    create(&node, "retry", "multi", &at("retry", 10))?;
    let reply = claim(
        &node,
        "FLOW.CLAIM_DUE",
        &[
            ("type", text("multi")),
            ("states", texts(&["queued", "retry"])),
            ("worker", text("worker-multi")),
            ("partition_key", text("tenant")),
            ("limit", int(1)),
            ("now_ms", int(101)),
            ("return", text("jobs_compact_state")),
        ],
    )??;
    let Term::List(jobs) = reply else {
        return Err("jobs are not a list".into());
    };
    let [Term::List(job)] = jobs.as_slice() else {
        return Err(format!("one job expected, got {jobs:?}").into());
    };
    let [id, partition, token, fence, state] = job.as_slice() else {
        return Err(format!("five fields expected, got {job:?}").into());
    };
    assert_eq!(
        (id, partition, state),
        (&text("retry"), &text("tenant"), &text("retry"))
    );
    assert!(matches!(token, Term::Bin(_)) && matches!(fence, Term::Int(_)));
    Ok(())
}

#[test]
fn claims_per_partition_take_one_each_without_overclaiming() -> TestResult {
    // Elixir: "pipeline_claim_due_batch groups interleaved independent
    // partitions" and "... batches distinct partition claims without
    // overclaiming".
    let node = open(&directory("audit-multi-partitions")?, 4)?;
    let partitions = ["p1", "p2", "p3", "p4"];
    for partition in partitions {
        for slot in ["first", "second"] {
            let options = [
                ("partition_key", text(partition)),
                ("now_ms", int(1)),
                ("run_at_ms", int(1)),
            ];
            create(&node, &format!("{partition}-{slot}"), "distinct", &options)?;
        }
    }
    for partition in partitions {
        let one = claim_with(
            &node,
            "distinct",
            &[("partition_key", text(partition)), ("now_ms", int(2))],
        )?;
        assert_eq!(one.len(), 1, "{partition}");
        let rest = claim_with(
            &node,
            "distinct",
            &[
                ("partition_key", text(partition)),
                ("limit", int(10)),
                ("now_ms", int(3)),
            ],
        )?;
        assert_eq!(rest.len(), 1, "{partition}");
        assert_ne!(sorted_ids(&one), sorted_ids(&rest));
    }
    Ok(())
}

#[test]
fn a_claim_spans_listed_auto_and_any_partitions() -> TestResult {
    // Elixir: "pipeline_claim_due_batch singleton partition list claims
    // across shards", "... singleton auto partition scans across shards" and
    // "... singleton any partition scans across shards".
    let node = open(&directory("audit-multi-scopes")?, 4)?;
    for (id, partition) in [("list-a", "pa"), ("list-b", "pb")] {
        let options = [
            ("partition_key", text(partition)),
            ("now_ms", int(1)),
            ("run_at_ms", int(1)),
        ];
        create(&node, id, "listed", &options)?;
    }
    let listed = claim_with(
        &node,
        "listed",
        &[
            ("partition_keys", texts(&["pa", "pb"])),
            ("limit", int(2)),
            ("now_ms", int(2)),
        ],
    )?;
    assert_eq!(sorted_ids(&listed), [text("list-a"), text("list-b")]);
    for id in ["auto-a", "auto-b", "auto-c", "auto-d"] {
        create(
            &node,
            id,
            "auto",
            &[("now_ms", int(1)), ("run_at_ms", int(1))],
        )?;
    }
    let auto = claim_with(&node, "auto", &[("limit", int(4)), ("now_ms", int(2))])?;
    assert_eq!(auto.len(), 4);
    for (id, partition) in [("any-a", "qa"), ("any-b", "qb")] {
        let options = [
            ("partition_key", text(partition)),
            ("now_ms", int(1)),
            ("run_at_ms", int(1)),
        ];
        create(&node, id, "any", &options)?;
    }
    let any = claim_with(
        &node,
        "any",
        &[
            ("partition_key", text("any")),
            ("limit", int(2)),
            ("now_ms", int(2)),
        ],
    )?;
    assert_eq!(sorted_ids(&any), [text("any-a"), text("any-b")]);
    Ok(())
}

#[test]
fn a_claim_without_a_clock_takes_work_due_in_the_past() -> TestResult {
    // Elixir: "pipeline_claim_due_batch accepts omitted NOW option".
    let node = open(&directory("audit-multi-no-now")?, 4)?;
    let now = wall()?;
    let options = [
        ("partition_key", text("tenant")),
        ("now_ms", int(now)),
        ("run_at_ms", int(now - 1_000)),
    ];
    create(&node, "past", "no-now", &options)?;
    let records = claim_with(&node, "no-now", &[("partition_key", text("tenant"))])?;
    let [record] = records.as_slice() else {
        return Err("one claim".into());
    };
    assert_eq!(
        (field(record, "id"), field(record, "lease_owner")),
        (text("past"), text("worker-a"))
    );
    Ok(())
}

/// Starts a blocking claim of `flow_type` in `tenant` on its own thread.
fn blocking(
    node: &std::sync::Arc<QueueNode>,
    flow_type: &str,
    extra: Vec<(&'static str, Term)>,
) -> thread::JoinHandle<Result<Vec<Term>, String>> {
    let node = std::sync::Arc::clone(node);
    let flow_type = flow_type.to_owned();
    thread::spawn(move || {
        let mut entries = vec![
            ("partition_key", text("tenant")),
            ("payload", Term::Bool(false)),
        ];
        entries.extend(extra);
        claim_with(&node, &flow_type, &entries).map_err(|error| error.to_string())
    })
}

#[test]
fn a_blocking_claim_wakes_for_a_create_in_the_default_state() -> TestResult {
    // Elixir: "flow_claim_due BLOCK wakes when due create uses default
    // state" (its waiter-count and spurious-wake checks are Elixir's
    // waiter registry).
    let node = std::sync::Arc::new(open(&directory("audit-multi-block-default")?, 4)?);
    let waiting = blocking(
        &node,
        "block-default",
        vec![
            ("state", text("queued")),
            ("priority", int(0)),
            ("block_ms", int(2_000)),
        ],
    );
    thread::sleep(Duration::from_millis(100));
    ok(
        &node,
        "FLOW.CREATE",
        &[
            ("id", text("woken")),
            ("type", text("block-default")),
            ("partition_key", text("tenant")),
            ("now_ms", int(1_000)),
            ("run_at_ms", int(1_000)),
        ],
    )?;
    let records = waiting.join().map_err(|_| "waiter panicked")??;
    let [record] = records.as_slice() else {
        return Err(format!("one claim expected, got {records:?}").into());
    };
    assert_eq!(
        (field(record, "id"), field(record, "state")),
        (text("woken"), text("running"))
    );
    Ok(())
}

#[test]
fn one_waking_claim_takes_up_to_its_limit_and_the_others_keep_waiting() -> TestResult {
    // Elixir: "flow_claim_due BLOCK uses claim limit as waiter credit
    // capacity".
    let node = std::sync::Arc::new(open(&directory("audit-multi-block-credit")?, 4)?);
    let waiting: Vec<_> = (0..3)
        .map(|_| {
            blocking(
                &node,
                "block-credit",
                vec![("limit", int(10)), ("block_ms", int(600))],
            )
        })
        .collect();
    thread::sleep(Duration::from_millis(100));
    let ids: Vec<Term> = (1..=5)
        .map(|index| text(&format!("credit-{index}")))
        .collect();
    ok(
        &node,
        "FLOW.CREATE_MANY",
        &[
            ("partition_key", text("tenant")),
            ("type", text("block-credit")),
            ("now_ms", int(1_000)),
            ("run_at_ms", int(1_000)),
            ("items", Term::List(ids.clone())),
        ],
    )?;
    let mut sizes: Vec<usize> = Vec::new();
    let mut taken = Vec::new();
    for waiter in waiting {
        let records = waiter.join().map_err(|_| "waiter panicked")??;
        sizes.push(records.len());
        taken.extend(records);
    }
    sizes.sort_unstable();
    assert_eq!(sizes, [0, 0, 5]);
    let mut expected = ids;
    expected.sort_by_key(|id| format!("{id:?}"));
    assert_eq!(sorted_ids(&taken), expected);
    Ok(())
}

#[test]
fn a_blocking_claim_wakes_for_a_retry_due_after_its_backoff() -> TestResult {
    // Elixir: "flow_claim_due BLOCK is not consumed by retry policy backoff
    // wake" and "flow_claim_due BLOCK wakes for retry scheduled before
    // waiter registers".
    let node = std::sync::Arc::new(open(&directory("audit-multi-block-retry")?, 4)?);
    for (id, backoff, wait_first) in [("before", 80, true), ("after", 150, false)] {
        let now = wall()?;
        let options = [
            ("partition_key", text("tenant")),
            ("run_at_ms", int(now)),
            ("now_ms", int(now)),
        ];
        create(&node, id, &format!("retry-{id}"), &options)?;
        let flow_type = format!("retry-{id}");
        let records = claim_with(
            &node,
            &flow_type,
            &[
                ("partition_key", text("tenant")),
                ("lease_ms", int(30_000)),
                ("now_ms", int(now)),
            ],
        )?;
        let [lease] = records.as_slice() else {
            return Err("one claim".into());
        };
        let waiter_options = vec![
            ("state", text("queued")),
            ("worker", text("worker-b")),
            ("block_ms", int(900)),
        ];
        let early = wait_first.then(|| blocking(&node, &flow_type, waiter_options.clone()));
        if wait_first {
            thread::sleep(Duration::from_millis(100));
        }
        let retry_now = wall()?;
        let policy = map(&[
            ("max_retries", int(3)),
            (
                "backoff",
                map(&[
                    ("kind", text("fixed")),
                    ("base_ms", int(backoff)),
                    ("max_ms", int(backoff)),
                    ("jitter_pct", int(0)),
                ]),
            ),
        ]);
        ok(
            &node,
            "FLOW.RETRY",
            &[
                ("id", text(id)),
                ("partition_key", text("tenant")),
                ("lease_token", field(lease, "lease_token")),
                ("fencing_token", field(lease, "fencing_token")),
                ("now_ms", int(retry_now)),
                ("retry", policy),
            ],
        )?;
        let next = field(
            &get(&node, id, &[("partition_key", text("tenant"))])?,
            "next_run_at_ms",
        );
        assert!(
            matches!(next, Term::Int(at) if at >= retry_now + backoff),
            "{next:?}"
        );
        let started = Instant::now();
        let waiter = early.unwrap_or_else(|| blocking(&node, &flow_type, waiter_options));
        let records = waiter.join().map_err(|_| "waiter panicked")??;
        let [record] = records.as_slice() else {
            return Err(format!("{id}: one reclaim expected, got {records:?}").into());
        };
        assert_eq!(
            (field(record, "id"), field(record, "lease_owner")),
            (text(id), text("worker-b"))
        );
        assert!(started.elapsed() < Duration::from_millis(900), "{id}");
    }
    Ok(())
}

#[test]
fn an_idempotent_create_answers_the_existing_flow_or_a_conflict() -> TestResult {
    // Elixir: "flow_create idempotent retry returns matching existing record
    // and rejects conflicts" and "... matches large blob-backed payloads".
    let node = open(&directory("audit-multi-idempotent")?, 4)?;
    let large = "blob-payload".repeat(24_000);
    for (id, payload) in [("small", "payload:small".to_owned()), ("large", large)] {
        let attempt = |body: &str, now: i64| {
            run(
                &node,
                "FLOW.CREATE",
                &[
                    ("id", text(id)),
                    ("type", text("checkout")),
                    ("state", text("queued")),
                    ("payload", text(body)),
                    ("run_at_ms", int(1_000)),
                    ("now_ms", int(now)),
                    ("idempotent", Term::Bool(true)),
                ],
            )
        };
        attempt(&payload, 10)??;
        let created = get(&node, id, &[])?;
        attempt(&payload, 20)??;
        let retried = get(&node, id, &[])?;
        for name in ["id", "version", "created_at_ms"] {
            assert_eq!(field(&retried, name), field(&created, name), "{id} {name}");
        }
        let names: Vec<Term> = events(&node, id, &[])?
            .iter()
            .map(|fields| field(fields, "event"))
            .collect();
        assert_eq!(names, [text("created")], "{id}");
        assert_eq!(
            refusal(attempt(&format!("different:{id}"), 30)?)?,
            "ERR flow idempotency conflict"
        );
    }
    let capped = get(
        &node,
        "large",
        &[
            ("full", Term::Bool(true)),
            ("payload_max_bytes", int(65_536)),
        ],
    )?;
    assert_eq!(field(&capped, "payload_omitted"), Term::Bool(true));
    assert!(matches!(field(&capped, "payload_size"), Term::Int(size) if size > 65_536));
    Ok(())
}
