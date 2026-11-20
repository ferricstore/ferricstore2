//! `FLOW.CREATE` with every record field and `FLOW.GET` (ADR-014): the
//! reply maps match the Elixir server's, idempotent retries answer as Elixir
//! does, and the fields survive a restart and a snapshot.

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

fn run(
    node: &QueueNode,
    name: &str,
    entries: &[(&str, Term)],
) -> TestResult<Result<Term, ServiceError>> {
    let command = flow::parse(name, &options(entries))?;
    call(|reply| node.execute(command, reply))
}

fn text(value: &str) -> Term {
    Term::text(value)
}

fn field<'a>(record: &'a Term, key: &str) -> Option<&'a Term> {
    record.get(key.as_bytes())
}

/// The create of the SDK's claim-record contract test.
fn full_create(id: &str) -> Vec<(&'static str, Term)> {
    vec![
        ("id", text(id)),
        ("type", text("t1")),
        ("state", text("queued")),
        ("now_ms", Term::Int(1_000)),
        ("partition_key", text("pk1")),
        ("payload", text("hello")),
        ("correlation_id", text("c1")),
        ("priority", Term::Int(2)),
        (
            "attributes",
            options(&[("tenant", text("acme")), ("case", text("f02"))]),
        ),
        ("state_meta", options(&[("attempt", Term::Int(1))])),
        ("values", options(&[("order", text("{\"n\":1}"))])),
        ("idempotent", Term::Bool(true)),
    ]
}

fn get(node: &QueueNode, id: &str, values: &[&str]) -> TestResult<Term> {
    let names = Term::List(values.iter().map(|name| text(name)).collect());
    Ok(run(
        node,
        "FLOW.GET",
        &[
            ("id", text(id)),
            ("partition_key", text("pk1")),
            ("values", names),
        ],
    )??)
}

fn assert_full_record(record: &Term) {
    assert_eq!(field(record, "state"), Some(&text("queued")));
    assert_eq!(field(record, "priority"), Some(&Term::Int(2)));
    assert_eq!(field(record, "partition_key"), Some(&text("pk1")));
    assert_eq!(field(record, "correlation_id"), Some(&text("c1")));
    assert_eq!(field(record, "root_flow_id"), Some(&text("f1")));
    assert_eq!(field(record, "created_at_ms"), Some(&Term::Int(1_000)));
    assert_eq!(field(record, "next_run_at_ms"), Some(&Term::Int(1_000)));
    assert_eq!(field(record, "lease_deadline_ms"), Some(&Term::Int(0)));
    assert_eq!(
        field(record, "retention_ttl_ms"),
        Some(&Term::Int(604_800_000))
    );
    assert_eq!(
        field(record, "history_max_events"),
        Some(&Term::Int(100_000))
    );
    assert_eq!(
        field(record, "attributes"),
        Some(&options(&[("case", text("f02")), ("tenant", text("acme"))]))
    );
    assert_eq!(
        field(record, "state_meta"),
        Some(&options(&[(
            "queued",
            options(&[("attempt", Term::Int(1))])
        )]))
    );
    assert_eq!(
        field(record, "values"),
        Some(&options(&[("order", text("{\"n\":1}"))]))
    );
    // The payload reference and named value reference use Elixir's names.
    assert_eq!(
        field(record, "payload_ref"),
        Some(&text("f:{f:bnO_8dlLCnUAFFvoJyLnRQ}:v:p:f1:1"))
    );
    let order = field(record, "value_refs").and_then(|refs| refs.get(b"order"));
    assert_eq!(
        order.and_then(|order| order.get(b"digest")),
        Some(&text(
            "2bfd14f43d17fc7cea24e0917a8879b4b2f880b8baeec1b9d90fbaad655e71bd"
        ))
    );
}

#[test]
fn a_created_flow_reads_back_with_every_field() -> TestResult {
    let path = directory("flow-record")?;
    let node = open(&path, 4)?;
    assert_eq!(run(&node, "FLOW.CREATE", &full_create("f1"))??, text("OK"));
    assert_full_record(&get(&node, "f1", &["order"])?);
    assert_eq!(get(&node, "missing", &[])?, Term::Nil);
    // Without its partition the Flow is looked up in the id's auto bucket.
    assert_eq!(run(&node, "FLOW.GET", &[("id", text("f1"))])??, Term::Nil);
    Ok(())
}

#[test]
fn creates_of_an_existing_flow_answer_as_elixir() -> TestResult {
    let path = directory("flow-idempotent")?;
    let node = open(&path, 4)?;
    run(&node, "FLOW.CREATE", &full_create("f1"))??;
    assert_eq!(run(&node, "FLOW.CREATE", &full_create("f1"))??, text("OK"));
    let mut other_type = full_create("f1");
    other_type.retain(|(key, _)| *key != "type");
    other_type.push(("type", text("t2")));
    assert_eq!(
        run(&node, "FLOW.CREATE", &other_type)?,
        Err(ServiceError::rejected(
            "ERR flow idempotency conflict".to_owned()
        ))
    );
    let mut plain = full_create("f1");
    plain.retain(|(key, _)| *key != "idempotent");
    assert_eq!(
        run(&node, "FLOW.CREATE", &plain)?,
        Err(ServiceError::rejected("ERR flow already exists".to_owned()))
    );
    Ok(())
}

#[test]
fn record_fields_survive_replay_and_snapshots() -> TestResult {
    let path = directory("flow-record-restart")?;
    {
        let node = open(&path, 4)?;
        run(&node, "FLOW.CREATE", &full_create("f1"))??;
    }
    {
        let node = open(&path, 4)?;
        assert_full_record(&get(&node, "f1", &["order"])?);
        node.snapshot()?;
        run(&node, "FLOW.CREATE", &full_create("f2"))??;
    }
    let node = open(&path, 2)?;
    assert_full_record(&get(&node, "f1", &["order"])?);
    assert_eq!(
        field(&get(&node, "f2", &[])?, "attributes"),
        field(&get(&node, "f1", &[])?, "attributes")
    );
    Ok(())
}

#[test]
fn invalid_create_options_are_refused_with_elixir_text() {
    let cases: [(&[(&str, Term)], &str); 7] = [
        (&[("id", text("x"))], "ERR flow type is required"),
        (
            &[
                ("id", text("x")),
                ("type", text("t")),
                ("priority", Term::Int(3)),
            ],
            "ERR flow priority must be between 0 and 2",
        ),
        (
            &[
                ("id", text("x")),
                ("type", text("t")),
                ("state", text("running")),
            ],
            "ERR flow running state is only entered by FLOW.CLAIM_DUE",
        ),
        (
            &[
                ("id", text("x")),
                ("type", text("t")),
                ("now_ms", Term::Int(-1)),
            ],
            "ERR flow now_ms must be a non-negative integer",
        ),
        (
            &[
                ("id", text("x")),
                ("type", text("t")),
                ("attributes", options(&[("__x", Term::Int(1))])),
            ],
            "ERR flow attribute key is reserved",
        ),
        (
            &[
                ("id", text("x")),
                ("type", text("t")),
                ("history_hot_max_events", Term::Int(20)),
                ("history_max_events", Term::Int(10)),
            ],
            "ERR flow history_max_events must be greater than or equal to history_hot_max_events",
        ),
        (
            &[
                ("id", text("x")),
                ("type", text("t")),
                ("max_active_ms", Term::Int(0)),
            ],
            "ERR flow max_active_ms must be between 1 and 31536000000 or infinity",
        ),
    ];
    for (entries, expected) in cases {
        assert_eq!(
            flow::parse("FLOW.CREATE", &options(entries)).err(),
            Some(ServiceError::rejected(expected.to_owned())),
            "{expected}"
        );
    }
}
