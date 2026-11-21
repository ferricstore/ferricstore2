//! Business cases of Elixir's `reference_evaluator_test.exs` and cursor
//! paging, run through this node's parser, binder and evaluator on rows
//! shaped like Elixir's fixtures.

use super::super::FlowError;
use super::super::term::Term;
use super::run::{Runs, runs};
use super::{Query, Reason, parse};

pub(super) fn text(value: &str) -> Term {
    Term::text(value)
}

pub(super) fn object(entries: Vec<(&str, Term)>) -> Term {
    Term::Map(
        entries
            .into_iter()
            .map(|(key, value)| (key.as_bytes().to_vec(), value))
            .collect(),
    )
}

pub(super) fn request(query: &str, params: Vec<(&str, Term)>) -> Result<Query, FlowError> {
    parse(&object(vec![
        ("version", text("FQL1")),
        ("query", text(query)),
        ("params", object(params)),
    ]))
}

pub(super) fn reason(query: &str, params: Vec<(&str, Term)>) -> Option<Reason> {
    match request(query, params) {
        Err(FlowError::Query(failure)) => Some(failure.reason),
        _ => None,
    }
}

/// Elixir's evaluator fixture records.
fn records() -> Vec<Term> {
    let record = |id: &str,
                  partition: &str,
                  state: &str,
                  updated: i64,
                  priority: Option<Term>,
                  attributes: Term| {
        let mut entries = vec![
            ("attributes", attributes),
            ("id", text(id)),
            ("partition_key", text(partition)),
            ("state", text(state)),
            ("type", text("invoice")),
            ("updated_at_ms", Term::Int(updated)),
        ];
        entries.extend(priority.map(|priority| ("priority", priority)));
        object(entries)
    };
    let mut first = record(
        "run-1",
        "tenant-a",
        "failed",
        110,
        Some(Term::Int(2)),
        object(vec![
            ("region", text("eu")),
            ("tags", Term::List(vec![text("urgent"), text("finance")])),
        ]),
    );
    if let Term::Map(entries) = &mut first {
        entries.push((
            b"state_meta".to_vec(),
            object(vec![("failed", object(vec![("reason", text("timeout"))]))]),
        ));
    }
    vec![
        first,
        record(
            "run-2",
            "tenant-a",
            "completed",
            120,
            Some(Term::Int(1)),
            object(vec![("region", text("us"))]),
        ),
        record(
            "run-3",
            "tenant-a",
            "failed",
            130,
            Some(Term::Nil),
            object(vec![("region", text("eu"))]),
        ),
        record(
            "run-4",
            "tenant-b",
            "failed",
            125,
            None,
            object(vec![("region", text("eu"))]),
        ),
    ]
}

/// Runs `query` over `rows` with the rows of the queried partition.
fn run(query: &str, rows: Vec<Term>) -> Result<Term, String> {
    let query = request(query, Vec::new()).map_err(|error| error.to_string())?;
    let partition = query.partition_key().map(<[u8]>::to_vec);
    let rows = rows
        .into_iter()
        .filter(|row| match (&partition, row.get(b"partition_key")) {
            (Some(partition), Some(Term::Bin(value))) => partition == value,
            _ => true,
        })
        .collect();
    runs(
        &query,
        Runs {
            rows,
            key: [3; 32],
            now_ms: 1_000,
        },
    )
    .map_err(|failure| failure.message().to_owned())
}

fn ids(result: &Term) -> Vec<String> {
    match result.get(b"records") {
        Some(Term::List(records)) => records
            .iter()
            .filter_map(|record| match record.get(b"id") {
                Some(Term::Bin(id)) => Some(String::from_utf8_lossy(id).into_owned()),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

#[test]
fn evaluates_equality_in_time_windows_and_metadata_predicates() -> Result<(), String> {
    let result = run(
        "FROM runs WHERE partition_key = 'tenant-a' AND type = 'invoice' AND state IN ('failed', 'completed') AND updated_at_ms FROM 105 TO 125 AND attribute.region = 'eu' ORDER BY updated_at_ms DESC LIMIT 10 RETURN RECORDS",
        records(),
    )?;
    assert_eq!(ids(&result), ["run-1"]);
    Ok(())
}

#[test]
fn projects_selected_fields_after_filtering_ordering_and_limiting() -> Result<(), String> {
    let result = run(
        "FROM runs WHERE partition_key = 'tenant-a' AND state = 'failed' ORDER BY updated_at_ms DESC LIMIT 1 RETURN RECORDS (run_id, state, attribute.region)",
        records(),
    )?;
    assert_eq!(
        result.get(b"records"),
        Some(&Term::List(vec![object(vec![
            ("attributes", object(vec![("region", text("eu"))])),
            ("id", text("run-3")),
            ("state", text("failed")),
        ])]))
    );
    Ok(())
}

#[test]
fn time_windows_include_the_lower_bound_and_exclude_the_upper() -> Result<(), String> {
    let rows = [("lower", 100), ("inside", 199), ("upper", 200)]
        .into_iter()
        .map(|(id, at)| {
            object(vec![
                ("id", text(id)),
                ("partition_key", text("tenant-a")),
                ("updated_at_ms", Term::Int(at)),
            ])
        })
        .collect();
    let result = run(
        "FROM runs WHERE partition_key = 'tenant-a' AND updated_at_ms FROM 100 TO 200 ORDER BY updated_at_ms ASC LIMIT 10 RETURN RECORDS",
        rows,
    )?;
    assert_eq!(ids(&result), ["lower", "inside"]);
    Ok(())
}

#[test]
fn attribute_list_equality_uses_membership() -> Result<(), String> {
    let result = run(
        "FROM runs WHERE partition_key = 'tenant-a' AND attribute.tags = 'urgent' ORDER BY updated_at_ms ASC LIMIT 10 RETURN RECORDS",
        records(),
    )?;
    assert_eq!(ids(&result), ["run-1"]);
    Ok(())
}

#[test]
fn null_and_missing_stay_distinct() -> Result<(), String> {
    let base = "FROM runs WHERE partition_key = 'tenant-a' AND priority IS";
    let tail = "ORDER BY updated_at_ms ASC LIMIT 10 RETURN RECORDS";
    assert_eq!(
        ids(&run(&format!("{base} NULL {tail}"), records())?),
        ["run-3"]
    );
    assert_eq!(
        ids(&run(&format!("{base} MISSING {tail}"), records())?).len(),
        0
    );
    let other = "FROM runs WHERE partition_key = 'tenant-b' AND priority IS MISSING ORDER BY updated_at_ms ASC LIMIT 10 RETURN RECORDS";
    assert_eq!(ids(&run(other, records())?), ["run-4"]);
    Ok(())
}

#[test]
fn nulls_and_missing_values_sort_last_with_a_stable_tie_breaker() -> Result<(), String> {
    let rows: Vec<Term> = [
        ("z", Some(Term::Int(1))),
        ("b", Some(Term::Nil)),
        ("a", None),
        ("a2", Some(Term::Int(1))),
    ]
    .into_iter()
    .map(|(id, priority)| {
        let mut entries = vec![("id", text(id)), ("partition_key", text("tenant-a"))];
        entries.extend(priority.map(|priority| ("priority", priority)));
        object(entries)
    })
    .collect();
    let query = |direction: &str| {
        format!(
            "FROM runs WHERE partition_key = 'tenant-a' ORDER BY priority {direction} LIMIT 10 RETURN RECORDS"
        )
    };
    let ascending = ids(&run(&query("ASC"), rows.clone())?);
    let descending = ids(&run(&query("DESC"), rows)?);
    // Equal priorities order by the SHA-256 of the id in both directions.
    let concrete: Vec<String> = ascending.iter().take(2).cloned().collect();
    let mut sorted = concrete.clone();
    sorted.sort_unstable();
    assert_eq!(sorted, ["a2", "z"]);
    assert_eq!(
        ascending.get(2..),
        Some(["b".to_owned(), "a".to_owned()].as_slice())
    );
    assert_eq!(descending.get(..2), Some(concrete.as_slice()));
    assert_eq!(
        descending.get(2..),
        Some(["b".to_owned(), "a".to_owned()].as_slice())
    );
    Ok(())
}

#[test]
fn a_cursor_resumes_its_own_query_after_the_last_row() -> Result<(), String> {
    let rows: Vec<Term> = (1..=3)
        .map(|index| {
            object(vec![
                ("id", text(&format!("r{index}"))),
                ("partition_key", text("p")),
                ("updated_at_ms", Term::Int(index)),
            ])
        })
        .collect();
    let base = "FROM runs WHERE partition_key = 'p' ORDER BY updated_at_ms ASC LIMIT 2";
    let first = run(&format!("{base} RETURN RECORDS"), rows.clone())?;
    assert_eq!(ids(&first), ["r1", "r2"]);
    let Some(Term::Bin(cursor)) = first
        .get(b"page")
        .and_then(|page| page.get(b"cursor"))
        .cloned()
    else {
        return Err("no cursor".to_owned());
    };
    let resumed = request(
        &format!("{base} CURSOR @c RETURN RECORDS"),
        vec![("c", Term::Bin(cursor.clone()))],
    )
    .map_err(|error| error.to_string())?;
    let next = runs(
        &resumed,
        Runs {
            rows: rows.clone(),
            key: [3; 32],
            now_ms: 1_000,
        },
    )
    .map_err(|failure| failure.message().to_owned())?;
    assert_eq!(ids(&next), ["r3"]);
    assert_eq!(
        next.get(b"page").and_then(|page| page.get(b"has_more")),
        Some(&Term::Bool(false))
    );
    let other = request(
        "FROM runs WHERE partition_key = 'p' ORDER BY updated_at_ms DESC LIMIT 2 CURSOR @c RETURN RECORDS",
        vec![("c", Term::Bin(cursor))],
    )
    .map_err(|error| error.to_string())?;
    assert_eq!(
        runs(
            &other,
            Runs {
                rows,
                key: [3; 32],
                now_ms: 1_000
            }
        )
        .err()
        .map(|failure| failure.reason),
        Some(Reason::QueryCursorInvalid)
    );
    Ok(())
}
