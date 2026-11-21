//! Payloads that are not binaries (ADR-014 step 13): Elixir stores any term
//! a create gives, and reads, value reads and scheduled targets return it.

use wf_application::commands::flow::{self, FlowService, Term};
use wf_application::commands::queue::ServiceError;
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

fn node(path: &std::path::Path) -> TestResult<QueueNode> {
    open_with(
        path,
        QueueNodeConfig {
            shards: 4,
            schedule_poll: None,
            ..QueueNodeConfig::default()
        },
    )
}

fn ok(node: &QueueNode, name: &str, entries: Vec<(&str, Term)>) -> TestResult<Term> {
    let command = flow::parse(name, &object(entries))?;
    let result: Result<Term, ServiceError> = call(|reply| node.execute(command, reply))?;
    Ok(result?)
}

fn field(term: &Term, key: &str) -> Term {
    term.get(key.as_bytes()).cloned().unwrap_or(Term::Nil)
}

#[test]
fn term_payloads_round_trip_through_create_and_schedules() -> TestResult {
    let path = directory("flow-term-payload")?;
    let node = node(&path)?;
    let payload = object(vec![("scheduled", Term::Bool(true))]);
    ok(
        &node,
        "FLOW.CREATE",
        vec![
            ("id", text("direct")),
            ("type", text("tt")),
            ("partition_key", text("tp")),
            ("payload", payload.clone()),
            ("now_ms", Term::Int(1_000)),
        ],
    )?;
    let record = ok(
        &node,
        "FLOW.GET",
        vec![
            ("id", text("direct")),
            ("partition_key", text("tp")),
            ("full", Term::Bool(true)),
        ],
    )?;
    assert_eq!(field(&record, "payload"), payload);
    let read = ok(
        &node,
        "FLOW.VALUE.MGET",
        vec![("refs", Term::List(vec![field(&record, "payload_ref")]))],
    )?;
    assert_eq!(read, Term::List(vec![payload.clone()]));
    let mut scheduled = vec![
        ("id", text("t-target")),
        ("type", text("tt")),
        ("partition_key", text("tp")),
    ];
    scheduled.push(("payload", payload.clone()));
    ok(
        &node,
        "FLOW.SCHEDULE.CREATE",
        vec![
            ("id", text("t")),
            ("target", object(scheduled)),
            ("at_ms", Term::Int(1_000)),
            ("now_ms", Term::Int(1_000)),
        ],
    )?;
    ok(
        &node,
        "FLOW.SCHEDULE.FIRE",
        vec![("id", text("t")), ("now_ms", Term::Int(1_000))],
    )?;
    let target = ok(
        &node,
        "FLOW.GET",
        vec![
            ("id", text("t-target")),
            ("partition_key", text("tp")),
            ("full", Term::Bool(true)),
        ],
    )?;
    assert_eq!(field(&target, "payload"), payload);
    Ok(())
}
