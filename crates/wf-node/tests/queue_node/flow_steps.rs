//! `FLOW.START_AND_CLAIM` and `FLOW.RUN_STEPS_MANY` (ADR-014 step 6), with
//! the records and history Elixir writes for them.

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

fn events(node: &QueueNode, id: &str) -> TestResult<Vec<(Term, Term, Term)>> {
    let Term::List(events) = run(
        node,
        "FLOW.HISTORY",
        &[("id", text(id)), ("partition_key", text("p"))],
    )??
    else {
        return Err("history is not a list".into());
    };
    Ok(events
        .iter()
        .filter_map(|event| match event {
            Term::List(parts) => {
                let fields = parts.get(1)?;
                Some((
                    parts.first()?.clone(),
                    field(fields, "event"),
                    field(fields, "next_run_at_ms"),
                ))
            }
            _ => None,
        })
        .collect())
}

#[test]
fn start_and_claim_creates_a_claimed_flow() -> TestResult {
    let path = directory("flow-start")?;
    let node = open(&path, 4)?;
    let start = [
        ("id", text("s1")),
        ("type", text("ts")),
        ("initial_state", text("step1")),
        ("worker", text("w")),
        ("lease_ms", Term::Int(1_000)),
        ("now_ms", Term::Int(100)),
        ("partition_key", text("p")),
        ("payload", text("pp")),
    ];
    let record = run(&node, "FLOW.START_AND_CLAIM", &start)??;
    assert_eq!(field(&record, "state"), text("running"));
    assert_eq!(field(&record, "run_state"), text("step1"));
    assert_eq!(field(&record, "lease_token"), text("w:100:1"));
    assert_eq!(field(&record, "attempts"), Term::Int(1));
    assert_eq!(field(&record, "lease_deadline_ms"), Term::Int(1_100));
    // Elixir's start-and-claim reply always names these two.
    assert_eq!(record.get(b"max_active_ms"), Some(&Term::Nil));
    assert_eq!(record.get(b"value_refs"), Some(&Term::Map(Vec::new())));
    assert_eq!(
        run(&node, "FLOW.START_AND_CLAIM", &start)?,
        Err(ServiceError::rejected("ERR flow already exists".to_owned()))
    );
    assert_eq!(
        events(&node, "s1")?,
        vec![(text("100-1"), text("created"), text("1100"))]
    );
    Ok(())
}

#[test]
fn run_steps_walks_each_flow_through_its_states_and_completes_it() -> TestResult {
    let path = directory("flow-run-steps")?;
    {
        let node = open(&path, 4)?;
        let item = |id: &str| options(&[("id", text(id)), ("partition_key", text("p"))]);
        let steps = |items: Vec<Term>| {
            vec![
                ("type", text("tr")),
                ("steps", Term::Int(3)),
                ("worker", text("w")),
                ("lease_ms", Term::Int(1_000)),
                ("now_ms", Term::Int(200)),
                ("result", text("done")),
                ("items", Term::List(items)),
            ]
        };
        assert_eq!(
            run(
                &node,
                "FLOW.RUN_STEPS_MANY",
                &steps(vec![item("r1"), item("r2")])
            )??,
            text("OK")
        );
        assert_eq!(
            run(&node, "FLOW.RUN_STEPS_MANY", &steps(vec![item("r1")]))??,
            Term::List(vec![Term::List(vec![
                text("error"),
                text("ERR flow already exists")
            ])])
        );
    }
    let node = open(&path, 2)?;
    let record = run(
        &node,
        "FLOW.GET",
        &[("id", text("r2")), ("partition_key", text("p"))],
    )??;
    assert_eq!(field(&record, "state"), text("completed"));
    assert_eq!(field(&record, "version"), Term::Int(4));
    assert_eq!(field(&record, "fencing_token"), Term::Int(3));
    assert_eq!(field(&record, "run_state"), text("step_3"));
    assert_eq!(field(&record, "updated_at_ms"), Term::Int(203));
    assert!(
        matches!(field(&record, "result_ref"), Term::Bin(name) if name.ends_with(b":v:r:r2:4"))
    );
    // The chain keeps the first lease deadline, as Elixir's does.
    assert_eq!(
        events(&node, "r2")?,
        vec![
            (text("200-1"), text("created"), text("1200")),
            (text("201-2"), text("step_continued"), text("1200")),
            (text("202-3"), text("step_continued"), text("1200")),
            (text("203-4"), text("completed"), text("")),
        ]
    );
    Ok(())
}
