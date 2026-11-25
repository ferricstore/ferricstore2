//! FLOW commands as text (ADR-014 step 9c), as the SDK sends them through
//! `COMMAND_EXEC`: numbers as arguments, `STATE_META` pairs, a policy with a
//! type-level retention, a retry with its own limit, and Elixir's refusals.

use wf_application::commands::flow::text::{self, Parsed};
use wf_application::commands::flow::{self, FlowService, Term};
use wf_application::commands::queue::{QueueService, ServiceError};
use wf_node::composition::QueueNode;

use super::{TestResult, call, directory, open};

fn text(value: &str) -> Term {
    Term::text(value)
}

/// Runs text command `name` with `args`.
fn exec(node: &QueueNode, name: &str, args: &[Term]) -> TestResult<Result<Term, ServiceError>> {
    match text::parse(name.as_bytes(), args) {
        Ok(Parsed::Command(command)) => Ok(call(|reply| node.execute(command, reply))?),
        Ok(Parsed::Claim(typed, map)) => {
            let parsed = flow::claim::parse(typed, &map)?;
            let shape = parsed.shape;
            let claimed = call(|reply| node.claim(parsed.request, reply))?;
            Ok(claimed.map(|claimed| flow::claim::reply(shape, claimed)))
        }
        Err(error) => Ok(Err(error)),
    }
}

fn field(term: &Term, path: &[&str]) -> Term {
    path.iter().fold(term.clone(), |value, key| {
        value.get(key.as_bytes()).cloned().unwrap_or(Term::Nil)
    })
}

fn refused(text: &str) -> Result<Term, ServiceError> {
    Err(ServiceError::rejected(text.to_owned()))
}

/// Claims `x1` and returns its lease token and fence.
fn claim_one(node: &QueueNode, now: i64) -> TestResult<(Term, Term)> {
    let claimed = exec(
        node,
        "flow.claim_due",
        &[
            text("tt"),
            text("WORKER"),
            text("w"),
            text("NOW"),
            Term::Int(now),
            text("PARTITION"),
            text("p"),
            text("LIMIT"),
            Term::Int(1),
        ],
    )??;
    let Term::List(records) = claimed else {
        return Err("claim reply is not a list".into());
    };
    let record = records.first().ok_or("nothing claimed")?;
    Ok((
        field(record, &["lease_token"]),
        field(record, &["fencing_token"]),
    ))
}

#[test]
fn text_commands_decide_as_their_typed_forms() -> TestResult {
    let path = directory("flow-text")?;
    let node = open(&path, 4)?;
    let policy = exec(
        &node,
        "FLOW.POLICY.SET",
        &[
            text("tt"),
            text("INDEXED_STATE_META"),
            text("phase"),
            text("retention_ttl"),
            Term::Int(9_000),
            text("STATE"),
            text("queued"),
            text("MAX_RETRIES"),
            Term::Int(5),
        ],
    )??;
    assert_eq!(field(&policy, &["indexed_state_meta"]), text("phase"));
    assert_eq!(field(&policy, &["retention", "ttl_ms"]), Term::Int(9_000));
    assert_eq!(
        field(&policy, &["states", "queued", "retry", "max_retries"]),
        Term::Int(5)
    );
    exec(
        &node,
        "FLOW.CREATE",
        &[
            text("x1"),
            text("TYPE"),
            text("tt"),
            text("NOW"),
            Term::Int(1_000),
            text("RUN_AT"),
            Term::Int(1_000),
            text("PARTITION"),
            text("p"),
            text("STATE_META"),
            text("phase"),
            text("one"),
        ],
    )??;
    let created = exec(
        &node,
        "FLOW.GET",
        &[text("x1"), text("PARTITION"), text("p")],
    )??;
    assert_eq!(field(&created, &["retention_ttl_ms"]), Term::Int(9_000));
    assert_eq!(
        field(&created, &["state_meta", "queued", "phase"]),
        text("one")
    );
    let (lease, fence) = claim_one(&node, 1_001)?;
    exec(
        &node,
        "FLOW.TRANSITION",
        &[
            text("x1"),
            text("running"),
            text("waiting"),
            text("LEASE_TOKEN"),
            lease,
            text("FENCING"),
            fence,
            text("NOW"),
            Term::Int(1_002),
            text("PARTITION"),
            text("p"),
            text("STATE_META"),
            text("n"),
            Term::Int(5),
        ],
    )??;
    let moved = exec(
        &node,
        "FLOW.GET",
        &[text("x1"), text("PARTITION"), text("p")],
    )??;
    // A number among the arguments counts as its text, as in Elixir.
    assert_eq!(field(&moved, &["state_meta", "waiting", "n"]), text("5"));
    Ok(())
}

#[test]
fn a_retry_limit_and_target_given_as_text_override_the_policy() -> TestResult {
    let path = directory("flow-text-retry")?;
    let node = open(&path, 4)?;
    exec(
        &node,
        "FLOW.CREATE",
        &[
            text("x1"),
            text("TYPE"),
            text("tt"),
            text("NOW"),
            Term::Int(1_000),
            text("RUN_AT"),
            Term::Int(1_000),
            text("PARTITION"),
            text("p"),
        ],
    )??;
    let (lease, fence) = claim_one(&node, 1_001)?;
    exec(
        &node,
        "FLOW.RETRY",
        &[
            text("x1"),
            lease,
            text("FENCING"),
            fence,
            text("NOW"),
            Term::Int(1_002),
            text("PARTITION"),
            text("p"),
            text("MAX_RETRIES"),
            Term::Int(0),
            text("EXHAUSTED_TO"),
            text("parked"),
        ],
    )??;
    let parked = exec(
        &node,
        "FLOW.GET",
        &[text("x1"), text("PARTITION"), text("p")],
    )??;
    assert_eq!(field(&parked, &["state"]), text("parked"));
    assert_eq!(
        exec(&node, "FLOW.GET", &[text("x1"), text("PARTITION")])?,
        refused("ERR syntax error")
    );
    assert_eq!(
        exec(
            &node,
            "FLOW.CREATE",
            &[text("x2"), text("NOW"), text("abc")]
        )?,
        refused("ERR value is not an integer or out of range")
    );
    assert_eq!(
        exec(&node, "FLOW.TRANSITION", &[text("x1")])?,
        refused("ERR wrong number of arguments for 'flow.transition' command")
    );
    // An unknown command is a bad request (audit gap G72).
    assert_eq!(
        exec(&node, "FLOW.NOPE", &[text("x1")])?,
        Err(ServiceError::BadRequest(
            "ERR unknown command 'flow.nope', with args beginning with: ".to_owned()
        ))
    );
    Ok(())
}

#[test]
fn a_text_query_decodes_its_parameters_by_field_type() -> TestResult {
    // `FLOW.QUERY` as text (audit gap G39): `FQL1 query [name value ...]`,
    // integer bounds sent as digits.
    let node = open(&directory("flow-text-query")?, 4)?;
    for (id, at) in [("early", 1_000), ("late", 5_000)] {
        exec(
            &node,
            "FLOW.CREATE",
            &[
                text(id),
                text("TYPE"),
                text("tq"),
                text("PARTITION"),
                text("p"),
                text("NOW"),
                Term::Int(at),
            ],
        )??;
    }
    let query = "FROM runs WHERE partition_key = @tenant AND updated_at_ms BETWEEN @from AND @until ORDER BY updated_at_ms ASC LIMIT 10 RETURN RECORDS";
    let reply = exec(
        &node,
        "FLOW.QUERY",
        &[
            text("FQL1"),
            text(query),
            text("tenant"),
            text("p"),
            text("from"),
            text("0"),
            text("until"),
            text("2000"),
        ],
    )??;
    let records = field(&reply, &["records"]);
    let Term::List(records) = records else {
        return Err(format!("records expected, got {reply:?}").into());
    };
    let ids: Vec<Term> = records
        .iter()
        .map(|record| field(record, &["id"]))
        .collect();
    assert_eq!(ids, [text("early")]);
    Ok(())
}

#[test]
fn a_text_signal_takes_every_if_state_it_names() -> TestResult {
    // Elixir: `flow_signal_command_test.exs` "dispatches Flow signal through
    // Rust AST": repeated `IF_STATE` options name every state the signal
    // may fire in (audit gap G71).
    let node = open(&directory("text-signal-if-states")?, 4)?;
    let words = |values: &[&str]| -> Vec<Term> { values.iter().map(|value| text(value)).collect() };
    for id in ["s1", "s2"] {
        exec(
            &node,
            "FLOW.CREATE",
            &words(&[
                id,
                "TYPE",
                "sd",
                "STATE",
                "waiting_payment",
                "PARTITION",
                "tenant-a",
            ]),
        )??;
    }
    let signal = |id: &str, states: &[&str]| {
        let mut args = words(&[id, "PARTITION", "tenant-a", "SIGNAL", "paid"]);
        for state in states {
            args.extend(words(&["IF_STATE", state]));
        }
        args.extend(words(&[
            "TRANSITION_TO",
            "verify",
            "VALUE",
            "payment_event",
            "payment-bytes",
        ]));
        exec(&node, "FLOW.SIGNAL", &args)
    };
    assert_eq!(
        signal("s1", &["manual_review", "waiting_payment"])??,
        text("OK")
    );
    let read = exec(
        &node,
        "FLOW.GET",
        &words(&["s1", "PARTITION", "tenant-a", "VALUE", "payment_event"]),
    )??;
    assert_eq!(read.get(b"state"), Some(&text("verify")));
    let values = read.get(b"values").ok_or("values expected")?;
    assert_eq!(values.get(b"payment_event"), Some(&text("payment-bytes")));
    let refused = signal("s2", &["manual_review", "closed"])?;
    assert_eq!(
        refused.map_err(|error| error.to_string()),
        Err("ERR flow state mismatch".to_owned())
    );
    Ok(())
}

#[test]
fn an_unknown_text_command_is_a_bad_request() {
    // Elixir: `native_ast_parser_test.exs` "removed Flow collection
    // commands have no parser route", answered with status `bad_request`
    // (audit gap G72).
    for name in ["FLOW.LIST", "flow.search", "FLOW.NOPE"] {
        let refused = text::parse(name.as_bytes(), &[text("job")]);
        let expected = format!(
            "ERR unknown command '{}', with args beginning with: ",
            name.to_ascii_lowercase()
        );
        assert!(
            matches!(&refused, Err(ServiceError::BadRequest(message)) if *message == expected),
            "{name}"
        );
    }
}
