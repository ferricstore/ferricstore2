//! `FLOW.REWIND` (ADR-014 step 10): a Flow returns to the fields a history
//! event recorded, with a new version and fence, no lease, the reason as its
//! error and the event remembered; leased Flows, wrong states and events
//! not held are refused.

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

fn rewind(node: &QueueNode, extra: &[(&str, Term)]) -> TestResult<Result<Term, ServiceError>> {
    let mut entries = vec![
        ("id", text("r1")),
        ("partition_key", text("p")),
        ("now_ms", Term::Int(1_005)),
    ];
    entries.extend(extra.iter().cloned());
    run(node, "FLOW.REWIND", &entries)
}

fn get(node: &QueueNode) -> TestResult<Term> {
    Ok(run(
        node,
        "FLOW.GET",
        &[("id", text("r1")), ("partition_key", text("p"))],
    )??)
}

fn refused(text: &str) -> Result<Term, ServiceError> {
    Err(ServiceError::rejected(text.to_owned()))
}

/// Creates `r1` with attributes and state metadata, claims it, and moves
/// it to `ready` with other attributes; returns the lease while held.
fn created_claimed_moved(node: &QueueNode) -> TestResult {
    run(
        node,
        "FLOW.CREATE",
        &[
            ("id", text("r1")),
            ("type", text("tr")),
            ("partition_key", text("p")),
            ("now_ms", Term::Int(1_000)),
            ("run_at_ms", Term::Int(1_000)),
            ("payload", text("pay")),
            ("attributes", options(&[("a", text("1"))])),
            ("state_meta", options(&[("k", text("v"))])),
        ],
    )??;
    let parsed = flow::claim::parse(
        "FLOW.CLAIM_DUE",
        &options(&[
            ("type", text("tr")),
            ("worker", text("w")),
            ("now_ms", Term::Int(1_001)),
            ("partition_key", text("p")),
        ]),
    )?;
    let claimed = call(|reply| node.claim(parsed.request, reply))??;
    let job = claimed.first().ok_or("nothing claimed")?;
    assert_eq!(
        rewind(node, &[("to_event", text("1000-1"))])?,
        refused("ERR flow cannot rewind leased flow")
    );
    run(
        node,
        "FLOW.TRANSITION",
        &[
            ("id", text("r1")),
            ("partition_key", text("p")),
            ("from_state", text("running")),
            ("to_state", text("ready")),
            ("lease_token", Term::Bin(job.lease_token.clone())),
            ("fencing_token", Term::int(job.fence)),
            ("now_ms", Term::Int(1_003)),
            ("attributes", options(&[("a", text("2")), ("b", text("x"))])),
            ("state_meta", options(&[("k2", text("v2"))])),
        ],
    )??;
    Ok(())
}

#[test]
fn a_rewind_restores_an_event_and_remembers_it() -> TestResult {
    let path = directory("flow-rewind")?;
    {
        let node = open(&path, 4)?;
        created_claimed_moved(&node)?;
        assert_eq!(
            rewind(
                &node,
                &[
                    ("to_event", text("1000-1")),
                    ("expect_state", text("queued"))
                ]
            )?,
            refused("ERR flow wrong state")
        );
        for missing in ["999-1", "1001-2", "x"] {
            assert_eq!(
                rewind(&node, &[("to_event", text(missing))])?,
                refused("ERR flow rewind target event not found")
            );
        }
        rewind(
            &node,
            &[
                ("to_event", text("1000-1")),
                ("expect_state", text("ready")),
                ("reason", text("because")),
            ],
        )??;
    }
    let node = open(&path, 3)?;
    let record = get(&node)?;
    assert_eq!(field(&record, "state"), text("queued"));
    assert_eq!(field(&record, "version"), Term::Int(4));
    assert_eq!(field(&record, "fencing_token"), Term::Int(2));
    assert_eq!(field(&record, "next_run_at_ms"), Term::Int(1_000));
    assert_eq!(field(&record, "run_state"), text("queued"));
    assert_eq!(field(&record, "rewound_to_event_id"), text("1000-1"));
    assert_eq!(field(&field(&record, "attributes"), "a"), text("1"));
    assert_eq!(field(&field(&record, "attributes"), "b"), Term::Nil);
    assert_eq!(field(&field(&record, "state_meta"), "ready"), Term::Nil);
    let error = run(
        &node,
        "FLOW.VALUE.MGET",
        &[("refs", Term::List(vec![field(&record, "error_ref")]))],
    )??;
    assert_eq!(error, Term::List(vec![text("because")]));
    let Term::List(events) = run(
        &node,
        "FLOW.HISTORY",
        &[("id", text("r1")), ("partition_key", text("p"))],
    )??
    else {
        return Err("history is not a list".into());
    };
    let Some(Term::List(last)) = events.last() else {
        return Err("no history".into());
    };
    let fields = last.get(1).ok_or("no event fields")?;
    assert_eq!(field(fields, "event"), text("rewound"));
    assert_eq!(field(fields, "rewound_to_event_id"), text("1000-1"));
    // A later rewind keeps its own reference and the given due time.
    rewind(
        &node,
        &[
            ("to_event", text("1003-3")),
            ("run_at_ms", Term::Int(7_777)),
        ],
    )??;
    let again = get(&node)?;
    assert_eq!(field(&again, "state"), text("ready"));
    assert_eq!(field(&again, "next_run_at_ms"), Term::Int(7_777));
    assert_eq!(field(&again, "error_ref"), Term::Nil);
    assert_eq!(field(&again, "rewound_to_event_id"), text("1003-3"));
    Ok(())
}
