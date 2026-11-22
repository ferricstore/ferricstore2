//! Elixir's `reference_parser_test.exs`, its `bind/2` cases: parameters
//! bind by name, repeated or mixed with literals; missing, mistyped and
//! unused ones are refused; bound partition and run ids stop at the
//! storage key's bounds. (Its hand-built malformed requests have no wire
//! form.)

use super::super::term::Term;
use super::Reason;
use super::tests::{reason, request, text};

const POINT: &str =
    "FROM runs WHERE partition_key = @partition AND run_id = @flow_id RETURN RECORD";

#[test]
fn parameters_bind_by_name_repeated_or_beside_literals() {
    // Elixir: "binds a named keyword parameter" and "binds repeated,
    // mixed, and literal-only values exactly".
    let named = vec![
        ("partition", text("tenant-a")),
        ("flow_id", text("run-123")),
    ];
    assert!(request(POINT, named).is_ok());
    let repeated = "FROM runs WHERE partition_key = @id AND run_id = @id RETURN RECORD";
    assert!(request(repeated, vec![("id", text("same"))]).is_ok());
    let mixed = "FROM runs WHERE partition_key = 'tenant-a' AND run_id = @id RETURN RECORD";
    assert!(request(mixed, vec![("id", text("run-123"))]).is_ok());
    let literals =
        "FROM runs WHERE partition_key = 'tenant-a' AND run_id = 'run-123' RETURN RECORD";
    assert!(request(literals, Vec::new()).is_ok());
    assert_eq!(
        reason(literals, vec![("unused", text("secret"))]),
        Some(Reason::UnexpectedParameter)
    );
}

#[test]
fn missing_mistyped_and_unused_parameters_are_refused() {
    // Elixir: "fails closed on missing, mistyped, and unused parameters"
    // (its atom-keyed map has no wire form).
    assert_eq!(reason(POINT, Vec::new()), Some(Reason::MissingParameter));
    assert_eq!(
        reason(
            POINT,
            vec![("partition", text("tenant-a")), ("flow_id", Term::Int(123))]
        ),
        Some(Reason::InvalidParameterType)
    );
    assert_eq!(
        reason(
            POINT,
            vec![
                ("partition", text("tenant-a")),
                ("flow_id", text("run-123")),
                ("unused", text("secret")),
            ]
        ),
        Some(Reason::UnexpectedParameter)
    );
}

#[test]
fn bound_ids_stop_at_the_storage_key_bounds() {
    // Elixir: "rejects oversized bound values before planning" and
    // "accepts exact storage-key boundaries and rejects the next byte"
    // (keys of at most 65535 bytes; a run id leaves room for the 52-byte
    // state key around it).
    let max_partition = "p".repeat(65_535);
    let max_run_id = "r".repeat(65_535 - 52);
    let bind = |partition: &str, run_id: &str| {
        reason(
            POINT,
            vec![("partition", text(partition)), ("flow_id", text(run_id))],
        )
    };
    assert_eq!(bind(&max_partition, "r"), None);
    assert_eq!(bind("p", &max_run_id), None);
    assert_eq!(
        bind(&format!("{max_partition}p"), "r"),
        Some(Reason::QueryValueTooLarge)
    );
    assert_eq!(
        bind("p", &format!("{max_run_id}r")),
        Some(Reason::QueryValueTooLarge)
    );
}

#[test]
fn identity_fields_take_no_empty_value() {
    // `request_test.exs`: "dynamic metadata allows empty strings without
    // allowing empty identity fields" (its metadata half is in
    // `validation_tests::metadata_allows_empty_text_but_bounds_its_size`).
    assert_eq!(
        reason(
            POINT,
            vec![("partition", text("")), ("flow_id", text("run-1"))]
        ),
        Some(Reason::InvalidParameterType)
    );
    assert_eq!(
        reason(
            POINT,
            vec![("partition", text("tenant-a")), ("flow_id", text(""))]
        ),
        Some(Reason::InvalidParameterType)
    );
}

#[test]
fn ranges_keep_one_type_and_run_forward() {
    // `reference_evaluator_test.exs`: "rejects unbounded execution and
    // invalid mixed-type ranges", "accepts mixed literal and parameter
    // bounds before binding" and "rejects reversed literal ranges before
    // execution" (Elixir's `invalid_predicate_range` has no wire code; it
    // answers as invalid syntax).
    let query = |predicate: &str| {
        format!(
            "FROM runs WHERE partition_key = 'tenant-a' AND {predicate} ORDER BY updated_at_ms ASC LIMIT 10 RETURN RECORDS"
        )
    };
    assert_eq!(
        reason(&query("priority BETWEEN 1 AND '9'"), Vec::new()),
        Some(Reason::InvalidParameterType)
    );
    assert!(
        request(
            &query("updated_at_ms BETWEEN 1 AND @until"),
            vec![("until", Term::Int(5))]
        )
        .is_ok()
    );
    for reversed in [
        "updated_at_ms BETWEEN 20 AND 10",
        "updated_at_ms FROM 20 TO 10",
    ] {
        assert_eq!(
            reason(&query(reversed), Vec::new()),
            Some(Reason::InvalidSyntax),
            "{reversed}"
        );
    }
}
