//! The maps a query answers with: `ferric.flow.query.result/v1` records or
//! a count, and `ferric.flow.explain/v1` plans, with the quality and usage
//! the SDK checks. This node reads its in-memory catalog, so results are
//! authoritative and current; its plans describe that catalog, not Elixir's
//! index costs.

use std::fmt::Write as _;

use sha2::{Digest, Sha256};

use super::super::term::{Term, map};
use super::Query;
use super::field::Field;
use super::fql::{Direction, Mode, Predicate, Shape, Source};

const RESULT_CONTRACT: &str = "ferric.flow.query.result/v1";
const EXPLAIN_CONTRACT: &str = "ferric.flow.explain/v1";

/// What a query read.
pub struct Usage {
    scanned: u64,
    results: u64,
    checks: u64,
}

fn count_of(value: usize) -> u64 {
    u64::try_from(value).unwrap_or(u64::MAX)
}

impl Usage {
    /// `scanned` rows read, `results` returned, `checks` predicate tests.
    #[must_use]
    pub fn new(scanned: usize, results: usize, checks: usize) -> Self {
        Self {
            scanned: count_of(scanned),
            results: count_of(results),
            checks: count_of(checks),
        }
    }

    fn term(&self) -> Term {
        map(vec![
            ("duplicate_entries", Term::Int(0)),
            ("hydrated_records", Term::Int(0)),
            ("memory_high_water_bytes", Term::Int(0)),
            ("range_pages", Term::int(u64::from(self.scanned > 0))),
            ("range_seeks", Term::Int(1)),
            ("residual_checks", Term::int(self.checks)),
            ("response_bytes", Term::Int(0)),
            ("result_records", Term::int(self.results)),
            ("scanned_bytes", Term::Int(0)),
            ("scanned_entries", Term::int(self.scanned)),
            ("wall_time_us", Term::Int(0)),
        ])
    }
}

/// The page quality of a query's shape.
fn quality(query: &Query, has_more: bool) -> Term {
    let pagination = match (query.shape, query.source) {
        (Shape::Point | Shape::Count, _) => "none",
        (_, Source::Events) if has_more => "authenticated_seek",
        (_, Source::Events) => "complete",
        _ => "live_seek",
    };
    map(vec![
        ("coverage", Term::text("complete")),
        ("exactness", Term::text("authoritative")),
        ("freshness", Term::text("current")),
        ("pagination", Term::text(pagination)),
    ])
}

/// Puts `value` at `path` in a sorted map, creating nested maps.
pub(super) fn put_path(entries: &mut Vec<(Vec<u8>, Term)>, path: &[&[u8]], value: Term) {
    let Some((first, rest)) = path.split_first() else {
        return;
    };
    let position = entries.iter().position(|(key, _)| key.as_slice() == *first);
    if rest.is_empty() {
        match position.and_then(|index| entries.get_mut(index)) {
            Some(entry) => entry.1 = value,
            None => entries.push((first.to_vec(), value)),
        }
    } else {
        let index = position.unwrap_or_else(|| {
            entries.push((first.to_vec(), Term::Map(Vec::new())));
            entries.len().saturating_sub(1)
        });
        if let Some((_, Term::Map(nested))) = entries.get_mut(index) {
            put_path(nested, rest, value);
            nested.sort_by(|left, right| left.0.cmp(&right.0));
        }
    }
    entries.sort_by(|left, right| left.0.cmp(&right.0));
}

/// A page of records.
#[must_use]
pub fn records(query: &Query, records: Vec<Term>, next: Option<Vec<u8>>, usage: &Usage) -> Term {
    let has_more = next.is_some();
    let page = map(vec![
        ("cursor", next.map_or(Term::Nil, Term::Bin)),
        ("has_more", Term::Bool(has_more)),
    ]);
    let quality = quality(query, has_more);
    if query.mode != Mode::Execute {
        return explain(query, Some(usage));
    }
    map(vec![
        ("page", page),
        ("quality", quality),
        ("records", Term::List(records)),
        ("usage", usage.term()),
        ("version", Term::text(RESULT_CONTRACT)),
    ])
}

/// A count.
#[must_use]
pub fn count(query: &Query, count: usize, usage: &Usage) -> Term {
    if query.mode != Mode::Execute {
        return explain(query, Some(usage));
    }
    map(vec![
        ("quality", quality(query, false)),
        (
            "result",
            map(vec![
                ("kind", Term::text("count")),
                ("value", Term::int(count_of(count))),
            ]),
        ),
        ("usage", usage.term()),
        ("version", Term::text(RESULT_CONTRACT)),
    ])
}

fn path(query: &Query) -> &'static str {
    match (query.shape, query.source) {
        (Shape::Point, _) => "point_read",
        (_, Source::Events) => "history_read",
        (Shape::Count, _) => "catalog_count",
        _ => "catalog_filter",
    }
}

fn operator(predicate: &Predicate) -> &'static str {
    match predicate {
        Predicate::Eq(..) => "eq",
        Predicate::In(..) => "in",
        Predicate::Range(..) => "range",
        Predicate::TimeWindow(..) => "time_window",
        Predicate::IsNull(_) => "is_null",
        Predicate::IsMissing(_) => "is_missing",
    }
}

/// Fields the catalog narrows by: partition, type and state equalities.
fn constrained(predicate: &Predicate) -> bool {
    matches!(
        predicate,
        Predicate::Eq(
            Field::PartitionKey | Field::RunId | Field::Type | Field::State,
            _
        ) | Predicate::In(Field::Type | Field::State, _)
    )
}

fn plan(query: &Query) -> Term {
    let dimension = |predicate: &Predicate| {
        map(vec![
            (
                "field",
                Term::Bin(predicate.field().external().into_bytes()),
            ),
            ("operator", Term::text(operator(predicate))),
            ("value", map(vec![("redacted", Term::Bool(true))])),
        ])
    };
    let (dimensions, residual): (Vec<&Predicate>, Vec<&Predicate>) = query
        .predicates
        .iter()
        .partition(|predicate| constrained(predicate));
    let requested = query
        .order_by
        .iter()
        .map(|(field, direction)| {
            let direction = if *direction == Direction::Asc {
                "asc"
            } else {
                "desc"
            };
            map(vec![
                ("direction", Term::text(direction)),
                ("field", Term::Bin(field.external().into_bytes())),
            ])
        })
        .collect();
    map(vec![
        (
            "constrained_dimensions",
            Term::List(dimensions.into_iter().map(dimension).collect()),
        ),
        ("fallback_reason", Term::text("none")),
        (
            "index",
            map(vec![(
                "logical_id",
                Term::text("flow_catalog_type_state_partition"),
            )]),
        ),
        ("limit", Term::optional_int(query.limit.map(count_of))),
        (
            "order",
            Term::text(if query.order_by.is_empty() {
                "none"
            } else {
                "sort"
            }),
        ),
        ("path", Term::text(path(query))),
        ("range_count", Term::Int(1)),
        ("record_source", Term::text("catalog")),
        ("requested_order", Term::List(requested)),
        (
            "residual_predicates",
            Term::List(residual.into_iter().map(dimension).collect()),
        ),
        (
            "return",
            Term::text(if query.shape == Shape::Count {
                "count"
            } else {
                "record"
            }),
        ),
    ])
}

/// `EXPLAIN` (planned) or `EXPLAIN ANALYZE` (executed, with `actual`).
fn explain(query: &Query, usage: Option<&Usage>) -> Term {
    let executed = query.mode == Mode::Analyze;
    let fingerprint =
        Sha256::digest(&query.text)
            .iter()
            .fold(String::with_capacity(64), |mut hex, byte| {
                let _written = write!(hex, "{byte:02x}");
                hex
            });
    let limit = Term::optional_int(query.limit.map(count_of));
    map(vec![
        (
            "actual",
            usage.filter(|_| executed).map_or(Term::Nil, Usage::term),
        ),
        ("alternatives", Term::List(Vec::new())),
        ("bounds", map(vec![("max_result_records", limit)])),
        (
            "decision",
            map(vec![
                ("reason", Term::text("only_plan")),
                ("selected", Term::text(path(query))),
            ]),
        ),
        ("diagnostic", Term::Nil),
        ("estimate", map(vec![("range_seeks", Term::Int(1))])),
        ("plan", plan(query)),
        ("pressure", map(vec![("state", Term::text("normal"))])),
        ("quality", quality(query, false)),
        ("query_fingerprint", Term::Bin(fingerprint.into_bytes())),
        (
            "stats",
            map(vec![
                ("source", Term::text("catalog")),
                ("state", Term::text("exact")),
            ]),
        ),
        (
            "status",
            Term::text(if executed { "executed" } else { "planned" }),
        ),
        ("version", Term::text(EXPLAIN_CONTRACT)),
    ])
}

/// A plan without running the query (`EXPLAIN`).
#[must_use]
pub fn planned(query: &Query) -> Term {
    explain(query, None)
}
