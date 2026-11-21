//! `FLOW.INFO`, `FLOW.STATS`, `FLOW.ATTRIBUTES` and `FLOW.ATTRIBUTE_VALUES`
//! (ADR-014 step 11), with the business cases of Elixir's
//! `flow_stats_test.exs` and the discovery and broad-index cases of
//! `flow_attributes_test.exs`: state counts per partition or auto bucket,
//! leased Flows, attribute counts that leave out expired terminal Flows,
//! retention cleanup, and state `any` seeing only the attributes a policy
//! indexed when the Flow was written, across a restart.

use std::time::Duration;

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

fn run(
    node: &QueueNode,
    name: &str,
    entries: &[(&str, Term)],
) -> TestResult<Result<Term, ServiceError>> {
    let command = flow::parse(name, &options(entries))?;
    call(|reply| node.execute(command, reply))
}

fn ok(node: &QueueNode, name: &str, entries: &[(&str, Term)]) -> TestResult<Term> {
    Ok(run(node, name, entries)??)
}

fn field(term: &Term, key: &str) -> Term {
    term.get(key.as_bytes()).cloned().unwrap_or(Term::Nil)
}

fn refused(name: &str, entries: &[(&str, Term)]) -> Option<ServiceError> {
    flow::parse(name, &options(entries)).err()
}

fn rejected(text: &str) -> ServiceError {
    ServiceError::rejected(text.to_owned())
}

/// Creates `id` of `flow_type` with `attributes` in partition `p`, due at
/// `1_000`; `extra` adds options or replaces these.
fn create(
    node: &QueueNode,
    (id, flow_type): (&str, &str),
    attributes: &[(&str, Term)],
    extra: &[(&str, Term)],
) -> TestResult {
    let mut entries: Vec<(&str, Term)> = [
        ("id", text(id)),
        ("type", text(flow_type)),
        ("now_ms", Term::Int(1_000)),
        ("run_at_ms", Term::Int(1_000)),
        ("attributes", options(attributes)),
        ("partition_key", text("p")),
    ]
    .into_iter()
    .filter(|(key, _)| !extra.iter().any(|(given, _)| given == key))
    .collect();
    entries.extend(extra.iter().cloned());
    ok(node, "FLOW.CREATE", &entries)?;
    Ok(())
}

/// Claims one Flow of `flow_type` in `p` and completes it at `1_002`.
fn claim_and_complete(node: &QueueNode, flow_type: &str) -> TestResult {
    let parsed = flow::claim::parse(
        "FLOW.CLAIM_DUE",
        &options(&[
            ("type", text(flow_type)),
            ("worker", text("w")),
            ("now_ms", Term::Int(1_001)),
            ("partition_key", text("p")),
            ("limit", Term::Int(1)),
        ]),
    )?;
    let claimed = call(|reply| node.claim(parsed.request, reply))??;
    let job = claimed.first().ok_or("nothing claimed")?;
    ok(
        node,
        "FLOW.COMPLETE",
        &[
            ("id", Term::Bin(job.id.clone())),
            ("partition_key", text("p")),
            ("lease_token", Term::Bin(job.lease_token.clone())),
            ("fencing_token", Term::int(job.fence)),
            ("now_ms", Term::Int(1_002)),
        ],
    )?;
    Ok(())
}

fn info(node: &QueueNode, flow_type: &str, partition: Option<&str>) -> TestResult<Term> {
    let mut entries = vec![("type", text(flow_type))];
    entries.extend(partition.map(|name| ("partition_key", text(name))));
    ok(node, "FLOW.INFO", &entries)
}

fn stats(node: &QueueNode, flow_type: &str, extra: &[(&str, Term)]) -> TestResult<Term> {
    let mut entries = vec![("type", text(flow_type)), ("partition_key", text("p"))];
    entries.extend(extra.iter().cloned());
    Ok(field(&ok(node, "FLOW.STATS", &entries)?, "count"))
}

fn named(count: i64, key: &str, value: Term) -> Term {
    options(&[("count", Term::Int(count)), (key, value)])
}

#[test]
fn info_and_stats_count_states_partitions_and_leases() -> TestResult {
    let path = directory("flow-info")?;
    let node = open(&path, 4)?;
    for id in ["q1", "q2", "q3"] {
        create(
            &node,
            (id, "ti"),
            &[("tenant", text("acme"))],
            &[("retention_ttl_ms", Term::Int(60_000))],
        )?;
    }
    create(&node, ("r1", "ti"), &[], &[("state", text("review"))])?;
    for id in ["u1", "u2"] {
        create(&node, (id, "ti"), &[], &[("partition_key", Term::Nil)])?;
    }
    claim_and_complete(&node, "ti")?;
    // Stats count past any list cap; review is no INFO state.
    let partition = info(&node, "ti", Some("p"))?;
    for (state, count) in [("queued", 2), ("running", 0), ("completed", 1)] {
        assert_eq!(field(&partition, state), Term::Int(count), "{state}");
    }
    assert_eq!(field(&partition, "inflight"), Term::Int(0));
    assert_eq!(field(&partition, "partition_key"), text("p"));
    let auto = info(&node, "ti", None)?;
    assert_eq!(field(&auto, "queued"), Term::Int(2));
    assert_eq!(field(&auto, "partition_key"), Term::Nil);
    assert_eq!(stats(&node, "ti", &[])?, Term::Int(2));
    assert_eq!(
        stats(&node, "ti", &[("state", text("review"))])?,
        Term::Int(1)
    );
    let full = ok(&node, "FLOW.STATS", &[("type", text("ti"))])?;
    assert_eq!(
        full,
        options(&[
            ("attributes", options(&[])),
            ("count", Term::Int(2)),
            ("state", text("queued")),
            ("type", text("ti")),
        ])
    );
    // A leased Flow is running and in flight.
    let parsed = flow::claim::parse(
        "FLOW.CLAIM_DUE",
        &options(&[
            ("type", text("ti")),
            ("worker", text("w")),
            ("now_ms", Term::Int(1_003)),
            ("partition_key", text("p")),
            ("limit", Term::Int(1)),
        ]),
    )?;
    assert_eq!(call(|reply| node.claim(parsed.request, reply))??.len(), 1);
    let leased = info(&node, "ti", Some("p"))?;
    assert_eq!(field(&leased, "running"), Term::Int(1));
    assert_eq!(field(&leased, "inflight"), Term::Int(1));
    // Retention cleanup removes the completed Flow from the counts.
    let completed = ok(
        &node,
        "FLOW.GET",
        &[("id", text("q1")), ("partition_key", text("p"))],
    )?;
    let Term::Int(until) = field(&completed, "terminal_retention_until_ms") else {
        return Err("no retention deadline".into());
    };
    ok(
        &node,
        "FLOW.RETENTION_CLEANUP",
        &[("now_ms", Term::Int(until + 1)), ("limit", Term::Int(10))],
    )?;
    assert_eq!(
        field(&info(&node, "ti", Some("p"))?, "completed"),
        Term::Int(0)
    );
    assert_eq!(
        stats(&node, "ti", &[("state", text("completed"))])?,
        Term::Int(0)
    );
    assert_eq!(
        refused(
            "FLOW.STATS",
            &[("type", text("ti")), ("state", text("any"))]
        ),
        Some(rejected("ERR flow state any requires attributes"))
    );
    assert_eq!(
        refused(
            "FLOW.INFO",
            &[("type", text("ti")), ("partition_key", Term::Int(5))]
        ),
        Some(rejected(
            "ERR flow partition_key must be a non-empty string"
        ))
    );
    assert_eq!(
        refused(
            "FLOW.ATTRIBUTES",
            &[("type", text("ti")), ("count", Term::Int(1_001))]
        ),
        Some(rejected("ERR flow count exceeds maximum 1000"))
    );
    Ok(())
}

#[test]
fn discovery_counts_policy_indexed_values_of_live_flows() -> TestResult {
    let path = directory("flow-discovery")?;
    let node = open(&path, 4)?;
    ok(
        &node,
        "FLOW.POLICY.SET",
        &[
            ("type", text("td")),
            (
                "indexed_attributes",
                Term::List(vec![text("tenant"), text("region")]),
            ),
        ],
    )?;
    let nul = "acme\0north";
    for (id, attributes) in [
        ("d1", vec![("tenant", text("acme")), ("region", text("us"))]),
        ("d2", vec![("tenant", text("acme"))]),
        ("d3", vec![("tenant", text("beta")), ("other", text("x"))]),
        ("d4", vec![("tenant", text(nul))]),
        (
            "d5",
            vec![("tenant", Term::List(vec![text("acme"), text("gamma")]))],
        ),
    ] {
        create(&node, (id, "td"), &attributes, &[])?;
    }
    let names = ok(
        &node,
        "FLOW.ATTRIBUTES",
        &[("type", text("td")), ("partition_key", text("p"))],
    )?;
    // A list attribute counts each of its values.
    assert_eq!(
        names,
        Term::List(vec![
            named(6, "name", text("tenant")),
            named(1, "name", text("region")),
        ])
    );
    let values = |count: i64| {
        ok(
            &node,
            "FLOW.ATTRIBUTE_VALUES",
            &[
                ("type", text("td")),
                ("attribute", text(" tenant ")),
                ("partition_key", text("p")),
                ("count", Term::Int(count)),
            ],
        )
    };
    assert_eq!(
        values(10)?,
        Term::List(vec![
            named(3, "value", text("acme")),
            named(1, "value", text("beta")),
            named(1, "value", text("gamma")),
            named(1, "value", text(nul)),
        ])
    );
    assert_eq!(
        values(2)?,
        Term::List(vec![
            named(3, "value", text("acme")),
            named(1, "value", text("beta")),
        ])
    );
    let unindexed = ok(
        &node,
        "FLOW.ATTRIBUTE_VALUES",
        &[
            ("type", text("td")),
            ("attribute", text("other")),
            ("partition_key", text("p")),
        ],
    )?;
    assert_eq!(unindexed, Term::List(Vec::new()));
    let tenant = |value: Term| ("attributes", options(&[("tenant", value)]));
    assert_eq!(stats(&node, "td", &[tenant(text("acme"))])?, Term::Int(3));
    assert_eq!(
        stats(
            &node,
            "td",
            &[tenant(Term::List(vec![text("acme"), text("gamma")]))]
        )?,
        Term::Int(1)
    );
    assert_eq!(
        stats(
            &node,
            "td",
            &[(
                "attributes",
                options(&[("tenant", text("beta")), ("other", text("x"))])
            )]
        )?,
        Term::Int(1)
    );
    expired_and_cleaned_flows_leave_discovery(&node)
}

/// A terminal Flow leaves attribute counts once its retention ends, and
/// discovery once retention cleanup removes it.
fn expired_and_cleaned_flows_leave_discovery(node: &QueueNode) -> TestResult {
    create(
        node,
        ("e1", "te"),
        &[("tenant", text("acme"))],
        &[("retention_ttl_ms", Term::Int(1))],
    )?;
    claim_and_complete(node, "te")?;
    std::thread::sleep(Duration::from_millis(5));
    let completed_acme = [
        ("state", text("completed")),
        ("attributes", options(&[("tenant", text("acme"))])),
    ];
    assert_eq!(stats(node, "te", &completed_acme)?, Term::Int(0));
    // A completed Flow is discoverable until retention cleanup removes it.
    claim_and_complete(node, "td")?;
    let discover = || {
        ok(
            node,
            "FLOW.ATTRIBUTES",
            &[
                ("type", text("td")),
                ("state", text("completed")),
                ("partition_key", text("p")),
            ],
        )
    };
    assert_eq!(
        discover()?,
        Term::List(vec![
            named(1, "name", text("region")),
            named(1, "name", text("tenant")),
        ])
    );
    ok(
        node,
        "FLOW.RETENTION_CLEANUP",
        &[
            ("now_ms", Term::Int(9_000_000_000_000)),
            ("limit", Term::Int(10)),
        ],
    )?;
    assert_eq!(discover()?, Term::List(Vec::new()));
    Ok(())
}

#[test]
fn state_any_sees_attributes_indexed_when_the_flow_was_written() -> TestResult {
    let path = directory("flow-broad")?;
    let acme = || ("attributes", options(&[("tenant", text("acme"))]));
    let any = || ("state", text("any"));
    {
        let node = open(&path, 4)?;
        create(&node, ("before", "tb"), &[("tenant", text("acme"))], &[])?;
        ok(
            &node,
            "FLOW.POLICY.SET",
            &[
                ("type", text("tb")),
                ("indexed_attributes", Term::List(vec![text("tenant")])),
            ],
        )?;
        create(&node, ("after", "tb"), &[("tenant", text("acme"))], &[])?;
        // One state sees every attribute; state any only indexed ones.
        assert_eq!(stats(&node, "tb", &[acme()])?, Term::Int(2));
        assert_eq!(stats(&node, "tb", &[any(), acme()])?, Term::Int(1));
        // An attribute added by a transition under the policy is indexed.
        create(
            &node,
            ("moved", "tb"),
            &[],
            &[("run_at_ms", Term::Int(500))],
        )?;
        let parsed = flow::claim::parse(
            "FLOW.CLAIM_DUE",
            &options(&[
                ("type", text("tb")),
                ("worker", text("w")),
                ("now_ms", Term::Int(1_001)),
                ("partition_key", text("p")),
                ("limit", Term::Int(1)),
            ]),
        )?;
        let claimed = call(|reply| node.claim(parsed.request, reply))??;
        let job = claimed.first().ok_or("nothing claimed")?;
        assert_eq!(job.id.as_slice(), b"moved");
        ok(
            &node,
            "FLOW.TRANSITION",
            &[
                ("id", text("moved")),
                ("partition_key", text("p")),
                ("from_state", text("running")),
                ("to_state", text("processing")),
                ("lease_token", Term::Bin(job.lease_token.clone())),
                ("fencing_token", Term::int(job.fence)),
                ("attributes_merge", options(&[("tenant", text("acme"))])),
                ("now_ms", Term::Int(1_002)),
            ],
        )?;
        assert_eq!(stats(&node, "tb", &[any(), acme()])?, Term::Int(2));
    }
    // The indexed names each Flow was written under survive a restart, and
    // a read shows them as Elixir's record does.
    let node = open(&path, 3)?;
    assert_eq!(stats(&node, "tb", &[any(), acme()])?, Term::Int(2));
    let indexed = |id: &str| -> TestResult<Term> {
        let record = ok(
            &node,
            "FLOW.GET",
            &[("id", text(id)), ("partition_key", text("p"))],
        )?;
        Ok(field(&record, "indexed_attributes"))
    };
    assert_eq!(indexed("after")?, Term::List(vec![text("tenant")]));
    assert_eq!(indexed("before")?, Term::Nil);
    assert_eq!(
        ok(
            &node,
            "FLOW.ATTRIBUTE_VALUES",
            &[
                ("type", text("tb")),
                ("attribute", text("tenant")),
                any(),
                ("partition_key", text("p"))
            ],
        )?,
        Term::List(vec![named(2, "value", text("acme"))])
    );
    // A filter on an attribute no Flow indexed counts nothing across states.
    assert_eq!(
        stats(
            &node,
            "tb",
            &[
                any(),
                (
                    "attributes",
                    options(&[("tenant", text("acme")), ("x", text("y"))])
                )
            ]
        )?,
        Term::Int(0)
    );
    Ok(())
}
