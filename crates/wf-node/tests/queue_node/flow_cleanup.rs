//! `FLOW.RETENTION_CLEANUP` and active-time limits (ADR-014 step 9): a Flow
//! past its `max_active_ms` fails with Elixir's term error and event, then
//! terminal Flows past their retention are removed, both against one limit
//! that a continuation resumes.

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

fn field(term: &Term, key: &str) -> Term {
    term.get(key.as_bytes()).cloned().unwrap_or(Term::Nil)
}

fn cleanup(node: &QueueNode, now: i64, limit: i64) -> TestResult<Term> {
    Ok(run(
        node,
        "FLOW.RETENTION_CLEANUP",
        &[("now_ms", Term::Int(now)), ("limit", Term::Int(limit))],
    )??)
}

fn get(node: &QueueNode, id: &str) -> TestResult<Term> {
    Ok(run(
        node,
        "FLOW.GET",
        &[("id", text(id)), ("partition_key", text("p"))],
    )??)
}

/// Creates `t1` with a 50 ms active limit and claims it.
fn running_with_limit(node: &QueueNode) -> TestResult {
    run(
        node,
        "FLOW.CREATE",
        &[
            ("id", text("t1")),
            ("type", text("tt")),
            ("partition_key", text("p")),
            ("now_ms", Term::Int(1_000)),
            ("run_at_ms", Term::Int(1_000)),
            ("max_active_ms", Term::Int(50)),
        ],
    )??;
    let parsed = flow::claim::parse(
        "FLOW.CLAIM_DUE",
        &options(&[
            ("type", text("tt")),
            ("worker", text("w")),
            ("now_ms", Term::Int(1_010)),
            ("partition_key", text("p")),
        ]),
    )?;
    assert_eq!(call(|reply| node.claim(parsed.request, reply))??.len(), 1);
    Ok(())
}

/// The timed-out Flow: failed, lease cleared, run state kept, and its
/// error the term Elixir stores.
fn assert_timed_out(node: &QueueNode) -> TestResult {
    let record = get(node, "t1")?;
    assert_eq!(field(&record, "state"), text("failed"));
    assert_eq!(field(&record, "run_state"), text("queued"));
    assert_eq!(field(&record, "lease_token"), Term::Nil);
    assert_eq!(field(&record, "version"), Term::Int(3));
    let error = run(
        node,
        "FLOW.VALUE.MGET",
        &[("refs", Term::List(vec![field(&record, "error_ref")]))],
    )??;
    let Term::List(values) = error else {
        return Err("value reply is not a list".into());
    };
    let value = values.first().ok_or("no value")?;
    assert_eq!(field(value, "reason"), text("max_active_ms"));
    assert_eq!(field(value, "max_active_ms"), Term::Int(50));
    let Term::List(events) = run(
        node,
        "FLOW.HISTORY",
        &[("id", text("t1")), ("partition_key", text("p"))],
    )??
    else {
        return Err("history is not a list".into());
    };
    let Some(Term::List(last)) = events.last() else {
        return Err("no history".into());
    };
    let fields = last.get(1).ok_or("no event fields")?;
    assert_eq!(field(fields, "event"), text("failed"));
    assert_eq!(field(fields, "reason"), text("max_active_ms"));
    assert_eq!(field(fields, "max_active_ms"), text("50"));
    Ok(())
}

#[test]
fn a_flow_past_its_active_limit_fails_and_cleanup_removes_expired_flows() -> TestResult {
    let path = directory("flow-cleanup")?;
    {
        let node = open(&path, 4)?;
        running_with_limit(&node)?;
        assert_eq!(
            field(&cleanup(&node, 1_049, 10)?, "active_timeouts"),
            Term::Int(0)
        );
        let first = cleanup(&node, 1_051, 10)?;
        assert_eq!(field(&first, "active_timeouts"), Term::Int(1));
        assert_eq!(field(&first, "more?"), Term::Bool(false));
        assert_timed_out(&node)?;
        for id in ["c1", "c2"] {
            run(
                &node,
                "FLOW.CREATE",
                &[
                    ("id", text(id)),
                    ("type", text("tt")),
                    ("partition_key", text("p")),
                    ("now_ms", Term::Int(1_000)),
                    // Long enough that only the far-future cleanup removes them.
                    ("retention_ttl_ms", Term::Int(3_600_000)),
                ],
            )??;
            run(
                &node,
                "FLOW.CANCEL",
                &[
                    ("id", text(id)),
                    ("partition_key", text("p")),
                    ("fencing_token", Term::Int(0)),
                    ("now_ms", Term::Int(1_001)),
                ],
            )??;
        }
    }
    // The timeout and its term error survive a restart.
    let node = open(&path, 3)?;
    assert_timed_out(&node)?;
    let far = 9_000_000_000_000;
    let limited = cleanup(&node, far, 1)?;
    assert_eq!(field(&limited, "flows"), Term::Int(1));
    assert_eq!(field(&limited, "more?"), Term::Bool(true));
    let rest = run(
        &node,
        "FLOW.RETENTION_CLEANUP",
        &[
            ("now_ms", Term::Int(far)),
            ("limit", Term::Int(10)),
            ("continuation", field(&limited, "continuation")),
        ],
    )??;
    assert_eq!(field(&rest, "flows"), Term::Int(2));
    assert_eq!(field(&rest, "more?"), Term::Bool(false));
    for id in ["t1", "c1", "c2"] {
        assert_eq!(get(&node, id)?, Term::Nil);
    }
    assert_eq!(
        flow::parse(
            "FLOW.RETENTION_CLEANUP",
            &options(&[("limit", Term::Int(0))])
        )
        .err(),
        Some(ServiceError::rejected(
            "ERR flow limit must be a positive integer".to_owned()
        ))
    );
    Ok(())
}
