//! `FLOW.VALUE.PUT` and `FLOW.VALUE.MGET` (ADR-014 step 5): named values
//! with their own versions, shared values under new references, reads by
//! reference of every kind, and all of it kept across restarts.

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

fn named_put(
    node: &QueueNode,
    value: &str,
    override_value: bool,
    now: i64,
) -> TestResult<Result<Term, ServiceError>> {
    run(
        node,
        "FLOW.VALUE.PUT",
        &[
            ("value", text(value)),
            ("partition_key", text("p")),
            ("owner_flow_id", text("f1")),
            ("name", text("order")),
            ("override", Term::Bool(override_value)),
            ("now_ms", Term::Int(now)),
        ],
    )
}

fn get(node: &QueueNode) -> TestResult<Term> {
    Ok(run(
        node,
        "FLOW.GET",
        &[
            ("id", text("f1")),
            ("partition_key", text("p")),
            ("values", Term::List(vec![text("order")])),
        ],
    )??)
}

fn mget(node: &QueueNode, refs: Vec<Term>, max_bytes: Option<i64>) -> TestResult<Term> {
    let mut entries = vec![("refs", Term::List(refs))];
    entries.extend(max_bytes.map(|max| ("max_bytes", Term::Int(max))));
    Ok(run(node, "FLOW.VALUE.MGET", &entries)??)
}

/// Writes named and shared values, then returns; the test reopens.
fn first_run(path: &std::path::Path) -> TestResult {
    let node = open(path, 4)?;
    run(
        &node,
        "FLOW.CREATE",
        &[
            ("id", text("f1")),
            ("type", text("t")),
            ("now_ms", Term::Int(1_000)),
            ("partition_key", text("p")),
            ("payload", text("pay")),
            ("values", options(&[("order", text("o1"))])),
        ],
    )??;
    // The same bytes change nothing.
    let same = named_put(&node, "o1", false, 2_000)??;
    assert_eq!(field(&same, "stored"), Term::Bool(false));
    assert_eq!(field(&same, "version"), Term::Int(1));
    assert_eq!(field(&get(&node)?, "version"), Term::Int(1));
    // Different bytes need the override.
    assert_eq!(
        named_put(&node, "o2", false, 2_500)?,
        Err(ServiceError::rejected(
            "ERR flow value order already exists with different digest; use OVERRIDE true"
                .to_owned()
        ))
    );
    let replaced = named_put(&node, "o2", true, 3_000)??;
    assert_eq!(field(&replaced, "stored"), Term::Bool(true));
    assert_eq!(field(&replaced, "created"), Term::Bool(false));
    assert_eq!(field(&replaced, "version"), Term::Int(2));
    let record = get(&node)?;
    assert_eq!(field(&record, "version"), Term::Int(2));
    assert_eq!(field(&record, "updated_at_ms"), Term::Int(3_000));
    assert_eq!(field(&record, "values"), options(&[("order", text("o2"))]));
    let shared = run(
        &node,
        "FLOW.VALUE.PUT",
        &[
            ("value", text("shared-bytes")),
            ("partition_key", text("p")),
        ],
    )??;
    assert_eq!(field(&shared, "partition_key"), text("p"));
    let Term::Bin(shared_ref) = field(&shared, "ref") else {
        return Err("no shared ref".into());
    };
    assert!(shared_ref.starts_with(b"f:{f:") && shared_ref.ends_with(b":1"));
    std::fs::write(path.join("shared-ref"), &shared_ref)?;
    Ok(())
}

#[test]
fn named_values_version_on_their_own_and_shared_values_get_new_refs() -> TestResult {
    let path = directory("flow-values")?;
    first_run(&path)?;
    let node = open(&path, 3)?;
    let record = get(&node)?;
    let payload_ref = field(&record, "payload_ref");
    let order_ref = field(&field(&field(&record, "value_refs"), "order"), "ref");
    let shared_ref = Term::Bin(std::fs::read(path.join("shared-ref"))?);
    let missing = text("f:{f:AAAAAAAAAAAAAAAAAAAAAA}:v:p:nope:1");
    assert_eq!(
        mget(
            &node,
            vec![
                payload_ref.clone(),
                order_ref.clone(),
                shared_ref.clone(),
                missing
            ],
            None
        )?,
        Term::List(vec![
            text("pay"),
            text("o2"),
            text("shared-bytes"),
            Term::Nil
        ])
    );
    assert_eq!(
        mget(&node, vec![payload_ref, order_ref.clone()], Some(1))?,
        Term::List(vec![
            options(&[
                (
                    "ref",
                    match mget_ref(&node)? {
                        Some(reference) => reference,
                        None => Term::Nil,
                    }
                ),
                ("value_size", Term::Int(3)),
                ("value_omitted", Term::Bool(true)),
            ]),
            options(&[
                ("ref", order_ref),
                ("value_size", Term::Int(2)),
                ("value_omitted", Term::Bool(true)),
            ]),
        ])
    );
    let history = run(
        &node,
        "FLOW.HISTORY",
        &[("id", text("f1")), ("partition_key", text("p"))],
    )??;
    let Term::List(events) = history else {
        return Err("history is not a list".into());
    };
    let names: Vec<Term> = events
        .iter()
        .filter_map(|event| match event {
            Term::List(parts) => parts.get(1).map(|fields| field(fields, "event")),
            _ => None,
        })
        .collect();
    assert_eq!(names, vec![text("created"), text("value_put")]);
    Ok(())
}

fn mget_ref(node: &QueueNode) -> TestResult<Option<Term>> {
    Ok(Some(field(&get(node)?, "payload_ref")))
}

#[test]
fn a_mutation_value_needs_the_override_to_change_bytes() -> TestResult {
    let path = directory("flow-mutation-values")?;
    let node = open(&path, 4)?;
    run(
        &node,
        "FLOW.CREATE",
        &[
            ("id", text("f1")),
            ("type", text("t")),
            ("now_ms", Term::Int(1_000)),
            ("partition_key", text("p")),
            ("values", options(&[("order", text("o1"))])),
        ],
    )??;
    let cancel = |values: Term, overrides: Vec<Term>| {
        run(
            &node,
            "FLOW.CANCEL",
            &[
                ("id", text("f1")),
                ("partition_key", text("p")),
                ("fencing_token", Term::Int(0)),
                ("now_ms", Term::Int(1_100)),
                ("values", values),
                ("override_values", Term::List(overrides)),
            ],
        )
    };
    assert_eq!(
        cancel(options(&[("order", text("o2"))]), Vec::new())?,
        Err(ServiceError::rejected(
            "ERR flow value order already exists with different digest; use OVERRIDE true"
                .to_owned()
        ))
    );
    cancel(options(&[("order", text("o2"))]), vec![text("order")])??;
    let order = field(&field(&get(&node)?, "value_refs"), "order");
    assert_eq!(field(&order, "version"), Term::Int(2));
    Ok(())
}
