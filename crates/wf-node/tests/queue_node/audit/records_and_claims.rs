//! `flow_test/sections/flow_internal_keys_use_compact_partition_tags.exs`:
//! creates, payload reads, named values kept across a transition, claim
//! reply shapes, and the per-command results Elixir's pipelines give (the
//! node answers each command of a pipeline as it answers it alone; the
//! adapter keeps their order: `wf-api-tcp`
//! `session::flow::a_typed_pipeline_reports_each_command_in_order`).

use wf_application::commands::flow::Term;
use wf_node::composition::QueueNode;

use super::{claim, claimed, events, field, get, has, int, map, ok, refusal, run, text, texts};
use crate::{TestResult, directory, open};

fn create(node: &QueueNode, id: &str, flow_type: &str, extra: &[(&str, Term)]) -> TestResult {
    let mut entries = vec![("id", text(id)), ("type", text(flow_type))];
    entries.extend(extra.iter().cloned());
    ok(node, "FLOW.CREATE", &entries)?;
    Ok(())
}

fn claim_shape(node: &QueueNode, flow_type: &str, extra: &[(&str, Term)]) -> TestResult<Vec<Term>> {
    let mut entries = vec![
        ("type", text(flow_type)),
        ("worker", text("worker-compact")),
        ("partition_key", text("tenant")),
        ("limit", int(1)),
        ("now_ms", int(2)),
    ];
    entries.retain(|(key, _)| !extra.iter().any(|(given, _)| given == key));
    entries.extend(extra.iter().cloned());
    match claim(node, "FLOW.CLAIM_DUE", &entries)?? {
        Term::List(items) => Ok(items),
        other => Err(format!("claim reply is not a list: {other:?}").into()),
    }
}

#[test]
fn a_create_stores_the_flow_once() -> TestResult {
    // Elixir: "flow_create stores state and prevents duplicate ids".
    let node = open(&directory("audit-rc-create")?, 4)?;
    let options = [
        ("state", text("queued")),
        ("payload", text("payload:c")),
        ("run_at_ms", int(1_000)),
    ];
    create(&node, "c", "checkout", &options)?;
    let flow = get(&node, "c", &[])?;
    assert_eq!(field(&flow, "id"), text("c"));
    assert_eq!(field(&flow, "type"), text("checkout"));
    assert_eq!(field(&flow, "state"), text("queued"));
    assert_eq!(field(&flow, "version"), int(1));
    assert_eq!(field(&flow, "fencing_token"), int(0));
    let payload_ref = field(&flow, "payload_ref");
    assert!(matches!(&payload_ref, Term::Bin(_)));
    assert_ne!(payload_ref, text("payload:c"));
    let again = [("type", text("checkout")), ("state", text("queued"))];
    let duplicate = run(
        &node,
        "FLOW.CREATE",
        &[&[("id", text("c"))][..], &again[..]].concat(),
    )?;
    assert_eq!(refusal(duplicate)?, "ERR flow already exists");
    Ok(())
}

#[test]
fn a_transition_without_value_options_keeps_named_values() -> TestResult {
    // Elixir: "flow_transition without value options preserves existing
    // named value refs".
    let node = open(&directory("audit-rc-named")?, 4)?;
    let options = [
        ("state", text("queued")),
        ("values", map(&[("doc", text("v1"))])),
        ("run_at_ms", int(1_000)),
        ("now_ms", int(1_000)),
    ];
    create(&node, "n", "checkout", &options)?;
    let flow = get(&node, "n", &[])?;
    let refs = field(&flow, "value_refs");
    assert!(matches!(field(&refs, "doc"), Term::Map(_)));
    ok(
        &node,
        "FLOW.TRANSITION",
        &[
            ("id", text("n")),
            ("from_state", text("queued")),
            ("to_state", text("ready")),
            ("fencing_token", int(0)),
            ("run_at_ms", int(2_000)),
            ("now_ms", int(1_100)),
        ],
    )?;
    let moved = get(&node, "n", &[])?;
    assert_eq!(field(&moved, "state"), text("ready"));
    assert_eq!(field(&moved, "value_refs"), refs);
    let hydrated = get(&node, "n", &[("values", Term::Bool(true))])?;
    assert_eq!(field(&hydrated, "values"), map(&[("doc", text("v1"))]));
    Ok(())
}

#[test]
fn a_read_returns_the_payload_only_when_asked_and_within_its_cap() -> TestResult {
    // Elixir: "flow_get hydrates payload refs only when full or payload is
    // requested" and the inline part of "flow_claim_due returns payload
    // inline up to cap without rolling back missing payloads" (a payload
    // the store lost is Elixir's storage case; the node cannot lose one).
    let node = open(&directory("audit-rc-payload")?, 4)?;
    let options = [
        ("state", text("queued")),
        ("payload", text("payload-body")),
        ("run_at_ms", int(1_000)),
    ];
    create(&node, "p", "checkout", &options)?;
    let plain = get(&node, "p", &[])?;
    assert!(!has(&plain, "payload") && !has(&plain, "payload_omitted"));
    let full = get(&node, "p", &[("full", Term::Bool(true))])?;
    assert_eq!(field(&full, "payload"), text("payload-body"));
    let size = field(&full, "payload_size");
    assert!(matches!(size, Term::Int(bytes) if bytes > 0));
    assert!(!has(&full, "payload_omitted"));
    let without = get(&node, "p", &[("payload", Term::Bool(false))])?;
    assert!(!has(&without, "payload") && !has(&without, "payload_omitted"));
    let capped = get(
        &node,
        "p",
        &[("full", Term::Bool(true)), ("payload_max_bytes", int(4))],
    )?;
    assert_eq!(field(&capped, "payload_omitted"), Term::Bool(true));
    assert_eq!(field(&capped, "payload_size"), size);
    assert!(!has(&capped, "payload"));

    let options = [
        ("state", text("queued")),
        ("payload", text("worker-input")),
        ("run_at_ms", int(1_000)),
    ];
    create(&node, "w", "claim-payload", &options)?;
    let records = claimed(
        &node,
        &[
            ("type", text("claim-payload")),
            ("worker", text("worker-a")),
            ("limit", int(1)),
            ("now_ms", int(1_000)),
            ("payload_max_bytes", int(64)),
        ],
    )?;
    let [record] = records.as_slice() else {
        return Err("one claim".into());
    };
    assert_eq!(field(record, "id"), text("w"));
    assert_eq!(field(record, "payload"), text("worker-input"));
    assert!(matches!(field(record, "payload_size"), Term::Int(bytes) if bytes > 0));
    Ok(())
}

#[test]
fn claims_answer_in_the_shape_they_ask_for() -> TestResult {
    // Elixir: "claim_due supports compact job responses", "claim_due
    // compact state response includes the claimed run state", "claim_due
    // multi-state precheck does not miss work after an empty route state"
    // and the reply shapes of "pipeline_claim_due_batch coalesces job-only
    // claim responses".
    let node = open(&directory("audit-rc-shapes")?, 4)?;
    let at = |state: &str| {
        [
            ("state", text(state)),
            ("partition_key", text("tenant")),
            ("now_ms", int(1)),
            ("run_at_ms", int(1)),
        ]
    };
    create(&node, "jc", "compact", &at("queued"))?;
    let jobs = claim_shape(&node, "compact", &[("return", text("jobs_compact"))])?;
    let [Term::List(job)] = jobs.as_slice() else {
        return Err(format!("one compact job expected, got {jobs:?}").into());
    };
    let [id, partition, token, fence] = job.as_slice() else {
        return Err(format!("four fields expected, got {job:?}").into());
    };
    assert_eq!((id, partition), (&text("jc"), &text("tenant")));
    assert!(matches!(token, Term::Bin(_)) && matches!(fence, Term::Int(_)));

    for (id, flow_type, states) in [
        ("js", "compact-state", texts(&["ready"])),
        ("jm", "multi-state", texts(&["queued", "ready"])),
    ] {
        create(&node, id, flow_type, &at("ready"))?;
        let shape = [("return", text("jobs_compact_state")), ("states", states)];
        let jobs = claim_shape(&node, flow_type, &shape)?;
        let [Term::List(job)] = jobs.as_slice() else {
            return Err(format!("one job expected, got {jobs:?}").into());
        };
        let [got, partition, token, fence, state] = job.as_slice() else {
            return Err(format!("five fields expected, got {job:?}").into());
        };
        assert_eq!(
            (got, partition, state),
            (&text(id), &text("tenant"), &text("ready"))
        );
        assert!(matches!(token, Term::Bin(_)) && matches!(fence, Term::Int(_)));
    }

    for id in ["j1", "j2"] {
        create(&node, id, "jobs", &at("queued"))?;
    }
    for _ in 0..2 {
        let jobs = claim_shape(
            &node,
            "jobs",
            &[("return", text("jobs")), ("worker", text("worker-a"))],
        )?;
        let [job] = jobs.as_slice() else {
            return Err(format!("one job expected, got {jobs:?}").into());
        };
        assert!(matches!(field(job, "lease_token"), Term::Bin(_)));
        assert!(matches!(field(job, "fencing_token"), Term::Int(_)));
        assert!(!has(job, "version"), "{job:?}");
    }
    Ok(())
}

#[test]
fn claims_one_after_another_keep_each_worker_s_leases() -> TestResult {
    // Elixir: "pipeline_claim_due_batch coalesces compatible claims and
    // preserves worker boundaries" (coalescing is Elixir's; each claim
    // still leases to its own worker).
    let node = open(&directory("audit-rc-workers")?, 4)?;
    for id in ["c1", "c2", "c3"] {
        let options = [
            ("partition_key", text("tenant")),
            ("now_ms", int(1)),
            ("run_at_ms", int(1)),
        ];
        create(&node, id, "pipeline-claim", &options)?;
    }
    let mut ids = Vec::new();
    for worker in ["worker-a", "worker-a", "worker-b"] {
        let records = claim_shape(&node, "pipeline-claim", &[("worker", text(worker))])?;
        let [record] = records.as_slice() else {
            return Err("one claim".into());
        };
        assert_eq!(field(record, "lease_owner"), text(worker));
        ids.push(field(record, "id"));
    }
    ids.sort_by_key(|id| format!("{id:?}"));
    assert_eq!(ids, [text("c1"), text("c2"), text("c3")]);
    Ok(())
}

#[test]
fn each_command_of_an_independent_batch_answers_for_itself() -> TestResult {
    // Elixir: "pipeline_write_batch_independent works in raft mode",
    // "... terminal commands preserve per-command success", "... terminal
    // commands preserve duplicate flow order" and "... transitions preserve
    // duplicate flow order".
    let node = open(&directory("audit-rc-independent")?, 4)?;
    let in_p = [("partition_key", text("pipe"))];
    let options = [
        ("partition_key", text("pipe")),
        ("now_ms", int(1_000)),
        ("run_at_ms", int(1_000)),
    ];
    for id in ["t1", "t2", "t3"] {
        create(&node, id, "pipeline-terminal", &options)?;
    }
    let records = claimed(
        &node,
        &[
            ("type", text("pipeline-terminal")),
            ("partition_key", text("pipe")),
            ("worker", text("pipeline-terminal")),
            ("limit", int(3)),
            ("now_ms", int(1_000)),
            ("lease_ms", int(30_000)),
        ],
    )?;
    assert_eq!(records.len(), 3);
    let complete = |record: &Term, token: Term| {
        run(
            &node,
            "FLOW.COMPLETE",
            &[
                ("id", field(record, "id")),
                ("partition_key", text("pipe")),
                ("lease_token", token),
                ("fencing_token", field(record, "fencing_token")),
                ("now_ms", int(1_001)),
            ],
        )
    };
    let mut outcomes = Vec::new();
    for (index, record) in records.iter().enumerate() {
        let token = if index == 1 {
            text("wrong-token")
        } else {
            field(record, "lease_token")
        };
        outcomes.push(complete(record, token)?.is_ok());
    }
    assert_eq!(outcomes, [true, false, true]);
    let states: Vec<Term> = records
        .iter()
        .map(|record| -> TestResult<Term> {
            let Term::Bin(id) = field(record, "id") else {
                return Err("no id".into());
            };
            Ok(field(&get(&node, &String::from_utf8(id)?, &in_p)?, "state"))
        })
        .collect::<TestResult<_>>()?;
    assert_eq!(
        states,
        [text("completed"), text("running"), text("completed")]
    );

    repeated_completion_and_ordered_transitions(&node, &options)
}

/// The same completion twice answers `OK` both times with one completed
/// event, and two transitions of one Flow apply in order.
fn repeated_completion_and_ordered_transitions(
    node: &QueueNode,
    options: &[(&str, Term)],
) -> TestResult {
    let in_p = [("partition_key", text("pipe"))];
    let complete = |record: &Term, token: Term| {
        run(
            node,
            "FLOW.COMPLETE",
            &[
                ("id", field(record, "id")),
                ("partition_key", text("pipe")),
                ("lease_token", token),
                ("fencing_token", field(record, "fencing_token")),
                ("now_ms", int(1_001)),
            ],
        )
    };
    // The same completion twice: both answer OK, one completed event.
    create(node, "dup", "pipeline-dup", options)?;
    let records = claimed(
        node,
        &[
            ("type", text("pipeline-dup")),
            ("partition_key", text("pipe")),
            ("worker", text("w")),
            ("limit", int(1)),
            ("now_ms", int(1_000)),
        ],
    )?;
    let [record] = records.as_slice() else {
        return Err("one claim".into());
    };
    for _ in 0..2 {
        assert_eq!(complete(record, field(record, "lease_token"))??, text("OK"));
    }
    let completed = events(node, "dup", &in_p)?
        .iter()
        .filter(|fields| field(fields, "event") == text("completed"))
        .count();
    assert_eq!(completed, 1);

    // Two transitions of one Flow in order.
    create(node, "moves", "pipeline-moves", options)?;
    for (from, to, now) in [("queued", "ready", 1_001), ("ready", "processing", 1_002)] {
        ok(
            node,
            "FLOW.TRANSITION",
            &[
                ("id", text("moves")),
                ("partition_key", text("pipe")),
                ("from_state", text(from)),
                ("to_state", text(to)),
                ("fencing_token", int(0)),
                ("now_ms", int(now)),
            ],
        )?;
    }
    let moved = get(node, "moves", &in_p)?;
    assert_eq!(
        (field(&moved, "state"), field(&moved, "version")),
        (text("processing"), int(3))
    );
    Ok(())
}
