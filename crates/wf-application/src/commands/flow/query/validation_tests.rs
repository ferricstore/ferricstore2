//! Business cases of Elixir's `binder_test.exs`, `request_test.exs` and
//! `error_test.exs`: parameter binding, request checks and diagnostic
//! positions.

use super::super::FlowError;
use super::super::term::Term;
use super::Position;
use super::Reason;
use super::tests::{object, reason, request, text};

#[test]
fn binds_repeated_parameters_once_and_rejects_missing_unused_or_mistyped() {
    let query = "FROM runs WHERE partition_key = @tenant AND updated_at_ms BETWEEN @edge AND @edge ORDER BY updated_at_ms ASC LIMIT 10 RETURN RECORDS";
    assert!(request(query, vec![("tenant", text("a")), ("edge", Term::Int(100))]).is_ok());
    assert_eq!(
        reason(query, vec![("tenant", text("a"))]),
        Some(Reason::MissingParameter)
    );
    assert_eq!(
        reason(
            query,
            vec![
                ("tenant", text("a")),
                ("edge", Term::Int(100)),
                ("secret", text("x"))
            ]
        ),
        Some(Reason::UnexpectedParameter)
    );
    assert_eq!(
        reason(query, vec![("tenant", text("a")), ("edge", text("100"))]),
        Some(Reason::InvalidParameterType)
    );
}

#[test]
fn dynamic_comparisons_cannot_bind_null_or_compound_values() {
    let query = "FROM runs WHERE partition_key = 'tenant-a' AND attribute.region = @region ORDER BY updated_at_ms ASC LIMIT 10 RETURN RECORDS";
    for value in [Term::Nil, object(Vec::new())] {
        assert_eq!(
            reason(query, vec![("region", value)]),
            Some(Reason::InvalidParameterType)
        );
    }
    assert!(request(query, vec![("region", Term::Float(1.5))]).is_ok());
}

#[test]
fn binds_the_cursor_as_a_bounded_keyword() {
    let query = "FROM runs WHERE partition_key = @tenant ORDER BY updated_at_ms ASC LIMIT 10 CURSOR @cursor RETURN RECORDS";
    let tenant = || ("tenant", text("tenant-a"));
    assert!(request(query, vec![tenant(), ("cursor", text(&"a".repeat(128)))]).is_ok());
    assert_eq!(
        reason(query, vec![tenant()]),
        Some(Reason::MissingParameter)
    );
    assert_eq!(
        reason(query, vec![tenant(), ("cursor", text(&"a".repeat(4_097)))]),
        Some(Reason::QueryCursorTooLarge)
    );
    assert_eq!(
        reason(query, vec![tenant(), ("cursor", text("short"))]),
        Some(Reason::QueryCursorInvalid)
    );
}

#[test]
fn collections_need_one_partition_equality_and_integer_order() {
    let shape = Some(Reason::UnsupportedQueryShape);
    let tail = "ORDER BY updated_at_ms DESC LIMIT 10 RETURN RECORDS";
    assert_eq!(
        reason(
            &format!("FROM runs WHERE partition_key = 'a' AND partition_key = 'b' {tail}"),
            Vec::new()
        ),
        shape
    );
    assert_eq!(
        reason(
            &format!("FROM runs WHERE partition_key = 'a' AND partition_key IN ('a', 'b') {tail}"),
            Vec::new()
        ),
        shape
    );
    assert_eq!(
        reason(
            "FROM runs WHERE partition_key = 'a' ORDER BY run_id ASC LIMIT 10 RETURN RECORDS",
            Vec::new()
        ),
        shape
    );
    assert_eq!(
        reason(
            "FROM runs WHERE partition_key = 'a' ORDER BY updated_at_ms ASC, updated_at_ms DESC LIMIT 10 RETURN RECORDS",
            Vec::new()
        ),
        Some(Reason::InvalidSyntax)
    );
}

#[test]
fn metadata_allows_empty_text_but_bounds_its_size() {
    let query = |predicate: &str| {
        format!(
            "FROM runs WHERE partition_key = 'a' AND {predicate} ORDER BY updated_at_ms ASC LIMIT 10 RETURN RECORDS"
        )
    };
    assert!(request(&query("attribute.region = ''"), Vec::new()).is_ok());
    assert_eq!(
        reason(&query("type = ''"), Vec::new()),
        Some(Reason::InvalidParameterType)
    );
    let value = |length: usize| vec![("value", text(&"x".repeat(length)))];
    assert!(request(&query("attribute.region = @value"), value(256)).is_ok());
    assert_eq!(
        reason(&query("attribute.region = @value"), value(257)),
        Some(Reason::QueryValueTooLarge)
    );
}

#[test]
fn positions_count_characters_lines_and_bytes() {
    let query = "FROM runs WHERE state = 'café' @".as_bytes();
    assert_eq!(
        Position::of(query, 33),
        Some(Position {
            byte: 33,
            line: 1,
            column: 32
        })
    );
    let crlf = "FROM runs\r\nWHERE state = 'café'\r\n@".as_bytes();
    let byte = crlf.len();
    assert_eq!(
        Position::of(crlf, byte),
        Some(Position {
            byte,
            line: 3,
            column: 1
        })
    );
    assert_eq!(Position::of(query, 0), None);
}

#[test]
fn only_diagnosed_reasons_report_a_position() {
    let positioned = |query: &str| match request(query, Vec::new()) {
        Err(FlowError::Query(failure)) => Some(failure.payload().get(b"position").is_some()),
        _ => None,
    };
    assert_eq!(
        positioned(
            "FROM runs WHERE partition_key = 'p' AND priority = 99999999999999999999 ORDER BY updated_at_ms ASC LIMIT 1 RETURN RECORDS"
        ),
        Some(false)
    );
    assert_eq!(
        positioned("FROM jobs WHERE run_id = 'a' RETURN RECORD"),
        Some(true)
    );
}
