//! Checking a parsed query and binding its parameters, as `Request.validate`
//! and `Binder.bind/2` do: every named parameter must be supplied and used,
//! values must have their field's type and size, collections and counts need
//! exactly one `partition_key =` predicate, and order fields are distinct
//! integer fields.

use std::cmp::Ordering;

use super::super::term::Term;
use super::error::Reason;
use super::field::{Field, ValueType};
use super::fql::{Literal, Parsed, Predicate, Shape, Value};

/// Most named parameters.
const MAX_PARAMETERS: usize = 64;
/// Shortest and longest cursor tokens.
const MIN_CURSOR_BYTES: usize = 16;
const MAX_CURSOR_BYTES: usize = 4_096;
/// Longest key, partition included.
const MAX_KEY_BYTES: usize = 65_535;
/// What a partitioned state key adds to a run id: `f:{f:<43>}:s:`.
const STATE_KEY_OVERHEAD: usize = 52;
/// Longest attribute or state metadata text.
const MAX_METADATA_VALUE_BYTES: usize = 256;
/// Longest text for any other field.
const MAX_TEXT_BYTES: usize = 1_024;

const TIME_FIELDS: [Field; 4] = [
    Field::CreatedAtMs,
    Field::UpdatedAtMs,
    Field::NextRunAtMs,
    Field::LeaseDeadlineMs,
];

/// Binds `params` into `parsed`, checking it before and after.
///
/// # Errors
/// The first failing check's reason.
pub fn bind(mut parsed: Parsed, params: &[(Vec<u8>, Term)]) -> Result<Parsed, Reason> {
    validate(&parsed, false)?;
    let names = names(&parsed)?;
    if params.len() > names.len() {
        return Err(Reason::UnexpectedParameter);
    }
    if names.iter().any(|name| {
        !params
            .iter()
            .any(|(key, _)| key.as_slice() == name.as_bytes())
    }) {
        return Err(Reason::MissingParameter);
    }
    let lookup = |name: &str| {
        params
            .iter()
            .find(|(key, _)| key.as_slice() == name.as_bytes())
            .map(|(_, value)| value)
    };
    for predicate in &mut parsed.predicates {
        for value in values_mut(predicate) {
            *value = bound(value, &lookup)?;
        }
    }
    if let Some(cursor) = &mut parsed.cursor {
        *cursor = bound(cursor, &lookup)?;
    }
    validate(&parsed, true)?;
    Ok(parsed)
}

fn values(predicate: &Predicate) -> Vec<&Value> {
    match predicate {
        Predicate::Eq(_, value) => vec![value],
        Predicate::In(_, values) => values.iter().collect(),
        Predicate::Range(_, lower, upper) | Predicate::TimeWindow(_, lower, upper) => {
            vec![lower, upper]
        }
        Predicate::IsNull(_) | Predicate::IsMissing(_) => Vec::new(),
    }
}

fn values_mut(predicate: &mut Predicate) -> Vec<&mut Value> {
    match predicate {
        Predicate::Eq(_, value) => vec![value],
        Predicate::In(_, values) => values.iter_mut().collect(),
        Predicate::Range(_, lower, upper) | Predicate::TimeWindow(_, lower, upper) => {
            vec![lower, upper]
        }
        Predicate::IsNull(_) | Predicate::IsMissing(_) => Vec::new(),
    }
}

/// Each parameter and the type its field declares, in order of first use:
/// how text values decode (audit gap G39).
pub(super) fn declared(parsed: &Parsed) -> Vec<(String, ValueType)> {
    let mut found: Vec<(String, ValueType)> = Vec::new();
    let used = parsed
        .predicates
        .iter()
        .flat_map(values)
        .chain(parsed.cursor.as_ref());
    for value in used {
        if let Value::Parameter(kind, name) = value
            && !found.iter().any(|(seen, _)| seen == name)
        {
            found.push((name.clone(), *kind));
        }
    }
    found
}

/// The parameter names in order of first use.
fn names(parsed: &Parsed) -> Result<Vec<String>, Reason> {
    let mut names: Vec<String> = Vec::new();
    let used = parsed
        .predicates
        .iter()
        .flat_map(values)
        .chain(parsed.cursor.as_ref());
    for value in used {
        if let Value::Parameter(_, name) = value
            && !names.contains(name)
        {
            names.push(name.clone());
        }
    }
    if names.len() > MAX_PARAMETERS {
        return Err(Reason::UnsupportedQueryShape);
    }
    Ok(names)
}

/// A parameter's value as a literal of its declared type.
fn bound<'a>(value: &Value, lookup: &impl Fn(&str) -> Option<&'a Term>) -> Result<Value, Reason> {
    let Value::Parameter(declared, name) = value else {
        return Ok(value.clone());
    };
    let supplied = lookup(name).ok_or(Reason::MissingParameter)?;
    let (literal, actual) = match supplied {
        Term::Bin(bytes) => (Literal::Keyword(bytes.clone()), Some(ValueType::Keyword)),
        Term::Int(number) => (Literal::Integer(*number), Some(ValueType::Integer)),
        Term::Float(number) => (Literal::Float(*number), None),
        Term::Bool(flag) => (Literal::Boolean(*flag), None),
        _ => return Err(Reason::InvalidParameterType),
    };
    if *declared == ValueType::Dynamic || actual == Some(*declared) {
        Ok(Value::Literal(literal))
    } else {
        Err(Reason::InvalidParameterType)
    }
}

/// `Request.validate/2`, for parameters (`bound` false) or literals.
fn validate(parsed: &Parsed, bound: bool) -> Result<(), Reason> {
    match parsed.shape {
        Shape::Point => {
            for predicate in &parsed.predicates {
                if let Predicate::Eq(field, value) = predicate {
                    comparison(field, value, bound)?;
                }
            }
        }
        Shape::History => {
            for predicate in &parsed.predicates {
                if let Predicate::Eq(field, value) = predicate {
                    comparison(field, value, bound)?;
                }
            }
            cursor(parsed.cursor.as_ref(), bound)?;
        }
        Shape::Count => {
            predicates(&parsed.predicates, bound)?;
            tenant_scope(&parsed.predicates)?;
        }
        Shape::Collection => {
            predicates(&parsed.predicates, bound)?;
            tenant_scope(&parsed.predicates)?;
            order(&parsed.order_by)?;
            cursor(parsed.cursor.as_ref(), bound)?;
        }
    }
    Ok(())
}

fn predicates(predicates: &[Predicate], bound: bool) -> Result<(), Reason> {
    for predicate in predicates {
        let field = predicate.field();
        let checked = match predicate {
            Predicate::TimeWindow(..) if !TIME_FIELDS.contains(field) => {
                Err(Reason::UnsupportedQueryShape)
            }
            _ if !field.is_valid() => Err(Reason::UnsupportedField),
            Predicate::Eq(_, value) => comparison(field, value, bound),
            Predicate::In(_, values) => in_values(field, values, bound),
            Predicate::Range(_, lower, upper) | Predicate::TimeWindow(_, lower, upper) => {
                range(field, lower, upper, bound)
            }
            Predicate::IsNull(_) | Predicate::IsMissing(_) => Ok(()),
        };
        checked?;
    }
    Ok(())
}

fn in_values(field: &Field, values: &[Value], bound: bool) -> Result<(), Reason> {
    for value in values {
        comparison(field, value, bound)?;
    }
    let distinct = values
        .iter()
        .enumerate()
        .all(|(index, value)| !values.iter().take(index).any(|earlier| earlier == value));
    // Elixir's `duplicate_predicate_value` has no code: invalid syntax.
    if distinct {
        Ok(())
    } else {
        Err(Reason::InvalidSyntax)
    }
}

fn range(field: &Field, lower: &Value, upper: &Value, bound: bool) -> Result<(), Reason> {
    comparison(field, lower, bound)?;
    comparison(field, upper, bound)?;
    if kind(lower) != kind(upper) {
        return Err(Reason::InvalidParameterType);
    }
    if let (Value::Literal(lower), Value::Literal(upper)) = (lower, upper) {
        // Elixir's `invalid_predicate_range` has no code: invalid syntax.
        if compare(lower, upper) == Some(Ordering::Greater) {
            return Err(Reason::InvalidSyntax);
        }
    }
    Ok(())
}

/// The type tag a value carries: a literal's own, a parameter's declared.
fn kind(value: &Value) -> u8 {
    match value {
        Value::Literal(Literal::Keyword(_)) | Value::Parameter(ValueType::Keyword, _) => 0,
        Value::Literal(Literal::Integer(_)) | Value::Parameter(ValueType::Integer, _) => 1,
        Value::Literal(Literal::Float(_)) => 2,
        Value::Literal(Literal::Boolean(_)) => 3,
        Value::Parameter(ValueType::Dynamic, _) => 4,
    }
}

/// Two literals of one type in order.
pub(super) fn compare(left: &Literal, right: &Literal) -> Option<Ordering> {
    match (left, right) {
        (Literal::Keyword(left), Literal::Keyword(right)) => Some(left.cmp(right)),
        (Literal::Integer(left), Literal::Integer(right)) => Some(left.cmp(right)),
        (Literal::Float(left), Literal::Float(right)) => left.partial_cmp(right),
        (Literal::Boolean(left), Literal::Boolean(right)) => Some(left.cmp(right)),
        _ => None,
    }
}

/// A value checked against its field: type, emptiness and size.
fn comparison(field: &Field, value: &Value, bound: bool) -> Result<(), Reason> {
    let literal = match value {
        Value::Parameter(_, name) if !bound && !name.is_empty() && name.len() <= 128 => {
            return Ok(());
        }
        Value::Parameter(..) => return Err(Reason::InvalidParameterType),
        Value::Literal(literal) => literal,
    };
    let expected = field.value_type();
    let typed = match literal {
        Literal::Keyword(_) => expected != ValueType::Integer,
        Literal::Integer(_) => expected != ValueType::Keyword,
        Literal::Float(number) => expected == ValueType::Dynamic && number.is_finite(),
        Literal::Boolean(_) => expected == ValueType::Dynamic,
    };
    if !typed {
        return Err(Reason::InvalidParameterType);
    }
    let Literal::Keyword(text) = literal else {
        return Ok(());
    };
    if text.is_empty() && expected == ValueType::Keyword {
        return Err(Reason::InvalidParameterType);
    }
    let limit = match field {
        Field::PartitionKey => MAX_KEY_BYTES,
        Field::RunId => MAX_KEY_BYTES.saturating_sub(STATE_KEY_OVERHEAD),
        Field::Attribute(_) | Field::StateMeta(..) => MAX_METADATA_VALUE_BYTES,
        _ => MAX_TEXT_BYTES,
    };
    if text.len() > limit {
        Err(Reason::QueryValueTooLarge)
    } else {
        Ok(())
    }
}

/// `require_tenant_scope/1`: one partition predicate, an equality.
fn tenant_scope(predicates: &[Predicate]) -> Result<(), Reason> {
    let mut partition = predicates
        .iter()
        .filter(|predicate| *predicate.field() == Field::PartitionKey);
    match (partition.next(), partition.next()) {
        (Some(Predicate::Eq(..)), None) => Ok(()),
        _ => Err(Reason::UnsupportedQueryShape),
    }
}

fn order(order_by: &[(Field, super::fql::Direction)]) -> Result<(), Reason> {
    for (index, (field, _)) in order_by.iter().enumerate() {
        if !field.is_valid() {
            return Err(Reason::UnsupportedField);
        }
        if field.is_metadata() || field.value_type() != ValueType::Integer {
            return Err(Reason::UnsupportedQueryShape);
        }
        // Elixir's `duplicate_order_field` has no code: invalid syntax.
        if order_by
            .iter()
            .take(index)
            .any(|(earlier, _)| earlier == field)
        {
            return Err(Reason::InvalidSyntax);
        }
    }
    Ok(())
}

fn cursor(cursor: Option<&Value>, bound: bool) -> Result<(), Reason> {
    match cursor {
        None => Ok(()),
        Some(Value::Parameter(ValueType::Keyword, name))
            if !bound && !name.is_empty() && name.len() <= 128 =>
        {
            Ok(())
        }
        Some(Value::Literal(Literal::Keyword(token))) if bound => {
            if token.len() > MAX_CURSOR_BYTES {
                Err(Reason::QueryCursorTooLarge)
            } else if token.len() < MIN_CURSOR_BYTES {
                Err(Reason::QueryCursorInvalid)
            } else {
                Ok(())
            }
        }
        Some(_) => Err(Reason::QueryCursorInvalid),
    }
}
