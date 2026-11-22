//! Elixir's `error_test.exs`, as this node's failures reply: each
//! diagnosed reason's detail, hint, context and position, from its parser's
//! own failure byte, and no echo of the rejected query's names or literals.
//! (Elixir's budget reasons, its message-to-reason lookup and the bounds of
//! hand-built diagnostics are its own.)

use super::super::FlowError;
use super::super::term::Term;
use super::Position;
use super::tests::request;

/// The failure payload of `query`, or nil when it does not fail as a query.
fn payload(query: &str) -> Term {
    match request(query, Vec::new()) {
        Err(FlowError::Query(failure)) => failure.payload(),
        _ => Term::Nil,
    }
}

/// The text under `key`, or empty when there is none.
fn text_of(term: &Term, key: &str) -> String {
    match term.get(key.as_bytes()) {
        Some(Term::Bin(bytes)) => String::from_utf8_lossy(bytes).into_owned(),
        _ => String::new(),
    }
}

fn position(term: &Term) -> Option<(i64, i64, i64)> {
    let position = term.get(b"position")?;
    let int = |key: &[u8]| match position.get(key) {
        Some(Term::Int(value)) => *value,
        _ => -1,
    };
    Some((int(b"byte"), int(b"line"), int(b"column")))
}

fn at(query: &str, needle: &str) -> i64 {
    query
        .find(needle)
        .and_then(|offset| i64::try_from(offset.saturating_add(1)).ok())
        .unwrap_or(0)
}

#[test]
fn projection_errors_say_what_to_change() {
    // Elixir: "makes bounded return projection errors actionable".
    let duplicate = payload("FROM runs WHERE run_id = 'run-1' RETURN RECORD (state, STATE)");
    assert!(text_of(&duplicate, "detail").contains("same result field"));
    assert!(text_of(&duplicate, "hint").contains("Remove the duplicate"));
    let fields: Vec<String> = (1..=33)
        .map(|index| format!("attribute['f{index}']"))
        .collect();
    let limit = payload(&format!(
        "FROM runs WHERE partition_key = 'p' ORDER BY updated_at_ms DESC LIMIT 10 RETURN RECORDS ({})",
        fields.join(", ")
    ));
    assert_eq!(text_of(&limit, "code"), "query_projection_limit_exceeded");
    assert_eq!(
        limit.get(b"context"),
        Some(&Term::Map(vec![(
            b"max_projection_fields".to_vec(),
            Term::Int(32)
        )]))
    );
    assert!(text_of(&limit, "hint").contains("Select fewer fields"));
}

#[test]
fn syntax_and_shape_errors_carry_their_position_and_guidance() {
    // Elixir: "adds a parser-supplied source position and actionable
    // guidance to syntax errors", "positions unsupported clause shapes
    // without changing their stable code", "positions EXPLAIN ANALYZE
    // failures after the full mode prefix" and "points at the first
    // unexpected clause token instead of the query end".
    let syntax = payload("FROM runs\nWHERE @");
    assert_eq!(text_of(&syntax, "code"), "invalid_syntax");
    assert_eq!(position(&syntax), Some((17, 2, 7)));
    assert!(text_of(&syntax, "detail").contains("reported position"));
    assert!(text_of(&syntax, "hint").contains("FROM runs"));
    let shape = payload("FROM runs RETURN RECORDS");
    assert_eq!(text_of(&shape, "code"), "unsupported_query_shape");
    assert_eq!(
        text_of(&shape, "message"),
        "ERR FQL1 query shape is not supported"
    );
    assert_eq!(position(&shape), Some((11, 1, 11)));
    assert!(text_of(&shape, "hint").contains("Point reads require"));
    assert!(text_of(&shape, "hint").contains("Collection reads require"));
    let analyze = "EXPLAIN ANALYZE FROM runs RETURN RECORDS";
    let byte = at(analyze, "RETURN");
    assert_eq!(position(&payload(analyze)), Some((byte, 1, byte)));
    let bogus = "FROM runs WHERE state = 'failed' BOGUS RETURN RECORDS";
    let byte = at(bogus, "BOGUS");
    assert_eq!(position(&payload(bogus)), Some((byte, 1, byte)));
}

#[test]
fn cursor_field_and_source_errors_never_echo_the_query() {
    // Elixir: "makes EXPLAIN ANALYZE cursor rejection actionable", "lists
    // bounded supported fields without echoing the rejected query" and
    // "reports unsupported sources without echoing query literals".
    let cursor = "EXPLAIN ANALYZE FROM runs WHERE partition_key = @tenant ORDER BY updated_at_ms DESC LIMIT 10 CURSOR @secret_page RETURN RECORDS";
    let reply = payload(cursor);
    let byte = at(cursor, "CURSOR");
    assert_eq!(position(&reply), Some((byte, 1, byte)));
    assert!(text_of(&reply, "detail").contains("fresh query"));
    assert!(text_of(&reply, "hint").contains("Remove CURSOR"));
    assert!(!format!("{reply:?}").contains("secret_page"));

    let field = payload(
        "FROM runs WHERE tenant_secret = 'value-secret' ORDER BY updated_at_ms DESC LIMIT 10 RETURN RECORDS",
    );
    assert_eq!(position(&field), Some((17, 1, 17)));
    let supported = match field
        .get(b"context")
        .and_then(|context| context.get(b"supported_fields"))
    {
        Some(Term::List(supported)) => supported.clone(),
        _ => Vec::new(),
    };
    assert!(
        !supported.is_empty(),
        "supported fields expected in {field:?}"
    );
    let names: Vec<String> = supported
        .iter()
        .filter_map(|name| match name {
            Term::Bin(bytes) => Some(String::from_utf8_lossy(bytes).into_owned()),
            _ => None,
        })
        .collect();
    let mut sorted = names.clone();
    sorted.sort();
    assert_eq!(names, sorted);
    for name in [
        "partition_key",
        "updated_at_ms",
        "attribute.<name>",
        "state_meta.<state>.<name>",
    ] {
        assert!(names.iter().any(|found| found == name), "{name}");
    }
    let events = field
        .get(b"context")
        .and_then(|context| context.get(b"supported_projection_fields"))
        .and_then(|fields| fields.get(b"events"))
        .cloned();
    assert_eq!(
        events,
        Some(Term::List(vec![
            Term::text("event_id"),
            Term::text("fields"),
            Term::text("fields['<name>']")
        ]))
    );
    let echoed = format!("{field:?}");
    assert!(!echoed.contains("tenant_secret") && !echoed.contains("value-secret"));

    let source = payload("FROM tenant_secret WHERE run_id = 'run-secret' RETURN RECORD");
    assert_eq!(text_of(&source, "code"), "unsupported_source");
    assert_eq!(position(&source), Some((6, 1, 6)));
    assert_eq!(
        source.get(b"context"),
        Some(&Term::Map(vec![(
            b"supported_sources".to_vec(),
            Term::List(vec![Term::text("events"), Term::text("runs")])
        )]))
    );
    let echoed = format!("{source:?}");
    assert!(!echoed.contains("tenant_secret") && !echoed.contains("run-secret"));
}

#[test]
fn positions_count_bytes_lines_and_characters_at_grammar_boundaries() {
    // Elixir: "positions malformed grammar boundaries deterministically",
    // "drops source positions outside the supplied query boundary" (its
    // UTF-8 and CRLF cases are in
    // `validation_tests::positions_count_characters_lines_and_bytes`).
    let place = |query: &str, byte: usize| {
        Position::of(query.as_bytes(), byte).map(|found| (found.byte, found.line, found.column))
    };
    let marker = |query: &str, needle: &str, nth: usize| {
        query
            .match_indices(needle)
            .nth(nth)
            .map_or(0, |(offset, _)| offset.saturating_add(1))
    };
    let empty_in = "FROM runs WHERE state IN () RETURN COUNT";
    let trailing_in = "FROM runs WHERE state IN ('failed',) RETURN COUNT";
    let extra = "FROM runs WHERE state = 'failed' RETURN COUNT;;";
    let unterminated = "FROM runs WHERE state = 'unterminated";
    for (query, byte, line, column) in [
        ("", 1, 1, 1),
        ("FROM", 5, 1, 5),
        ("FROM runs\r\nWHERE", 17, 2, 6),
        ("FROM runs WHERE state =", 24, 1, 24),
        (
            empty_in,
            marker(empty_in, ")", 0),
            1,
            marker(empty_in, ")", 0),
        ),
        (
            trailing_in,
            marker(trailing_in, ")", 0),
            1,
            marker(trailing_in, ")", 0),
        ),
        (extra, marker(extra, ";", 1), 1, marker(extra, ";", 1)),
        (
            unterminated,
            marker(unterminated, "'", 0),
            1,
            marker(unterminated, "'", 0),
        ),
    ] {
        assert_eq!(place(query, byte), Some((byte, line, column)), "{query:?}");
    }
    assert_eq!(place("query", 0), None);
    assert_eq!(place("query", 7), None);
}
