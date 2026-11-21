//! The production parser's own tests (the Elixir server's native `fql.rs`),
//! ported: the same queries, reasons and failure bytes, with projections
//! compared as canonical selectors instead of external names.

use std::fmt::Write as _;

use super::super::field::{Field, Selector, ValueType};
use super::{
    Direction, Failure, Literal, MAX_QUERY_BYTES, MAX_RETURN_FIELDS, MAX_TOKENS, Mode, Predicate,
    Reason, Shape, Source, Value, parse,
};

fn reason(query: &[u8]) -> Result<Shape, Reason> {
    parse(query)
        .map(|parsed| parsed.shape)
        .map_err(|failure| failure.reason)
}

fn parameter(kind: ValueType, name: &str) -> Value {
    Value::Parameter(kind, name.to_owned())
}

fn find_byte(query: &[u8], needle: &[u8]) -> usize {
    query
        .windows(needle.len())
        .position(|window| window == needle)
        .map_or(0, |position| position.saturating_add(1))
}

#[test]
fn parses_point_reads_and_normalizes_predicate_order() {
    let parsed = parse(
        b"from RUNS where RUN_ID = 'Run''''42' and PARTITION_KEY = @partition return record;",
    );
    let parsed = parsed.ok();
    assert_eq!(
        parsed.as_ref().map(|parsed| parsed.mode),
        Some(Mode::Execute)
    );
    assert_eq!(
        parsed.as_ref().map(|parsed| parsed.shape),
        Some(Shape::Point)
    );
    assert_eq!(parsed.as_ref().and_then(|parsed| parsed.limit), Some(1));
    assert_eq!(
        parsed.map(|parsed| parsed.predicates),
        Some(vec![
            Predicate::Eq(
                Field::PartitionKey,
                parameter(ValueType::Keyword, "partition")
            ),
            Predicate::Eq(
                Field::RunId,
                Value::Literal(Literal::Keyword(b"Run''42".to_vec()))
            ),
        ])
    );
}

#[test]
fn parses_run_id_only_point_reads() {
    let parsed = parse(b"FROM runs WHERE run_id = @run_id RETURN RECORD").ok();
    assert_eq!(
        parsed.as_ref().map(|parsed| parsed.shape),
        Some(Shape::Point)
    );
    assert_eq!(
        parsed.map(|parsed| parsed.predicates),
        Some(vec![Predicate::Eq(
            Field::RunId,
            parameter(ValueType::Keyword, "run_id")
        )])
    );
}

#[test]
fn parses_composite_collection_operators() -> Result<(), &'static str> {
    let parsed = parse(
        b"EXPLAIN FROM runs WHERE partition_key = @tenant AND state IN ('failed', 'completed') AND updated_at_ms FROM @from TO @until ORDER BY updated_at_ms DESC, run_id DESC LIMIT 25 RETURN RECORDS",
    )
    .ok();
    let parsed = parsed.ok_or("collection query does not parse")?;
    assert_eq!(parsed.mode, Mode::Explain);
    assert_eq!(parsed.shape, Shape::Collection);
    assert!(
        matches!(parsed.predicates.get(1), Some(Predicate::In(_, values)) if values.len() == 2)
    );
    assert_eq!(
        parsed.predicates.get(2),
        Some(&Predicate::TimeWindow(
            Field::UpdatedAtMs,
            parameter(ValueType::Integer, "from"),
            parameter(ValueType::Integer, "until"),
        ))
    );
    assert_eq!(
        parsed.order_by,
        vec![
            (Field::UpdatedAtMs, Direction::Desc),
            (Field::RunId, Direction::Desc)
        ]
    );
    assert_eq!(parsed.limit, Some(25));
    assert_eq!(parsed.cursor, None);
    Ok(())
}

#[test]
fn parses_explain_analyze_as_a_distinct_mode() {
    let parsed = parse(
        b"EXPLAIN ANALYZE FROM runs WHERE partition_key = @tenant ORDER BY updated_at_ms DESC LIMIT 10 RETURN RECORDS",
    )
    .ok();
    assert_eq!(
        parsed.as_ref().map(|parsed| parsed.mode),
        Some(Mode::Analyze)
    );
    assert_eq!(parsed.map(|parsed| parsed.shape), Some(Shape::Collection));
}

#[test]
fn parses_direct_event_history() {
    let parsed =
        parse(b"FROM events WHERE run_id = @run_id ORDER BY event_id DESC LIMIT 25 RETURN RECORDS")
            .ok();
    assert_eq!(
        parsed.as_ref().map(|parsed| parsed.source),
        Some(Source::Events)
    );
    assert_eq!(
        parsed.as_ref().map(|parsed| parsed.shape),
        Some(Shape::History)
    );
    assert_eq!(parsed.map(|parsed| parsed.limit), Some(Some(25)));
}

#[test]
fn parses_bounded_source_specific_return_projections() {
    let runs = parse(
        b"FROM runs WHERE run_id = @run_id RETURN RECORD (run_id, state, attributes, state_meta, attribute['customer'])",
    )
    .ok()
    .and_then(|parsed| parsed.projection);
    assert_eq!(
        runs,
        Some(vec![
            Selector::Field(Field::RunId),
            Selector::Field(Field::State),
            Selector::Attributes,
            Selector::StateMeta,
            Selector::Field(Field::Attribute("customer".to_owned())),
        ])
    );
    let events = parse(
        b"FROM events WHERE run_id = @run_id ORDER BY event_id ASC LIMIT 10 RETURN RECORDS (event_id, fields['event'])",
    )
    .ok()
    .and_then(|parsed| parsed.projection);
    assert_eq!(
        events,
        Some(vec![
            Selector::EventId,
            Selector::EventField("event".to_owned())
        ])
    );
    for (query, expected) in [
        (
            b"FROM runs WHERE run_id = 'run-1' RETURN RECORD (event_id)".as_slice(),
            Reason::UnsupportedField,
        ),
        (
            b"FROM events WHERE run_id = 'run-1' ORDER BY event_id ASC LIMIT 1 RETURN RECORDS (state)",
            Reason::UnsupportedField,
        ),
        (
            b"FROM runs WHERE run_id = 'run-1' RETURN RECORD (state, STATE)",
            Reason::DuplicateProjectionField,
        ),
        (
            b"FROM runs WHERE run_id = 'run-1' RETURN RECORD (attribute.customer, attribute['customer'])",
            Reason::DuplicateProjectionField,
        ),
        (
            b"FROM runs WHERE run_id = 'run-1' RETURN RECORD (state_meta.review.owner, state_meta['review']['owner'])",
            Reason::DuplicateProjectionField,
        ),
    ] {
        assert_eq!(reason(query), Err(expected));
    }
}

#[test]
fn parses_scalar_counts_without_row_pagination() -> Result<(), &'static str> {
    let parsed = parse(
        b"EXPLAIN FROM runs WHERE partition_key = @partition AND type = 'payment' AND state = 'failed' RETURN COUNT;",
    )
    .ok();
    let parsed = parsed.ok_or("count query does not parse")?;
    assert_eq!(parsed.mode, Mode::Explain);
    assert_eq!(parsed.shape, Shape::Count);
    assert_eq!(parsed.predicates.len(), 3);
    assert_eq!(parsed.order_by.len(), 0);
    assert_eq!(parsed.limit, None);
    for query in [
        b"FROM events WHERE partition_key = 'p' RETURN COUNT".as_slice(),
        b"FROM runs WHERE partition_key = 'p' LIMIT 1 RETURN COUNT",
        b"FROM runs WHERE partition_key = 'p' CURSOR @page RETURN COUNT",
    ] {
        assert_eq!(reason(query), Err(Reason::UnsupportedQueryShape));
    }
    Ok(())
}

#[test]
fn parses_only_parameterized_collection_cursors() {
    let parsed = parse(
        b"FROM runs WHERE partition_key = @tenant ORDER BY run_id ASC LIMIT 25 CURSOR @page RETURN RECORDS",
    )
    .ok();
    assert_eq!(
        parsed.and_then(|parsed| parsed.cursor),
        Some(parameter(ValueType::Keyword, "page"))
    );
    assert_eq!(
        reason(b"FROM runs WHERE partition_key = @tenant ORDER BY run_id ASC LIMIT 25 CURSOR 'plaintext-token' RETURN RECORDS"),
        Err(Reason::UnsupportedQueryShape)
    );
}

#[test]
fn parses_ranges_and_null_missing_checks() {
    for predicate in [
        b"priority BETWEEN 1 AND 5".as_slice(),
        b"attribute.region IS NULL",
        b"attribute.region IS MISSING",
    ] {
        let mut query = b"FROM runs WHERE partition_key = 'p' AND ".to_vec();
        query.extend_from_slice(predicate);
        query.extend_from_slice(b" ORDER BY run_id ASC LIMIT 1 RETURN RECORDS");
        assert_eq!(reason(&query), Ok(Shape::Collection));
    }
}

#[test]
fn rejects_unsupported_shapes_and_unbounded_integers() {
    assert_eq!(
        reason(b"FROM runs RETURN RECORDS"),
        Err(Reason::UnsupportedQueryShape)
    );
    assert_eq!(
        reason(b"FROM events WHERE partition_key = 'p' AND run_id = 'r' RETURN RECORD"),
        Err(Reason::UnsupportedQueryShape)
    );
    assert_eq!(
        reason(b"FROM runs WHERE partition_key = 'p' AND priority BETWEEN 999999999999999999999999 AND 9 ORDER BY run_id ASC LIMIT 1 RETURN RECORDS"),
        Err(Reason::InvalidParameterType)
    );
}

#[test]
fn enforces_byte_and_token_budgets() {
    let query = vec![b'x'; MAX_QUERY_BYTES.saturating_add(1)];
    assert_eq!(reason(&query), Err(Reason::QueryTooLarge));
    let exact = "x ".repeat(MAX_TOKENS).into_bytes();
    assert_eq!(reason(&exact), Err(Reason::InvalidSyntax));
    let over = "x ".repeat(MAX_TOKENS.saturating_add(1)).into_bytes();
    assert_eq!(reason(&over), Err(Reason::UnsupportedQueryShape));
}

fn predicate_query(predicates: usize) -> Vec<u8> {
    let mut query = String::from("FROM runs WHERE partition_key = @partition");
    for index in 1..predicates {
        let _written = write!(query, " AND attribute.field{index} = @value{index}");
    }
    query.push_str(" ORDER BY updated_at_ms ASC LIMIT 25 RETURN RECORDS");
    query.into_bytes()
}

#[test]
fn enforces_predicate_budget_at_the_first_excess_predicate() {
    assert!(parse(&predicate_query(12)).is_ok());
    let over = predicate_query(13);
    assert_eq!(
        parse(&over).err(),
        Some(Failure {
            reason: Reason::UnsupportedQueryShape,
            byte: find_byte(&over, b"attribute.field12"),
        })
    );
}

fn in_query(cardinality: usize) -> Vec<u8> {
    let values: Vec<String> = (0..cardinality)
        .map(|index| format!("'state{index}'"))
        .collect();
    format!(
        "FROM runs WHERE partition_key = @partition AND state IN ({}) ORDER BY updated_at_ms DESC LIMIT 25 RETURN RECORDS",
        values.join(",")
    )
    .into_bytes()
}

#[test]
fn enforces_in_value_budget_at_the_first_excess_value() {
    assert!(parse(&in_query(20)).is_ok());
    let over = in_query(21);
    assert_eq!(
        parse(&over).err(),
        Some(Failure {
            reason: Reason::UnsupportedQueryShape,
            byte: find_byte(&over, b"'state20'"),
        })
    );
}

fn projection_query(cardinality: usize) -> Vec<u8> {
    let fields: Vec<String> = (0..cardinality)
        .map(|index| format!("attribute['field-{index}']"))
        .collect();
    format!(
        "FROM runs WHERE partition_key = @partition ORDER BY updated_at_ms DESC LIMIT 25 RETURN RECORDS ({})",
        fields.join(",")
    )
    .into_bytes()
}

#[test]
fn enforces_return_projection_budget_at_the_first_excess_field() {
    assert!(parse(&projection_query(MAX_RETURN_FIELDS)).is_ok());
    let over = projection_query(MAX_RETURN_FIELDS.saturating_add(1));
    let excess = format!("attribute['field-{MAX_RETURN_FIELDS}']");
    assert_eq!(
        parse(&over).err(),
        Some(Failure {
            reason: Reason::QueryProjectionLimitExceeded,
            byte: find_byte(&over, excess.as_bytes()),
        })
    );
}

#[test]
fn diagnostics_report_the_actual_failure_token() {
    let missing_tail =
        b"FROM runs WHERE partition_key = 'p' AND attribute['customer.region'] = 'eu'";
    assert_eq!(
        parse(missing_tail).err(),
        Some(Failure {
            reason: Reason::UnsupportedQueryShape,
            byte: missing_tail.len().saturating_add(1),
        })
    );
    let explain_cursor = b"EXPLAIN FROM runs WHERE partition_key = @tenant ORDER BY updated_at_ms DESC LIMIT 10 CURSOR @page RETURN RECORDS";
    assert_eq!(
        parse(explain_cursor).err(),
        Some(Failure {
            reason: Reason::QueryCursorInvalid,
            byte: find_byte(explain_cursor, b"CURSOR"),
        })
    );
}
