//! Running a bound query over the rows a shard supplies, as
//! `ReferenceEvaluator`, `RecordOrder` and `RecordProjection` define it:
//! predicates test a row's fields (a list attribute matches when one of its
//! values does; a nil field only matches `IS NULL`, an absent one only `IS
//! MISSING`), rows sort by their order fields with nulls and absent values
//! last and ties broken by the SHA-256 of the id, and a page resumes after
//! the cursor's sort key.

use std::cmp::Ordering;

use sha2::{Digest, Sha256};

use super::super::term::{Term, map};
use super::bind::compare;
use super::cursor::{self, Key};
use super::error::{Failure, Reason};
use super::field::{Field, Selector};
use super::fql::{Direction, Literal, Mode, Predicate, Shape, Value};
use super::{Query, answer};

/// The run fields a query row carries.
pub const ROW_FIELDS: [&str; 18] = [
    "attempts",
    "attributes",
    "correlation_id",
    "created_at_ms",
    "id",
    "lease_deadline_ms",
    "max_active_ms",
    "next_run_at_ms",
    "parent_flow_id",
    "partition_key",
    "priority",
    "root_flow_id",
    "run_state",
    "state",
    "state_meta",
    "type",
    "updated_at_ms",
    "version",
];

/// The query row of a Flow from its record map: the allowlisted fields,
/// sorted.
#[must_use]
pub fn row(record: Term) -> Term {
    let Term::Map(entries) = record else {
        return Term::Map(Vec::new());
    };
    let mut kept: Vec<(Vec<u8>, Term)> = entries
        .into_iter()
        .filter(|(key, _)| {
            ROW_FIELDS
                .iter()
                .any(|name| name.as_bytes() == key.as_slice())
        })
        .collect();
    kept.sort_by(|left, right| left.0.cmp(&right.0));
    Term::Map(kept)
}

/// The value at `field` in a row, or `None` when absent.
fn fetch<'a>(row: &'a Term, field: &Field) -> Option<&'a Term> {
    match field {
        Field::Attribute(name) => row.get(b"attributes")?.get(name.as_bytes()),
        Field::StateMeta(state, name) => row
            .get(b"state_meta")?
            .get(state.as_bytes())?
            .get(name.as_bytes()),
        builtin => row.get(builtin.record_key()?.as_bytes()),
    }
}

fn same(actual: &Term, expected: &Literal) -> bool {
    match (actual, expected) {
        (Term::Bin(actual), Literal::Keyword(expected)) => actual == expected,
        (Term::Int(actual), Literal::Integer(expected)) => actual == expected,
        (Term::Float(actual), Literal::Float(expected)) => {
            actual.partial_cmp(expected) == Some(Ordering::Equal)
        }
        (Term::Bool(actual), Literal::Boolean(expected)) => actual == expected,
        _ => false,
    }
}

fn literal(term: &Term) -> Option<Literal> {
    match term {
        Term::Bin(bytes) => Some(Literal::Keyword(bytes.clone())),
        Term::Int(number) => Some(Literal::Integer(*number)),
        Term::Float(number) => Some(Literal::Float(*number)),
        Term::Bool(flag) => Some(Literal::Boolean(*flag)),
        _ => None,
    }
}

fn within(actual: &Term, lower: &Literal, upper: &Literal, inclusive: bool) -> bool {
    let Some(actual) = literal(actual) else {
        return false;
    };
    let above = compare(&actual, lower).is_some_and(Ordering::is_ge);
    let below =
        compare(&actual, upper).is_some_and(|order| order.is_lt() || (inclusive && order.is_eq()));
    above && below
}

fn bound(value: &Value) -> Option<&Literal> {
    match value {
        Value::Literal(literal) => Some(literal),
        Value::Parameter(..) => None,
    }
}

/// The values a field holds: each of a list's, or the value.
fn held(term: &Term) -> &[Term] {
    match term {
        Term::List(items) => items,
        term => std::slice::from_ref(term),
    }
}

/// Whether `row` satisfies `predicate`.
fn matches(row: &Term, predicate: &Predicate) -> bool {
    let fetched = fetch(row, predicate.field());
    match predicate {
        Predicate::IsNull(_) => fetched == Some(&Term::Nil),
        Predicate::IsMissing(_) => fetched.is_none(),
        Predicate::Eq(_, value) => fetched
            .zip(bound(value))
            .is_some_and(|(actual, expected)| held(actual).iter().any(|item| same(item, expected))),
        Predicate::In(_, values) => fetched.is_some_and(|actual| {
            held(actual).iter().any(|item| {
                values
                    .iter()
                    .filter_map(bound)
                    .any(|expected| same(item, expected))
            })
        }),
        Predicate::Range(_, lower, upper) | Predicate::TimeWindow(_, lower, upper) => {
            let inclusive = matches!(predicate, Predicate::Range(..));
            match (fetched, bound(lower), bound(upper)) {
                (Some(actual), Some(lower), Some(upper)) => held(actual)
                    .iter()
                    .any(|item| within(item, lower, upper, inclusive)),
                _ => false,
            }
        }
    }
}

/// `TupleCodec` order bytes of one sort component.
fn component(value: Option<&Term>, direction: Direction, out: &mut Vec<u8>) {
    let ascending: Vec<u8> = match value {
        None => return out.push(0xFF),
        Some(Term::Nil) => return out.push(0xFE),
        Some(Term::Int(number)) => {
            let mut bytes = vec![0x30];
            bytes.extend_from_slice(&(number.cast_unsigned() ^ (1 << 63)).to_be_bytes());
            bytes
        }
        Some(Term::Bin(text)) => {
            let mut bytes = vec![0x40];
            bytes.extend_from_slice(text);
            bytes.push(0);
            bytes
        }
        Some(_) => vec![0x50],
    };
    match direction {
        Direction::Asc => out.extend(ascending),
        Direction::Desc => out.extend(ascending.into_iter().map(|byte| !byte)),
    }
}

/// A row's sort key: its order components, then the SHA-256 of its id.
fn sort_key(row: &Term, order_by: &[(Field, Direction)]) -> Vec<u8> {
    let mut key = Vec::with_capacity(48);
    for (field, direction) in order_by {
        component(fetch(row, field), *direction, &mut key);
    }
    key.push(0x60);
    let id = match row.get(b"id") {
        Some(Term::Bin(id)) => id.as_slice(),
        _ => &[],
    };
    key.extend_from_slice(&Sha256::digest(id));
    key
}

/// A row reduced to the projected selectors, or the whole row.
pub(super) fn project(row: Term, projection: Option<&[Selector]>) -> Term {
    let Some(selectors) = projection else {
        return row;
    };
    let mut out: Vec<(Vec<u8>, Term)> = Vec::new();
    let mut put = |path: &[&[u8]], value: Option<&Term>| {
        if let Some(value) = value {
            answer::put_path(&mut out, path, value.clone());
        }
    };
    for selector in selectors {
        match selector {
            Selector::Field(Field::Attribute(name)) => put(
                &[b"attributes", name.as_bytes()],
                fetch(&row, &Field::Attribute(name.clone())),
            ),
            Selector::Field(field @ Field::StateMeta(state, name)) => {
                put(
                    &[b"state_meta", state.as_bytes(), name.as_bytes()],
                    fetch(&row, field),
                );
            }
            Selector::Field(field) => {
                let key = field.record_key().unwrap_or_default();
                put(&[key.as_bytes()], row.get(key.as_bytes()));
            }
            Selector::Attributes => put(&[b"attributes"], row.get(b"attributes")),
            Selector::StateMeta => put(&[b"state_meta"], row.get(b"state_meta")),
            Selector::EventId => put(&[b"event_id"], row.get(b"event_id")),
            Selector::Fields => put(&[b"fields"], row.get(b"fields")),
            Selector::EventField(name) => put(
                &[b"fields", name.as_bytes()],
                row.get(b"fields")
                    .and_then(|fields| fields.get(name.as_bytes())),
            ),
        }
    }
    out.sort_by(|left, right| left.0.cmp(&right.0));
    Term::Map(out)
}

/// What the node supplies for one run query.
pub struct Runs {
    /// Candidate rows: every Flow the query's scope could match.
    pub rows: Vec<Term>,
    /// The node's cursor key.
    pub key: Key,
    /// Wall time.
    pub now_ms: u64,
}

/// Answers a run query (point, collection or count) over `input`.
///
/// # Errors
/// An invalid or expired cursor.
pub fn runs(query: &Query, input: Runs) -> Result<Term, Failure> {
    let scanned = input.rows.len();
    let after = match &query.cursor {
        Some(token) => Some(
            cursor::open(&input.key, &query.digest, input.now_ms, token).map_err(Failure::new)?,
        ),
        None => None,
    };
    let predicate_count = query.predicates.len();
    let matching: Vec<Term> = input
        .rows
        .into_iter()
        .filter(|row| {
            query
                .predicates
                .iter()
                .all(|predicate| matches(row, predicate))
        })
        .collect();
    let checks = scanned.saturating_mul(predicate_count.min(12));
    if query.shape == Shape::Count {
        let usage = answer::Usage::new(scanned.max(1), 1, checks);
        return Ok(answer::count(query, matching.len(), &usage));
    }
    let mut keyed: Vec<(Vec<u8>, Term)> = matching
        .into_iter()
        .map(|row| (sort_key(&row, &query.order_by), row))
        .filter(|(key, _)| after.as_ref().is_none_or(|after| key > after))
        .collect();
    keyed.sort_by(|left, right| left.0.cmp(&right.0));
    let limit = query.limit.unwrap_or(1);
    let has_more = keyed.len() > limit;
    keyed.truncate(limit);
    let next = if has_more && query.mode == Mode::Execute {
        keyed
            .last()
            .map(|(key, _)| cursor::issue(&input.key, &query.digest, input.now_ms, key))
    } else {
        None
    };
    let records: Vec<Term> = keyed
        .into_iter()
        .map(|(_, row)| project(row, query.projection.as_deref()))
        .collect();
    let usage = answer::Usage::new(scanned.max(records.len()), records.len(), checks);
    Ok(answer::records(query, records, next, &usage))
}

/// One history event as a query row: `event_id` and `fields`.
pub struct Event {
    /// The event's time and version, its order.
    pub order: (u64, u64),
    /// The row.
    pub row: Term,
}

/// Answers an event query over one Flow's `events`, oldest first.
///
/// # Errors
/// An invalid or expired cursor.
pub fn events(
    query: &Query,
    mut events: Vec<Event>,
    key: &Key,
    now_ms: u64,
) -> Result<Term, Failure> {
    let scanned = events.len();
    let descending = matches!(query.order_by.first(), Some((_, Direction::Desc)));
    if descending {
        events.reverse();
    }
    let encode = |order: (u64, u64)| {
        let mut bytes = order.0.to_be_bytes().to_vec();
        bytes.extend_from_slice(&order.1.to_be_bytes());
        bytes
    };
    if let Some(token) = &query.cursor {
        let after = cursor::open(key, &query.digest, now_ms, token).map_err(Failure::new)?;
        let position = events
            .iter()
            .position(|event| encode(event.order) == after)
            .ok_or(Failure::new(Reason::QueryCursorInvalid))?;
        events.drain(..=position);
    }
    let limit = query.limit.unwrap_or(1);
    let has_more = events.len() > limit;
    events.truncate(limit);
    let next = if has_more && query.mode == Mode::Execute {
        events
            .last()
            .map(|event| cursor::issue(key, &query.digest, now_ms, &encode(event.order)))
    } else {
        None
    };
    let records: Vec<Term> = events
        .into_iter()
        .map(|event| project(event.row, query.projection.as_deref()))
        .collect();
    let usage = answer::Usage::new(scanned.max(records.len()), records.len(), 0);
    Ok(answer::records(query, records, next, &usage))
}

/// The row of one event: its id and its fields.
#[must_use]
pub fn event_row(event_id: Vec<u8>, fields: Term) -> Term {
    map(vec![("event_id", Term::Bin(event_id)), ("fields", fields)])
}
