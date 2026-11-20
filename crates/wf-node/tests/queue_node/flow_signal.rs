//! `FLOW.SIGNAL` (ADR-014 step 7): a signal raises the Flow's version, may
//! move it to another state without its lease, writes named values, records
//! a `signaled` event with the signal's name, and an idempotency key makes a
//! repeat with the same content a no-op and different content a conflict.

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

fn signal(
    node: &QueueNode,
    now: i64,
    extra: &[(&str, Term)],
) -> TestResult<Result<Term, ServiceError>> {
    let mut entries = vec![
        ("id", text("g1")),
        ("signal", text("go")),
        ("partition_key", text("p")),
        ("now_ms", Term::Int(now)),
    ];
    entries.extend(extra.iter().cloned());
    run(node, "FLOW.SIGNAL", &entries)
}

fn get(node: &QueueNode) -> TestResult<Term> {
    Ok(run(
        node,
        "FLOW.GET",
        &[("id", text("g1")), ("partition_key", text("p"))],
    )??)
}

#[test]
fn signals_move_flows_and_repeat_only_with_the_same_content() -> TestResult {
    let path = directory("flow-signal")?;
    {
        let node = open(&path, 4)?;
        run(
            &node,
            "FLOW.CREATE",
            &[
                ("id", text("g1")),
                ("type", text("tg")),
                ("now_ms", Term::Int(1_000)),
                ("run_at_ms", Term::Int(999)),
                ("partition_key", text("p")),
            ],
        )??;
        let claim = flow::claim::parse(
            "FLOW.CLAIM_DUE",
            &options(&[
                ("type", text("tg")),
                ("worker", text("w")),
                ("now_ms", Term::Int(1_001)),
                ("partition_key", text("p")),
            ]),
        )?;
        call(|reply| {
            wf_application::commands::queue::QueueService::claim(&node, claim.request, reply);
        })??;
        assert_eq!(signal(&node, 1_002, &[])??, text("OK"));
        assert_eq!(field(&get(&node)?, "version"), Term::Int(3));
        assert_eq!(
            signal(&node, 1_003, &[("transition_to", text("waiting"))])?,
            Err(ServiceError::rejected(
                "ERR flow signal transition requires if_state".to_owned()
            ))
        );
        assert_eq!(
            signal(
                &node,
                1_003,
                &[
                    ("transition_to", text("waiting")),
                    ("if_state", text("queued"))
                ]
            )?,
            Err(ServiceError::rejected("ERR flow state mismatch".to_owned()))
        );
        let moving = [
            ("transition_to", text("waiting")),
            ("if_state", text("running")),
            ("values", options(&[("b", text("2"))])),
            ("idempotency_key", text("k1")),
        ];
        assert_eq!(signal(&node, 1_004, &moving)??, text("OK"));
        // The same content under the same key changes nothing.
        assert_eq!(signal(&node, 1_005, &moving)??, text("OK"));
        assert_eq!(
            signal(&node, 1_006, &[("idempotency_key", text("k1"))])?,
            Err(ServiceError::rejected(
                "ERR flow signal idempotency conflict".to_owned()
            ))
        );
    }
    let node = open(&path, 3)?;
    let record = get(&node)?;
    assert_eq!(field(&record, "state"), text("waiting"));
    assert_eq!(field(&record, "run_state"), text("queued"));
    assert_eq!(field(&record, "lease_token"), Term::Nil);
    assert_eq!(field(&record, "next_run_at_ms"), Term::Int(1_004));
    assert_eq!(field(&record, "version"), Term::Int(4));
    let Term::List(events) = run(
        &node,
        "FLOW.HISTORY",
        &[("id", text("g1")), ("partition_key", text("p"))],
    )??
    else {
        return Err("history is not a list".into());
    };
    let signaled: Vec<(Term, Term)> = events
        .iter()
        .filter_map(|event| match event {
            Term::List(parts) => {
                let fields = parts.get(1)?;
                (field(fields, "event") == text("signaled"))
                    .then(|| (field(fields, "signal"), field(fields, "state")))
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        signaled,
        vec![(text("go"), text("running")), (text("go"), text("waiting"))]
    );
    assert_eq!(
        run(
            &node,
            "FLOW.SIGNAL",
            &[("id", text("nope")), ("signal", text("x"))]
        )?,
        Err(ServiceError::rejected("ERR flow not found".to_owned()))
    );
    Ok(())
}
