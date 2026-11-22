//! `FLOW.QUERY` in its text form (audit gap G39), as Elixir's native
//! parser and `Binder.bind_text` read it: `version query [name value ...]`,
//! string values decoded by the type the query declares, and a missing
//! parameter named in the failure's context (Elixir's
//! `native_ast_parser_test.exs` and `binder_test.exs`).

use super::super::FlowError;
use super::super::term::Term;
use super::super::text::{Parsed, parse};
use super::Reason;

fn words(items: &[&str]) -> Vec<Term> {
    items.iter().map(|item| Term::text(item)).collect()
}

fn text_query(args: &[&str]) -> Result<Parsed, FlowError> {
    parse(b"FLOW.QUERY", &words(args))
}

fn refusal(args: &[&str]) -> String {
    match text_query(args) {
        Err(error) => error.to_string(),
        Ok(_) => "accepted".to_owned(),
    }
}

const POINT: &str =
    "FROM runs WHERE partition_key = @partition AND run_id = @flow_id RETURN RECORD";
const WINDOW: &str = "FROM runs WHERE partition_key = @tenant AND updated_at_ms BETWEEN @from AND @until AND attribute.group = @group ORDER BY updated_at_ms DESC LIMIT 10 RETURN RECORDS";

#[test]
fn reads_the_version_query_and_name_value_pairs() {
    // Elixir: "FLOW.QUERY keeps FQL version, query text, and named
    // parameters distinct".
    assert!(text_query(&["FQL1", POINT, "partition", "tenant-a", "flow_id", "run-123"]).is_ok());
    assert_eq!(
        refusal(&["FQL1", POINT, "dangling"]),
        "ERR FLOW.QUERY parameters must be name/value pairs"
    );
    let mut many = vec!["FQL1", POINT];
    let names: Vec<String> = (1..=65).map(|index| format!("parameter_{index}")).collect();
    for name in &names {
        many.extend([name.as_str(), "value"]);
    }
    assert_eq!(
        refusal(&many),
        "ERR FLOW.QUERY accepts at most 64 named parameters"
    );
    assert_eq!(
        refusal(&["FQL1", POINT, "partition", "a", "partition", "b"]),
        "ERR FLOW.QUERY parameter names must be unique"
    );
    assert_eq!(
        refusal(&["FQL1"]),
        "ERR wrong number of arguments for 'flow.query' command"
    );
}

#[test]
fn decodes_each_value_by_the_type_its_field_declares() {
    // Elixir: "text binding decodes only the type declared by the query
    // field".
    let base = ["FQL1", WINDOW, "tenant", "tenant-a", "group", "42"];
    let with = |from: &str, until: &str| {
        let mut args = base.to_vec();
        args.extend(["from", from, "until", until]);
        text_query(&args)
    };
    assert!(with("-9223372036854775808", "9223372036854775807").is_ok());
    let huge = "9".repeat(8_000);
    let reason = match with(&huge, "10") {
        Err(FlowError::Query(failure)) => Some(failure.reason),
        _ => None,
    };
    assert_eq!(reason, Some(Reason::InvalidParameterType));
}

#[test]
fn a_missing_parameter_is_named_in_the_context() -> Result<(), String> {
    // Elixir: "diagnostic binding names missing parameters without
    // exposing values".
    let query = "FROM runs WHERE partition_key = @partition AND type = @type ORDER BY updated_at_ms DESC LIMIT 10 RETURN RECORDS";
    let failure = match text_query(&["FQL1", query, "partition", "tenant-a"]) {
        Err(FlowError::Query(failure)) => failure,
        other => return Err(format!("a missing parameter expected, got {other:?}")),
    };
    assert_eq!(failure.reason, Reason::MissingParameter);
    let payload = failure.payload();
    assert_eq!(
        payload.get(b"context"),
        Some(&Term::Map(vec![(
            b"missing_parameters".to_vec(),
            Term::List(vec![Term::text("type")])
        )]))
    );
    assert_eq!(
        payload.get(b"detail"),
        Some(&Term::text("Missing named parameter: @type."))
    );
    Ok(())
}

#[test]
fn index_ids_are_checked_before_their_lookup() {
    // Elixir's `commands_test.exs`: "returns actionable usage and
    // availability errors" (audit gap G41; its registry-unavailable case
    // has no counterpart: the node's catalog is always there).
    let lookup = |id: &str| {
        super::indexes::reply(
            &super::indexes::Indexes {
                id: Some(id.as_bytes().to_vec()),
            },
            4,
            0,
        )
        .err()
        .map(|error| error.to_string())
    };
    let invalid = Some("ERR invalid query index id; use FLOW.QUERY.INDEXES [index-id]".to_owned());
    assert_eq!(lookup("invalid/index"), invalid);
    assert_eq!(lookup(""), invalid);
    assert_eq!(lookup(&"x".repeat(65)), invalid);
    assert_eq!(
        lookup("unknown.index-1"),
        Some("ERR query index not found; use FLOW.QUERY.INDEXES to list valid indexes".to_owned())
    );
    let usage = match parse(b"FLOW.QUERY.INDEXES", &words(&["one", "two"])) {
        Err(error) => error.to_string(),
        Ok(_) => String::new(),
    };
    assert_eq!(usage, "ERR syntax error; use FLOW.QUERY.INDEXES [index-id]");
}
