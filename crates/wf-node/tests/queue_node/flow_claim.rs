//! Typed-map `FLOW.CLAIM_DUE` and `FLOW.RECLAIM` (ADR-014 step 2): claims
//! decided at the client's clock, Elixir's return shapes, records with the
//! payload, named values, attributes and state metadata.

use wf_application::commands::flow::claim::{self, ClaimReturn};
use wf_application::commands::flow::{self, FlowService, Term};
use wf_application::commands::queue::{QueueService, ServiceError};
use wf_node::composition::QueueNode;

use super::{TestResult, call, directory, open};

fn options(entries: &[(&str, Term)]) -> Term {
    Term::Map(
        entries
            .iter()
            .map(|(key, value)| (key.as_bytes().to_vec(), value.clone()))
            .collect(),
    )
}

fn text(value: &str) -> Term {
    Term::text(value)
}

fn names(values: &[&str]) -> Term {
    Term::List(values.iter().map(|value| text(value)).collect())
}

fn create(node: &QueueNode, id: &str) -> TestResult {
    let command = flow::parse(
        "FLOW.CREATE",
        &options(&[
            ("id", text(id)),
            ("type", text("t1")),
            ("state", text("queued")),
            ("now_ms", Term::Int(1_000)),
            ("run_at_ms", Term::Int(999)),
            ("partition_key", text("pk1")),
            ("payload", text("{\"answer\":42}")),
            ("attributes", options(&[("tenant", text("acme"))])),
            ("state_meta", options(&[("attempt", Term::Int(1))])),
            ("values", options(&[("order", text("{\"n\":1}"))])),
        ]),
    )?;
    call(|reply| node.execute(command, reply))??;
    Ok(())
}

/// Runs a typed claim command and builds its reply as Elixir would.
fn claim_term(
    node: &QueueNode,
    name: &str,
    entries: &[(&str, Term)],
) -> TestResult<Result<Term, ServiceError>> {
    let parsed = claim::parse(name, &options(entries))?;
    let shape = parsed.shape;
    let claimed = call(|reply| node.claim(parsed.request, reply))?;
    Ok(claimed.map(|claimed| claim::reply(shape, claimed)))
}

fn first(reply: &Term) -> Option<&Term> {
    match reply {
        Term::List(items) => items.first(),
        _ => None,
    }
}

fn field<'a>(record: Option<&'a Term>, key: &str) -> Option<&'a Term> {
    record.and_then(|record| record.get(key.as_bytes()))
}

#[test]
fn a_typed_claim_returns_records_at_the_client_clock() -> TestResult {
    let path = directory("flow-claim-records")?;
    let node = open(&path, 4)?;
    create(&node, "f1")?;
    let reply = claim_term(
        &node,
        "FLOW.CLAIM_DUE",
        &[
            ("type", text("t1")),
            ("state", text("queued")),
            ("worker", text("w1")),
            ("lease_ms", Term::Int(30_000)),
            ("limit", Term::Int(1)),
            ("now_ms", Term::Int(1_001)),
            ("partition_key", text("pk1")),
            ("return", text("records")),
            ("payload", Term::Bool(true)),
            ("values", names(&["order", "missing"])),
        ],
    )??;
    let record = first(&reply);
    assert_eq!(field(record, "state"), Some(&text("running")));
    assert_eq!(field(record, "run_state"), Some(&text("queued")));
    assert_eq!(field(record, "lease_token"), Some(&text("w1:1001:1")));
    assert_eq!(field(record, "fencing_token"), Some(&Term::Int(1)));
    assert_eq!(field(record, "lease_deadline_ms"), Some(&Term::Int(31_001)));
    assert_eq!(field(record, "next_run_at_ms"), Some(&Term::Int(31_001)));
    assert_eq!(field(record, "updated_at_ms"), Some(&Term::Int(1_001)));
    assert_eq!(field(record, "payload"), Some(&text("{\"answer\":42}")));
    assert_eq!(field(record, "payload_size"), Some(&Term::Int(13)));
    assert_eq!(
        field(record, "values"),
        Some(&options(&[("order", text("{\"n\":1}"))]))
    );
    assert_eq!(
        field(record, "attributes"),
        Some(&options(&[("tenant", text("acme"))]))
    );
    assert_eq!(
        field(record, "state_meta"),
        Some(&options(&[(
            "queued",
            options(&[("attempt", Term::Int(1))])
        )]))
    );
    Ok(())
}

#[test]
fn reclaim_takes_an_expired_lease_with_a_new_fence() -> TestResult {
    let path = directory("flow-claim-reclaim")?;
    let node = open(&path, 4)?;
    create(&node, "f1")?;
    let claim_at = |now: i64, worker: &str| {
        vec![
            ("type", text("t1")),
            ("worker", text(worker)),
            ("lease_ms", Term::Int(30_000)),
            ("now_ms", Term::Int(now)),
            ("partition_key", text("pk1")),
        ]
    };
    claim_term(&node, "FLOW.CLAIM_DUE", &claim_at(1_001, "w1"))??;
    // Before the lease ends there is nothing to reclaim.
    let early = claim_term(&node, "FLOW.RECLAIM", &claim_at(31_000, "w2"))??;
    assert_eq!(early, Term::List(Vec::new()));
    let reply = claim_term(&node, "FLOW.RECLAIM", &claim_at(31_003, "w2"))??;
    let record = first(&reply);
    assert_eq!(field(record, "lease_token"), Some(&text("w2:31003:2")));
    assert_eq!(field(record, "fencing_token"), Some(&Term::Int(2)));
    assert_eq!(field(record, "lease_owner"), Some(&text("w2")));
    // Records carry the payload by default.
    assert_eq!(field(record, "payload"), Some(&text("{\"answer\":42}")));
    Ok(())
}

#[test]
fn claim_return_shapes_follow_elixir() -> TestResult {
    let path = directory("flow-claim-shapes")?;
    let node = open(&path, 4)?;
    let shapes = [
        ("jobs", ClaimReturn::Jobs),
        ("JOBS_COMPACT", ClaimReturn::JobsCompact),
        ("jobs_compact_attrs", ClaimReturn::JobsCompactAttrs),
        ("jobs_compact_state", ClaimReturn::JobsCompactState),
        (
            "jobs_compact_state_attrs",
            ClaimReturn::JobsCompactStateAttrs,
        ),
    ];
    let mut replies = Vec::new();
    for (index, (name, shape)) in shapes.iter().enumerate() {
        create(&node, &format!("f{index}"))?;
        let entries = [
            ("type", text("t1")),
            ("worker", text("w1")),
            ("now_ms", Term::Int(1_001)),
            ("partition_key", text("pk1")),
            ("return", text(name)),
        ];
        assert_eq!(
            claim::parse("FLOW.CLAIM_DUE", &options(&entries))?.shape,
            *shape
        );
        replies.push(claim_term(&node, "FLOW.CLAIM_DUE", &entries)??);
    }
    let job = |reply: &Term| first(reply).cloned();
    assert_eq!(
        job(&replies[0]),
        Some(options(&[
            ("id", text("f0")),
            ("type", text("t1")),
            ("state", text("running")),
            ("run_state", text("queued")),
            ("partition_key", text("pk1")),
            ("lease_token", text("w1:1001:1")),
            ("fencing_token", Term::Int(1)),
        ]))
    );
    let compact = |id: &str, extra: Vec<Term>| {
        let mut items = vec![text(id), text("pk1"), text("w1:1001:1"), Term::Int(1)];
        items.extend(extra);
        Some(Term::List(items))
    };
    let attrs = options(&[("tenant", text("acme"))]);
    assert_eq!(job(&replies[1]), compact("f1", vec![]));
    assert_eq!(job(&replies[2]), compact("f2", vec![attrs.clone()]));
    assert_eq!(job(&replies[3]), compact("f3", vec![text("queued")]));
    assert_eq!(job(&replies[4]), compact("f4", vec![text("queued"), attrs]));
    Ok(())
}

#[test]
fn invalid_claim_options_are_refused_with_elixir_text() {
    let cases: [(&[(&str, Term)], &str); 6] = [
        (&[("type", text("t"))], "ERR flow worker is required"),
        (
            &[
                ("type", text("t")),
                ("worker", text("w")),
                ("limit", Term::Int(0)),
            ],
            "ERR flow limit must be a positive integer",
        ),
        (
            &[
                ("type", text("t")),
                ("worker", text("w")),
                ("lease_ms", Term::Int(0)),
            ],
            "ERR flow lease_ms must be a positive integer",
        ),
        (
            &[
                ("type", text("t")),
                ("worker", text("w")),
                ("return", text("bogus")),
            ],
            "ERR flow claim return must be records, jobs, or compact jobs",
        ),
        (
            &[
                ("type", text("t")),
                ("worker", text("w")),
                ("reclaim_ratio", Term::Int(101)),
            ],
            "ERR flow reclaim_ratio must be an integer between 0 and 100",
        ),
        (
            &[
                ("type", text("t")),
                ("worker", text("w")),
                ("state", text("a")),
                ("states", names(&["b"])),
            ],
            "ERR flow state and states are mutually exclusive",
        ),
    ];
    for (entries, expected) in cases {
        assert_eq!(
            claim::parse("FLOW.CLAIM_DUE", &options(entries)).err(),
            Some(ServiceError::rejected(expected.to_owned())),
            "{expected}"
        );
    }
}

#[test]
fn a_claim_without_a_partition_takes_only_auto_partitions() -> TestResult {
    let path = directory("flow-claim-scope")?;
    let node = open(&path, 4)?;
    create(&node, "named")?;
    let auto = flow::parse(
        "FLOW.CREATE",
        &options(&[
            ("id", text("auto")),
            ("type", text("t1")),
            ("now_ms", Term::Int(1_000)),
        ]),
    )?;
    call(|reply| node.execute(auto, reply))??;
    let claim_with = |partition: Option<&str>| {
        let mut entries = vec![
            ("type", text("t1")),
            ("worker", text("w1")),
            ("limit", Term::Int(5)),
            ("now_ms", Term::Int(2_000)),
            ("return", text("jobs_compact")),
        ];
        entries.extend(partition.map(|name| ("partition_key", text(name))));
        claim_term(&node, "FLOW.CLAIM_DUE", &entries)
    };
    let ids = |reply: &Term| -> Vec<Term> {
        match reply {
            Term::List(jobs) => jobs
                .iter()
                .filter_map(|job| match job {
                    Term::List(items) => items.first().cloned(),
                    _ => None,
                })
                .collect(),
            _ => Vec::new(),
        }
    };
    assert_eq!(ids(&claim_with(None)??), vec![text("auto")]);
    assert_eq!(ids(&claim_with(Some("GLOBAL"))??), Vec::<Term>::new());
    assert_eq!(ids(&claim_with(Some("any"))??), vec![text("named")]);
    Ok(())
}
