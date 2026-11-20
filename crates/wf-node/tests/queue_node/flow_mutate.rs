//! Lifecycle mutations and their batches (ADR-014 step 3): the domain
//! decides each change, the record's other fields follow Elixir, and a batch
//! that is not independent changes nothing unless every item succeeds.

use wf_application::commands::flow::{self, FlowService, Term};
use wf_application::commands::queue::ServiceError;
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

fn field(record: &Term, key: &str) -> Term {
    record.get(key.as_bytes()).cloned().unwrap_or(Term::Nil)
}

/// Creates Flows in partition `p` and claims them all at 1001 by `w1`.
fn claimed(node: &QueueNode, ids: &[&str]) -> TestResult {
    for id in ids {
        run(
            node,
            "FLOW.CREATE",
            &[
                ("id", text(id)),
                ("type", text("t")),
                ("now_ms", Term::Int(1_000)),
                ("run_at_ms", Term::Int(999)),
                ("partition_key", text("p")),
                ("payload", text("pay")),
            ],
        )??;
    }
    let claim = flow::claim::parse(
        "FLOW.CLAIM_DUE",
        &options(&[
            ("type", text("t")),
            ("worker", text("w1")),
            ("now_ms", Term::Int(1_001)),
            ("limit", Term::Int(10)),
            ("partition_key", text("p")),
        ]),
    )?;
    let claimed = call(|reply| {
        wf_application::commands::queue::QueueService::claim(node, claim.request, reply);
    })??;
    assert_eq!(claimed.len(), ids.len());
    Ok(())
}

fn get(node: &QueueNode, id: &str) -> TestResult<Term> {
    Ok(run(
        node,
        "FLOW.GET",
        &[("id", text(id)), ("partition_key", text("p"))],
    )??)
}

fn guard(id: &str, token: &str, fence: i64, now: i64) -> Vec<(&'static str, Term)> {
    vec![
        ("id", text(id)),
        ("partition_key", text("p")),
        ("lease_token", text(token)),
        ("fencing_token", Term::Int(fence)),
        ("now_ms", Term::Int(now)),
    ]
}

#[test]
fn a_flow_moves_through_every_lifecycle_mutation() -> TestResult {
    let path = directory("flow-mutate")?;
    let node = open(&path, 4)?;
    claimed(&node, &["a"])?;
    let mut transition = guard("a", "w1:1001:1", 1, 1_002);
    transition.extend([
        ("from_state", text("running")),
        ("to_state", text("waiting")),
        ("run_at_ms", Term::Int(1_002)),
        ("attributes_merge", options(&[("k", text("v"))])),
        ("priority", Term::Int(1)),
    ]);
    assert_eq!(run(&node, "FLOW.TRANSITION", &transition)??, text("OK"));
    let record = get(&node, "a")?;
    assert_eq!(field(&record, "state"), text("waiting"));
    assert_eq!(field(&record, "lease_token"), Term::Nil);
    assert_eq!(field(&record, "priority"), Term::Int(1));
    assert_eq!(field(&record, "attributes"), options(&[("k", text("v"))]));

    let mut extend = guard("a", "w1:1001:1", 1, 1_003);
    extend.push(("lease_ms", Term::Int(60_000)));
    // The lease was released by the transition.
    assert_eq!(
        run(&node, "FLOW.EXTEND_LEASE", &extend)?,
        Err(ServiceError::Domain(
            wf_domain::value::DomainError::StaleLease
        ))
    );

    let mut cancel = guard("a", "w1:1001:1", 1, 1_100);
    cancel.retain(|(key, _)| *key != "lease_token");
    cancel.push(("reason", text("stop")));
    assert_eq!(run(&node, "FLOW.CANCEL", &cancel)??, text("OK"));
    let record = get(&node, "a")?;
    assert_eq!(field(&record, "state"), text("cancelled"));
    // The reason is stored as the error, referenced at the version that wrote it.
    assert_eq!(field(&record, "version"), Term::Int(4));
    assert!(matches!(field(&record, "error_ref"), Term::Bin(name) if name.ends_with(b":v:e:a:4")));
    assert!(matches!(
        field(&record, "terminal_retention_until_ms"),
        Term::Int(_)
    ));
    Ok(())
}

#[test]
fn step_continue_and_retry_keep_elixir_tokens_and_timing() -> TestResult {
    let path = directory("flow-step")?;
    let node = open(&path, 4)?;
    claimed(&node, &["a"])?;
    let mut step = guard("a", "w1:1001:1", 1, 1_006);
    step.extend([("from_state", text("queued")), ("to_state", text("step2"))]);
    let record = run(&node, "FLOW.STEP_CONTINUE", &step)??;
    assert_eq!(field(&record, "lease_token"), text("w1:1006:2"));
    assert_eq!(field(&record, "fencing_token"), Term::Int(2));
    assert_eq!(field(&record, "run_state"), text("step2"));
    let mut retry = guard("a", "w1:1006:2", 2, 1_007);
    retry.extend([("run_at_ms", Term::Int(5_000)), ("error", text("boom"))]);
    assert_eq!(run(&node, "FLOW.RETRY", &retry)??, text("OK"));
    let record = get(&node, "a")?;
    assert_eq!(field(&record, "state"), text("step2"));
    assert_eq!(field(&record, "attempts"), Term::Int(1));
    assert_eq!(field(&record, "next_run_at_ms"), Term::Int(5_000));
    Ok(())
}

#[test]
fn a_dependent_batch_is_all_or_nothing_and_an_independent_one_is_not() -> TestResult {
    let path = directory("flow-many")?;
    let node = open(&path, 4)?;
    claimed(&node, &["a", "b"])?;
    let item = |id: &str, token: &str, fence: i64| {
        Term::List(vec![text(id), text(token), Term::Int(fence)])
    };
    let batch = |independent: bool| {
        vec![
            ("partition_key", text("p")),
            ("now_ms", Term::Int(1_002)),
            ("run_at_ms", Term::Int(5_000)),
            ("independent", Term::Bool(independent)),
            (
                "items",
                Term::List(vec![item("a", "w1:1001:1", 1), item("b", "w1:1:9", 9)]),
            ),
        ]
    };
    assert_eq!(
        run(&node, "FLOW.RETRY_MANY", &batch(false))?,
        Err(ServiceError::Domain(
            wf_domain::value::DomainError::StaleLease
        ))
    );
    assert_eq!(field(&get(&node, "a")?, "state"), text("running"));
    assert_eq!(
        run(&node, "FLOW.RETRY_MANY", &batch(true))??,
        Term::List(vec![
            text("ok"),
            Term::List(vec![text("error"), text("ERR stale flow lease")]),
        ])
    );
    assert_eq!(field(&get(&node, "a")?, "state"), text("queued"));
    drop(node);
    // The independent batch's change survives a restart.
    let node = open(&path, 2)?;
    assert_eq!(field(&get(&node, "a")?, "attempts"), Term::Int(1));
    Ok(())
}
