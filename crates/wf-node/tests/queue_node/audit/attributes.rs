//! `flow_attributes_test.exs`, the record side: attributes stored and read
//! back, merged and deleted by transitions, carried through every lifecycle
//! command and a restart, returned by claims, and refused past their limits
//! without touching the Flow.

use wf_application::commands::flow::Term;
use wf_node::composition::QueueNode;

use super::{claim, claimed, field, get, int, map, ok, refusal, run, sorted, text, texts};
use crate::{TestResult, directory, open};

pub(super) const P: &str = "tenant-attrs";

fn in_p<'a>(entries: &[(&'a str, Term)]) -> Vec<(&'a str, Term)> {
    let mut all = vec![("partition_key", text(P))];
    all.extend(entries.iter().cloned());
    all
}

/// Creates `id` of `flow_type` in `P`, queued and due at `now`.
pub(super) fn create(
    node: &QueueNode,
    id: &str,
    flow_type: &str,
    attributes: Option<Term>,
    now: i64,
) -> TestResult {
    let mut entries = in_p(&[
        ("id", text(id)),
        ("type", text(flow_type)),
        ("state", text("queued")),
        ("run_at_ms", int(now)),
        ("now_ms", int(now)),
    ]);
    entries.extend(attributes.map(|attributes| ("attributes", attributes)));
    ok(node, "FLOW.CREATE", &entries)?;
    Ok(())
}

/// Claims the one due Flow of `flow_type` in `state`.
pub(super) fn claim_one(
    node: &QueueNode,
    flow_type: &str,
    state: &str,
    now: i64,
) -> TestResult<Term> {
    let records = claimed(
        node,
        &in_p(&[
            ("type", text(flow_type)),
            ("states", texts(&[state])),
            ("worker", text("w1")),
            ("limit", int(1)),
            ("now_ms", int(now)),
        ]),
    )?;
    let [record] = records.as_slice() else {
        return Err(format!("one claim expected, got {records:?}").into());
    };
    Ok(record.clone())
}

/// The lease of a claimed record, with the partition.
pub(super) fn lease(record: &Term) -> Vec<(&'static str, Term)> {
    vec![
        ("id", field(record, "id")),
        ("partition_key", text(P)),
        ("lease_token", field(record, "lease_token")),
        ("fencing_token", field(record, "fencing_token")),
    ]
}

pub(super) fn plus<'a>(
    mut entries: Vec<(&'a str, Term)>,
    extra: &[(&'a str, Term)],
) -> Vec<(&'a str, Term)> {
    entries.extend(extra.iter().cloned());
    entries
}

fn attributes_of(node: &QueueNode, id: &str) -> TestResult<Term> {
    Ok(sorted(&field(&get(node, id, &in_p(&[]))?, "attributes")))
}

fn assert_attributes(node: &QueueNode, id: &str, expected: &Term) -> TestResult {
    assert_eq!(attributes_of(node, id)?, sorted(expected), "{id}");
    Ok(())
}

fn acme(phase: &str) -> Term {
    map(&[("tenant", text("acme")), ("phase", text(phase))])
}

#[test]
fn attributes_are_stored_and_read_back_across_a_restart() -> TestResult {
    // Elixir: "create stores attributes in durable Flow state and codec
    // round trips them" (the codec round trip is the node's own: a reopen).
    let path = directory("audit-attrs-create")?;
    let attributes = map(&[
        ("tenant", text("acme")),
        ("priority_bucket", int(7)),
        ("sandbox", Term::Bool(true)),
        ("tags", texts(&["iot", "fanout"])),
    ]);
    {
        let node = open(&path, 4)?;
        create(&node, "c", "attrs", Some(attributes.clone()), 1_000)?;
        assert_attributes(&node, "c", &attributes)?;
    }
    let node = open(&path, 4)?;
    assert_attributes(&node, "c", &attributes)?;
    Ok(())
}

#[test]
fn a_transition_merges_and_deletes_attributes() -> TestResult {
    // Elixir: "transition merges and deletes attributes without touching
    // payload refs".
    let node = open(&directory("audit-attrs-transition")?, 4)?;
    let start = map(&[
        ("tenant", text("acme")),
        ("phase", text("created")),
        ("drop_me", text("yes")),
    ]);
    create(&node, "t", "attrs", Some(start), 1_000)?;
    let claim = claim_one(&node, "attrs", "queued", 1_001)?;
    ok(
        &node,
        "FLOW.TRANSITION",
        &plus(
            lease(&claim),
            &[
                ("from_state", text("running")),
                ("to_state", text("processing")),
                (
                    "attributes_merge",
                    map(&[("phase", text("processing")), ("region", text("us"))]),
                ),
                ("attributes_delete", texts(&["drop_me"])),
                ("run_at_ms", int(1_010)),
                ("now_ms", int(1_002)),
            ],
        ),
    )?;
    assert_attributes(
        &node,
        "t",
        &map(&[
            ("tenant", text("acme")),
            ("phase", text("processing")),
            ("region", text("us")),
        ]),
    )?;
    Ok(())
}

#[test]
fn claims_return_attributes_in_records_and_compact_attribute_jobs_only() -> TestResult {
    // Elixir: "claim_due compact job return can include attributes" and
    // "claim_due records include attributes by default and compact mode
    // omits them".
    let node = open(&directory("audit-attrs-claim")?, 4)?;
    let compact_claim = |flow_type: &str, mode: &str, now: i64| {
        claim(
            &node,
            "FLOW.CLAIM_DUE",
            &in_p(&[
                ("type", text(flow_type)),
                ("states", texts(&["queued"])),
                ("worker", text("w1")),
                ("limit", int(1)),
                ("now_ms", int(now)),
                ("return", text(mode)),
            ]),
        )
    };
    create(&node, "ca", "attrs-claim", Some(acme("queued")), 1_000)?;
    let Term::List(jobs) = compact_claim("attrs-claim", "jobs_compact_attrs", 1_001)?? else {
        return Err("jobs are not a list".into());
    };
    let [Term::List(job)] = jobs.as_slice() else {
        return Err(format!("one job expected, got {jobs:?}").into());
    };
    let [id, partition, token, fence, attributes] = job.as_slice() else {
        return Err(format!("five job fields expected, got {job:?}").into());
    };
    assert_eq!((id, partition, fence), (&text("ca"), &text(P), &int(1)));
    assert!(matches!(token, Term::Bin(_)));
    assert_eq!(sorted(attributes), sorted(&acme("queued")));

    create(
        &node,
        "record",
        "attrs-default",
        Some(acme("queued")),
        1_000,
    )?;
    let record = claim_one(&node, "attrs-default", "queued", 1_001)?;
    assert_eq!(field(&record, "id"), text("record"));
    assert_eq!(
        sorted(&field(&record, "attributes")),
        sorted(&acme("queued"))
    );
    create(
        &node,
        "compact",
        "attrs-default",
        Some(acme("compact")),
        1_010,
    )?;
    let Term::List(jobs) = compact_claim("attrs-default", "jobs_compact", 1_011)?? else {
        return Err("jobs are not a list".into());
    };
    let [Term::List(job)] = jobs.as_slice() else {
        return Err(format!("one job expected, got {jobs:?}").into());
    };
    let [id, partition, token, fence] = job.as_slice() else {
        return Err(format!("four job fields expected, got {job:?}").into());
    };
    assert_eq!(
        (id, partition, fence),
        (&text("compact"), &text(P), &int(1))
    );
    assert!(matches!(token, Term::Bin(_)));
    Ok(())
}

#[test]
fn attributes_persist_through_retry_transition_complete_fail_and_rewind() -> TestResult {
    // Elixir: "attributes persist through transition complete fail retry
    // and rewind".
    let node = open(&directory("audit-attrs-lifecycle")?, 4)?;
    let kind = "attrs-lifecycle";
    create(&node, "retry", kind, Some(acme("created")), 1_000)?;
    let claim = claim_one(&node, kind, "queued", 1_001)?;
    let retry = [
        ("error", text("temporary")),
        ("run_at_ms", int(1_010)),
        ("now_ms", int(1_002)),
    ];
    ok(&node, "FLOW.RETRY", &plus(lease(&claim), &retry))?;
    assert_attributes(&node, "retry", &acme("created"))?;
    let claim = claim_one(&node, kind, "queued", 1_011)?;
    let merged = map(&[("phase", text("processing")), ("attempt", int(2))]);
    let moved = [
        ("from_state", text("running")),
        ("to_state", text("processing")),
        ("attributes_merge", merged),
        ("run_at_ms", int(1_020)),
        ("now_ms", int(1_012)),
    ];
    ok(&node, "FLOW.TRANSITION", &plus(lease(&claim), &moved))?;
    let processing = map(&[
        ("tenant", text("acme")),
        ("phase", text("processing")),
        ("attempt", int(2)),
    ]);
    assert_attributes(&node, "retry", &processing)?;
    let claim = claim_one(&node, kind, "processing", 1_021)?;
    ok(
        &node,
        "FLOW.COMPLETE",
        &plus(lease(&claim), &[("now_ms", int(1_022))]),
    )?;
    assert_attributes(&node, "retry", &processing)?;

    create(&node, "fail", kind, Some(acme("fail-path")), 2_000)?;
    let claim = claim_one(&node, kind, "queued", 2_001)?;
    let fail = [("error", text("fatal")), ("now_ms", int(2_002))];
    ok(&node, "FLOW.FAIL", &plus(lease(&claim), &fail))?;
    assert_attributes(&node, "fail", &acme("fail-path"))?;

    create(&node, "rewind", kind, Some(acme("created")), 3_000)?;
    let Term::List(entries) = ok(
        &node,
        "FLOW.HISTORY",
        &in_p(&[("id", text("rewind")), ("count", int(10))]),
    )?
    else {
        return Err("history is not a list".into());
    };
    let created_event = match entries.first() {
        Some(Term::List(parts)) => parts.first().cloned().ok_or("no event id")?,
        other => return Err(format!("no created event: {other:?}").into()),
    };
    let claim = claim_one(&node, kind, "queued", 3_001)?;
    let moved = [
        ("from_state", text("running")),
        ("to_state", text("processing")),
        ("attributes_merge", map(&[("phase", text("processing"))])),
        ("run_at_ms", int(3_010)),
        ("now_ms", int(3_002)),
    ];
    ok(&node, "FLOW.TRANSITION", &plus(lease(&claim), &moved))?;
    assert_attributes(&node, "rewind", &acme("processing"))?;
    let rewind = in_p(&[
        ("id", text("rewind")),
        ("to_event", created_event),
        ("now_ms", int(3_003)),
    ]);
    ok(&node, "FLOW.REWIND", &rewind)?;
    assert_attributes(&node, "rewind", &acme("created"))?;
    Ok(())
}

fn numbered(count: usize) -> Term {
    Term::Map(
        (1..=count)
            .map(|index| (format!("k{index}").into_bytes(), text("v")))
            .collect(),
    )
}

#[test]
fn attribute_limits_refuse_at_the_command_and_leave_the_flow_alone() -> TestResult {
    // Elixir: "rejects oversized attributes at command boundary", "rejects
    // an attribute merge that would overflow the existing bounded record"
    // and "rejects oversized attribute keys values and invalid types".
    let node = open(&directory("audit-attrs-limits")?, 4)?;
    let create_with = |id: &str, attributes: Term| {
        run(
            &node,
            "FLOW.CREATE",
            &in_p(&[
                ("id", text(id)),
                ("type", text("attrs")),
                ("attributes", attributes),
                ("run_at_ms", int(1_000)),
                ("now_ms", int(1_000)),
            ]),
        )
    };
    assert_eq!(
        refusal(create_with("many", numbered(17))?)?,
        "ERR too many flow attributes"
    );
    let long_key = Term::Map(vec![("k".repeat(65).into_bytes(), text("v"))]);
    assert_eq!(
        refusal(create_with("key", long_key)?)?,
        "ERR flow attribute key too large"
    );
    let long_value = map(&[("k", text(&"v".repeat(257)))]);
    assert_eq!(
        refusal(create_with("value", long_value)?)?,
        "ERR flow attribute value too large"
    );
    let nested = map(&[("k", map(&[("nested", text("no"))]))]);
    assert_eq!(
        refusal(create_with("nested", nested)?)?,
        "ERR flow attribute value must be scalar or string list"
    );

    create(&node, "full", "attrs-full", Some(numbered(16)), 1_000)?;
    let claim = claim_one(&node, "attrs-full", "queued", 1_001)?;
    let overflow = plus(
        lease(&claim),
        &[
            ("from_state", text("running")),
            ("to_state", text("processing")),
            ("attributes_merge", map(&[("overflow", text("v"))])),
            ("run_at_ms", int(1_010)),
            ("now_ms", int(1_002)),
        ],
    );
    assert_eq!(
        refusal(run(&node, "FLOW.TRANSITION", &overflow)?)?,
        "ERR too many flow attributes"
    );
    let record = get(&node, "full", &in_p(&[]))?;
    assert_eq!(field(&record, "state"), text("running"));
    assert_eq!(field(&record, "version"), field(&claim, "version"));
    assert_eq!(sorted(&field(&record, "attributes")), sorted(&numbered(16)));
    Ok(())
}

#[test]
fn signed_64_bit_attribute_bounds_are_accepted() -> TestResult {
    // Elixir: "rejects attribute numbers that cannot be represented by query
    // and native codecs". The protocol's integers are signed 64-bit, so a
    // larger one cannot reach the node; both bounds are stored.
    let node = open(&directory("audit-attrs-bounds")?, 4)?;
    let bounds = map(&[("minimum", int(i64::MIN)), ("maximum", int(i64::MAX))]);
    create(&node, "bounds", "attrs", Some(bounds.clone()), 1_000)?;
    assert_attributes(&node, "bounds", &bounds)?;
    Ok(())
}

#[test]
fn a_policy_indexes_at_most_three_attribute_names() -> TestResult {
    // Elixir: "flow policy limits indexed attributes to three names".
    let node = open(&directory("audit-attrs-policy")?, 4)?;
    let set = |flow_type: &str, names: &[&str]| {
        run(
            &node,
            "FLOW.POLICY.SET",
            &[
                ("type", text(flow_type)),
                ("indexed_attributes", texts(names)),
            ],
        )
    };
    let policy = set("indexed", &["tenant", "region", "campaign", "tenant"])??;
    let expected = texts(&["tenant", "region", "campaign"]);
    assert_eq!(field(&policy, "indexed_attributes"), expected);
    let state_policy = ok(
        &node,
        "FLOW.POLICY.GET",
        &[("type", text("indexed")), ("state", text("queued"))],
    )?;
    assert_eq!(field(&state_policy, "indexed_attributes"), expected);
    assert_eq!(
        refusal(set("too-many", &["a", "b", "c", "d"])?)?,
        "ERR flow indexed_attributes supports at most 3 keys"
    );
    Ok(())
}
