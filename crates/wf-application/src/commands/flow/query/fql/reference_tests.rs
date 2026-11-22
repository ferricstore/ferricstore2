//! Elixir's `reference_parser_test.exs`: its queries through this node's
//! parser, what each accepts, and the reason and byte of each refusal
//! (bytes count from 1, as Elixir's diagnostics do).

use super::{MAX_RETURN_FIELDS, Mode, Reason, parse};

fn outcome(query: &str) -> Result<Mode, (Reason, usize)> {
    parse(query.as_bytes())
        .map(|parsed| parsed.mode)
        .map_err(|failure| (failure.reason, failure.byte))
}

fn refused(query: &str) -> Option<Reason> {
    outcome(query).err().map(|(reason, _)| reason)
}

/// The 1-based byte where `needle` starts in `query`.
fn at(query: &str, needle: &str) -> usize {
    query
        .find(needle)
        .map_or(0, |offset| offset.saturating_add(1))
}

#[test]
fn accepts_every_shape_the_reference_parser_accepts() {
    // Elixir: "parses bounded run and history return projections", "...
    // the bounded run point-read shape", "... a run-id point read that
    // uses the canonical auto partition", "... bounded direct event
    // history", "... a partition-contained parent lineage query",
    // "normalizes keywords and supports a trailing semicolon", "parses
    // EXPLAIN and named parameters without binding values", "parses
    // bounded collection equality, IN, time window, order, and limit",
    // "parses an optional parameterized collection cursor", "parses
    // inclusive ranges and explicit null or missing predicates", "parses
    // safely quoted metadata field segments" and "parses a bounded scalar
    // count without row ordering or pagination".
    for (query, mode) in [
        (
            "FROM runs WHERE partition_key = 'tenant-a' AND run_id = 'run-1' RETURN RECORD (run_id, state, attributes, state_meta, attribute['customer'], state_meta['review.v2']['owner'])",
            Mode::Execute,
        ),
        (
            "FROM events WHERE run_id = 'run-1' ORDER BY event_id DESC LIMIT 10 RETURN RECORDS (event_id, fields['event'], fields['worker']);",
            Mode::Execute,
        ),
        (
            "FROM runs WHERE partition_key = 'tenant-a' AND run_id = 'run-123' RETURN RECORD",
            Mode::Execute,
        ),
        (
            "FROM runs WHERE run_id = 'run-auto' RETURN RECORD",
            Mode::Execute,
        ),
        (
            "FROM events WHERE run_id = @run_id ORDER BY event_id DESC LIMIT 25 RETURN RECORDS",
            Mode::Execute,
        ),
        (
            "FROM runs WHERE partition_key = @partition AND parent_flow_id = @parent ORDER BY updated_at_ms DESC LIMIT 25 RETURN RECORDS",
            Mode::Execute,
        ),
        (
            "from RUNS where RUN_ID = 'Run''''42' and PARTITION_KEY = 'partition' return record;",
            Mode::Execute,
        ),
        (
            "EXPLAIN FROM runs WHERE partition_key = @partition AND run_id = @flow_id RETURN RECORD",
            Mode::Explain,
        ),
        (
            "EXPLAIN ANALYZE FROM runs WHERE partition_key = @partition AND state = 'failed' ORDER BY updated_at_ms DESC LIMIT 10 RETURN RECORDS",
            Mode::Analyze,
        ),
        (
            "FROM runs WHERE partition_key = @tenant AND state IN ('failed', 'completed') AND updated_at_ms FROM @from TO @until ORDER BY updated_at_ms DESC LIMIT 25 RETURN RECORDS",
            Mode::Execute,
        ),
        (
            "FROM runs WHERE partition_key = @tenant ORDER BY updated_at_ms DESC LIMIT 25 CURSOR @page RETURN RECORDS",
            Mode::Execute,
        ),
        (
            "EXPLAIN FROM runs WHERE partition_key = 'tenant-a' AND priority BETWEEN 1 AND 5 ORDER BY updated_at_ms DESC LIMIT 10 RETURN RECORDS;",
            Mode::Explain,
        ),
        (
            "EXPLAIN FROM runs WHERE partition_key = 'tenant-a' AND attribute.region IS NULL ORDER BY updated_at_ms DESC LIMIT 10 RETURN RECORDS;",
            Mode::Explain,
        ),
        (
            "EXPLAIN FROM runs WHERE partition_key = 'tenant-a' AND attribute.region IS MISSING ORDER BY updated_at_ms DESC LIMIT 10 RETURN RECORDS;",
            Mode::Explain,
        ),
        (
            "FROM runs WHERE partition_key = @tenant AND state_meta['review.v2']['ai.model'] = @model ORDER BY updated_at_ms DESC LIMIT 10 RETURN RECORDS",
            Mode::Execute,
        ),
        (
            "FROM runs WHERE partition_key = @tenant AND attribute['customer''s.region'] = @region ORDER BY updated_at_ms DESC LIMIT 10 RETURN RECORDS",
            Mode::Execute,
        ),
        (
            "EXPLAIN FROM runs WHERE partition_key = @partition AND type = 'payment' AND state = 'failed' RETURN COUNT;",
            Mode::Explain,
        ),
        // `request_test.exs`: "EXPLAIN ANALYZE is a bounded fresh execution
        // and rejects cursors", its count.
        (
            "EXPLAIN ANALYZE FROM runs WHERE partition_key = 'tenant-a' RETURN COUNT",
            Mode::Analyze,
        ),
    ] {
        assert_eq!(
            outcome(query).map_err(|(reason, _)| reason),
            Ok(mode),
            "{query}"
        );
    }
}

#[test]
fn refuses_what_the_reference_parser_refuses() {
    // Elixir: "rejects unsafe, duplicate, empty, and oversized return
    // projections", "parses EXPLAIN ANALYZE as a distinct fresh-execution
    // mode" (its refusals), "parses an optional parameterized collection
    // cursor" (its refusals), "parses a bounded scalar count ..." (its
    // refusals), "rejects collection queries without explicit bounds or
    // malformed IN lists", "rejects unbounded and unsupported query
    // shapes", "rejects oversized input before tokenization" and "rejects
    // huge integer literals without constructing an unbounded integer".
    let prefix = "FROM runs WHERE partition_key = 'tenant-a' ORDER BY updated_at_ms DESC LIMIT 10 RETURN RECORDS ";
    let history =
        "FROM events WHERE run_id = 'run-1' ORDER BY event_id ASC LIMIT 10 RETURN RECORDS ";
    let fields: Vec<String> = (1..=MAX_RETURN_FIELDS.saturating_add(1))
        .map(|index| format!("attribute['field-{index}']"))
        .collect();
    let too_long = format!(
        "FROM runs WHERE partition_key = 'p' AND run_id = '{}' RETURN RECORD",
        "x".repeat(16_385)
    );
    let huge = format!(
        "FROM runs WHERE partition_key = 'tenant-a' AND priority BETWEEN {} AND 9 ORDER BY run_id ASC LIMIT 1 RETURN RECORDS",
        "9".repeat(8_000)
    );
    let cases: Vec<(String, Reason)> = vec![
        (format!("{prefix}(lease_token)"), Reason::UnsupportedField),
        (format!("{prefix}(event_id)"), Reason::UnsupportedField),
        (format!("{prefix}(state, STATE)"), Reason::DuplicateProjectionField),
        (format!("{prefix}()"), Reason::UnsupportedQueryShape),
        (format!("{prefix}(state,)"), Reason::UnsupportedQueryShape),
        (format!("{prefix}({})", fields.join(", ")), Reason::QueryProjectionLimitExceeded),
        (format!("{history}(state)"), Reason::UnsupportedField),
        (format!("{history}(fields['__internal'])"), Reason::UnsupportedField),
        ("ANALYZE FROM runs WHERE run_id = 'run-1' RETURN RECORD".to_owned(), Reason::InvalidSyntax),
        ("EXPLAIN ANALYZE FROM runs WHERE partition_key = @partition ORDER BY updated_at_ms DESC LIMIT 10 CURSOR @page RETURN RECORDS".to_owned(), Reason::QueryCursorInvalid),
        ("FROM runs WHERE partition_key = @tenant ORDER BY run_id ASC LIMIT 10 CURSOR 'token-in-query-text' RETURN RECORDS".to_owned(), Reason::UnsupportedQueryShape),
        ("FROM runs WHERE partition_key = @tenant AND run_id = @id CURSOR @page RETURN RECORD".to_owned(), Reason::UnsupportedQueryShape),
        ("FROM events WHERE partition_key = 'p' RETURN COUNT".to_owned(), Reason::UnsupportedQueryShape),
        ("FROM runs WHERE partition_key = 'p' ORDER BY updated_at_ms DESC LIMIT 1 RETURN COUNT".to_owned(), Reason::UnsupportedQueryShape),
        ("FROM runs WHERE partition_key = 'p' LIMIT 1 RETURN COUNT".to_owned(), Reason::UnsupportedQueryShape),
        ("FROM runs WHERE partition_key = 'p' CURSOR @page RETURN COUNT".to_owned(), Reason::UnsupportedQueryShape),
        ("FROM runs WHERE partition_key = 'tenant-a' RETURN RECORDS".to_owned(), Reason::UnsupportedQueryShape),
        ("FROM runs WHERE partition_key = 'tenant-a' LIMIT 10 RETURN RECORDS".to_owned(), Reason::UnsupportedQueryShape),
        ("FROM runs WHERE partition_key = 'tenant-a' AND state IN () LIMIT 10 RETURN RECORDS".to_owned(), Reason::UnsupportedQueryShape),
        ("FROM runs WHERE partition_key = 'tenant-a' AND state IN ('a',) LIMIT 10 RETURN RECORDS".to_owned(), Reason::UnsupportedQueryShape),
        ("FROM runs RETURN RECORDS".to_owned(), Reason::UnsupportedQueryShape),
        ("FROM runs WHERE tenant_ref = 'forged' AND run_id = 'one' RETURN RECORD".to_owned(), Reason::UnsupportedField),
        ("FROM runs WHERE partition_key = 'p' OR run_id = 'two' RETURN RECORD".to_owned(), Reason::UnsupportedQueryShape),
        // `request_test.exs`: "EXPLAIN rejects execution cursors" and
        // "event history is bounded by one run and ordered by event id".
        (
            "EXPLAIN FROM runs WHERE partition_key = @tenant ORDER BY updated_at_ms DESC LIMIT 10 CURSOR @page RETURN RECORDS".to_owned(),
            Reason::QueryCursorInvalid,
        ),
        (
            "FROM events WHERE run_id = 'run-1' ORDER BY updated_at_ms ASC LIMIT 10 RETURN RECORDS".to_owned(),
            Reason::UnsupportedQueryShape,
        ),
        (too_long, Reason::QueryTooLarge),
        (huge, Reason::InvalidParameterType),
    ];
    for (query, reason) in cases {
        let short: String = query.chars().take(120).collect();
        assert_eq!(refused(&query), Some(reason), "{short}");
    }
}

#[test]
fn diagnostics_point_at_the_rejected_token() {
    // Elixir: "reports the unsupported source and field tokens after
    // optional mode prefixes", "reports a later unsupported field after a
    // valid metadata predicate", "reports unsupported and duplicate return
    // projection fields at the rejected selector" and "reports the clause
    // that appears where WHERE is required".
    let field =
        "FROM runs WHERE tenant_secret = 'x' ORDER BY updated_at_ms DESC LIMIT 10 RETURN RECORDS";
    let source = "EXPLAIN FROM jobs WHERE partition_key = 'p' ORDER BY updated_at_ms DESC LIMIT 1 RETURN RECORDS";
    let later = "FROM runs WHERE state_meta['queued']['priority'] = 'high' AND tenant_secret = 'x' ORDER BY updated_at_ms DESC LIMIT 10 RETURN RECORDS";
    let projected = "FROM runs WHERE run_id = 'run-1' RETURN RECORD (lease_token)";
    let duplicate = "FROM runs WHERE run_id = 'run-1' RETURN RECORD (state, STATE)";
    let missing_where = "EXPLAIN FROM runs RETURN RECORDS";
    for (query, failure) in [
        (field, (Reason::UnsupportedField, 17)),
        (source, (Reason::UnsupportedSource, 14)),
        (
            later,
            (Reason::UnsupportedField, at(later, "tenant_secret")),
        ),
        (
            projected,
            (Reason::UnsupportedField, at(projected, "lease_token")),
        ),
        (
            duplicate,
            (Reason::DuplicateProjectionField, at(duplicate, "STATE")),
        ),
        (missing_where, (Reason::UnsupportedQueryShape, 19)),
    ] {
        assert_eq!(outcome(query), Err(failure), "{query}");
    }
}
