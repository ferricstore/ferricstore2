//! The clauses after the predicates: `RETURN RECORD`, `RETURN COUNT`, or
//! `ORDER BY ... LIMIT ... [CURSOR @c] RETURN RECORDS`, and the canonical
//! point and history shapes the native parser accepts.

use super::super::field::{Field, keyword};
use super::{
    Direction, Failure, Kind, Mode, Parsed, Predicate, Reason, Shape, Source, Stream, Value,
    ValueType, clause::parse_i64, project,
};

/// Parses the tail at the cursor.
pub(super) fn parse(
    mode: Mode,
    source: Source,
    predicates: Vec<Predicate>,
    stream: &mut Stream<'_, '_>,
) -> Result<Parsed, Failure> {
    if !stream.is(0, b"RETURN") {
        return collection(mode, source, predicates, stream);
    }
    if source == Source::Runs && stream.is(1, b"RECORD") {
        let projection = project::parse(source, stream, 2)?;
        return point(mode, predicates, projection, stream.byte(0));
    }
    if source == Source::Runs && stream.is(1, b"COUNT") {
        stream.terminator(2)?;
        return Ok(Parsed {
            mode,
            source,
            shape: Shape::Count,
            predicates,
            order_by: Vec::new(),
            limit: None,
            cursor: None,
            projection: None,
        });
    }
    Err(stream.shape(1))
}

/// `canonical_point/4`: `run_id`, or `partition_key` and `run_id`.
fn point(
    mode: Mode,
    predicates: Vec<Predicate>,
    projection: Option<Vec<super::Selector>>,
    error_byte: usize,
) -> Result<Parsed, Failure> {
    let shape = || Failure::at(Reason::UnsupportedQueryShape, error_byte);
    let mut partition = None;
    let mut run_id = None;
    let single = predicates.len() == 1;
    if predicates.len() > 2 {
        return Err(shape());
    }
    for predicate in predicates {
        match predicate {
            Predicate::Eq(Field::PartitionKey, value) if !single => partition = Some(value),
            Predicate::Eq(Field::RunId, value) => run_id = Some(value),
            _ => return Err(shape()),
        }
    }
    let run_id = run_id.ok_or_else(shape)?;
    let mut predicates = Vec::with_capacity(2);
    match (single, partition) {
        (true, _) => {}
        (false, Some(partition)) => predicates.push(Predicate::Eq(Field::PartitionKey, partition)),
        (false, None) => return Err(shape()),
    }
    predicates.push(Predicate::Eq(Field::RunId, run_id));
    Ok(Parsed {
        mode,
        source: Source::Runs,
        shape: Shape::Point,
        predicates,
        order_by: Vec::new(),
        limit: Some(1),
        cursor: None,
        projection,
    })
}

fn order(stream: &Stream<'_, '_>) -> Result<(Vec<(Field, Direction)>, usize), Failure> {
    if !stream.is(0, b"ORDER") {
        return Err(stream.shape(0));
    }
    if !stream.is(1, b"BY") {
        return Err(stream.shape(1));
    }
    let mut offset = 2_usize;
    let mut order_by = Vec::with_capacity(2);
    loop {
        if order_by.len() >= 2 {
            return Err(stream.shape(offset));
        }
        let word = stream.word(offset).ok_or_else(|| stream.shape(offset))?;
        let field =
            Field::parse(word).ok_or_else(|| stream.fail(Reason::UnsupportedField, offset))?;
        let direction_at = offset.saturating_add(1);
        let direction = if stream.is(direction_at, b"ASC") {
            Direction::Asc
        } else if stream.is(direction_at, b"DESC") {
            Direction::Desc
        } else {
            return Err(stream.shape(direction_at));
        };
        order_by.push((field, direction));
        offset = offset.saturating_add(2);
        if stream.kind(offset) == Some(&Kind::Comma) {
            offset = offset.saturating_add(1);
        } else {
            return Ok((order_by, offset));
        }
    }
}

/// `ORDER BY ... LIMIT n [CURSOR @c] RETURN RECORDS [(fields)]`.
fn collection(
    mode: Mode,
    source: Source,
    predicates: Vec<Predicate>,
    stream: &Stream<'_, '_>,
) -> Result<Parsed, Failure> {
    let (order_by, mut offset) = order(stream)?;
    if !stream.is(offset, b"LIMIT") {
        return Err(stream.shape(offset));
    }
    let limit_at = offset.saturating_add(1);
    let limit = match stream.kind(limit_at) {
        Some(Kind::Integer(raw)) => parse_i64(raw)
            .filter(|limit| (1..=100).contains(limit))
            .and_then(|limit| usize::try_from(limit).ok())
            .ok_or_else(|| stream.shape(limit_at))?,
        _ => return Err(stream.shape(limit_at)),
    };
    offset = offset.saturating_add(2);
    let cursor = if stream.is(offset, b"CURSOR") {
        if mode != Mode::Execute {
            return Err(stream.fail(Reason::QueryCursorInvalid, offset));
        }
        let name_at = offset.saturating_add(1);
        let Some(Kind::Parameter(name)) = stream.kind(name_at) else {
            return Err(stream.shape(name_at));
        };
        offset = offset.saturating_add(2);
        Some(Value::Parameter(
            ValueType::Keyword,
            String::from_utf8_lossy(name).into_owned(),
        ))
    } else {
        None
    };
    if !stream.is(offset, b"RETURN") {
        return Err(stream.shape(offset));
    }
    if !stream.is(offset.saturating_add(1), b"RECORDS") {
        return Err(stream.shape(offset.saturating_add(1)));
    }
    let projection = project::parse(source, stream, offset.saturating_add(2))?;
    let parsed = Parsed {
        mode,
        source,
        shape: Shape::Collection,
        predicates,
        order_by,
        limit: Some(limit),
        cursor,
        projection,
    };
    match source {
        Source::Runs => Ok(parsed),
        Source::Events => history(parsed, stream.byte(offset)),
    }
}

/// `canonical_history/7`: `run_id` and at most one `partition_key`, ordered
/// by `event_id` alone.
fn history(mut parsed: Parsed, error_byte: usize) -> Result<Parsed, Failure> {
    let shape = || Failure::at(Reason::UnsupportedQueryShape, error_byte);
    if !matches!(parsed.order_by.as_slice(), [(Field::EventId, _)]) {
        return Err(shape());
    }
    let mut partitions = 0_usize;
    let mut runs = 0_usize;
    for predicate in &parsed.predicates {
        match predicate {
            Predicate::Eq(Field::PartitionKey, _) => partitions = partitions.saturating_add(1),
            Predicate::Eq(Field::RunId, _) => runs = runs.saturating_add(1),
            _ => return Err(shape()),
        }
    }
    if runs != 1 || partitions > 1 {
        return Err(shape());
    }
    parsed.shape = Shape::History;
    Ok(parsed)
}

/// Whether `word` is the keyword `expected`, for the projection parser.
pub(super) fn is_word(word: Option<&[u8]>, expected: &[u8]) -> bool {
    word.is_some_and(|word| keyword(word, expected))
}
