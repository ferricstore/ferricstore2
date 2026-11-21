//! Predicates: `field = value`, `IN (...)`, `IS NULL|MISSING`, `BETWEEN`
//! and `FROM ... TO`, bracketed `attribute['..']` and `state_meta['..']['..']`
//! fields, and literal integers within 64 bits.

use super::super::field::{Field, keyword, valid_key, valid_state_name};
use super::{
    Failure, Kind, Literal, MAX_IN_VALUES, MAX_PREDICATES, Predicate, Reason, Stream, Value,
};

pub(super) fn predicates(stream: &mut Stream<'_, '_>) -> Result<Vec<Predicate>, Failure> {
    let mut predicates = Vec::with_capacity(4);
    loop {
        let (predicate, consumed) = predicate(stream)?;
        predicates.push(predicate);
        stream.skip(consumed);
        if !stream.is(0, b"AND") {
            return Ok(predicates);
        }
        stream.skip(1);
        if predicates.len() == MAX_PREDICATES {
            return Err(stream.shape(0));
        }
    }
}

/// One predicate at the cursor and how many tokens it took.
fn predicate(stream: &Stream<'_, '_>) -> Result<(Predicate, usize), Failure> {
    let (field, at) = field_tokens(stream, 0)?;
    match stream.kind(at) {
        Some(Kind::Equals) => {
            let value = value(stream, at.saturating_add(1), &field)?;
            Ok((Predicate::Eq(field, value), at.saturating_add(2)))
        }
        Some(Kind::Word(word)) if keyword(word, b"IN") => in_list(stream, field, at),
        Some(Kind::Word(word)) if keyword(word, b"IS") => {
            let next = at.saturating_add(1);
            let predicate = if stream.is(next, b"NULL") {
                Predicate::IsNull(field)
            } else if stream.is(next, b"MISSING") {
                Predicate::IsMissing(field)
            } else {
                return Err(stream.shape(next));
            };
            Ok((predicate, at.saturating_add(2)))
        }
        Some(Kind::Word(word)) if keyword(word, b"BETWEEN") || keyword(word, b"FROM") => {
            range(stream, field, word, at)
        }
        _ => Err(stream.shape(at)),
    }
}

fn in_list(
    stream: &Stream<'_, '_>,
    field: Field,
    at: usize,
) -> Result<(Predicate, usize), Failure> {
    let open = at.saturating_add(1);
    if stream.kind(open) != Some(&Kind::LeftParen) {
        return Err(stream.shape(open));
    }
    let mut offset = open.saturating_add(1);
    if stream.kind(offset) == Some(&Kind::RightParen) {
        return Err(stream.shape(offset));
    }
    let mut values = Vec::with_capacity(4);
    loop {
        values.push(value(stream, offset, &field)?);
        offset = offset.saturating_add(1);
        match (stream.kind(offset), stream.kind(offset.saturating_add(1))) {
            (Some(Kind::Comma), Some(Kind::RightParen)) => {
                return Err(stream.shape(offset.saturating_add(1)));
            }
            (Some(Kind::Comma), _) => {
                offset = offset.saturating_add(1);
                if values.len() == MAX_IN_VALUES {
                    return Err(stream.shape(offset));
                }
            }
            (Some(Kind::RightParen), _) => {
                return Ok((Predicate::In(field, values), offset.saturating_add(1)));
            }
            _ => return Err(stream.shape(offset)),
        }
    }
}

fn range(
    stream: &Stream<'_, '_>,
    field: Field,
    operator: &[u8],
    at: usize,
) -> Result<(Predicate, usize), Failure> {
    let separator_at = at.saturating_add(2);
    let separator = stream
        .word(separator_at)
        .ok_or_else(|| stream.shape(separator_at))?;
    let between = keyword(operator, b"BETWEEN") && keyword(separator, b"AND");
    let window = keyword(operator, b"FROM") && keyword(separator, b"TO");
    if !between && !window {
        return Err(stream.shape(separator_at));
    }
    let lower = value(stream, at.saturating_add(1), &field)?;
    let upper = value(stream, at.saturating_add(3), &field)?;
    let predicate = if between {
        Predicate::Range(field, lower, upper)
    } else {
        Predicate::TimeWindow(field, lower, upper)
    };
    Ok((predicate, at.saturating_add(4)))
}

/// The value token `ahead`, typed by its field.
fn value(stream: &Stream<'_, '_>, ahead: usize, field: &Field) -> Result<Value, Failure> {
    match stream.kind(ahead) {
        Some(Kind::Text(text)) => Ok(Value::Literal(Literal::Keyword(text.clone()))),
        Some(Kind::Integer(raw)) => parse_i64(raw)
            .map(|number| Value::Literal(Literal::Integer(number)))
            .ok_or_else(|| stream.fail(Reason::InvalidParameterType, ahead)),
        Some(Kind::Parameter(name)) => Ok(Value::Parameter(
            field.value_type(),
            String::from_utf8_lossy(name).into_owned(),
        )),
        _ => Err(stream.shape(ahead)),
    }
}

/// A field at `ahead`, bracketed or bare, and the offset after it.
pub(super) fn field_tokens(
    stream: &Stream<'_, '_>,
    ahead: usize,
) -> Result<(Field, usize), Failure> {
    let at = |offset: usize| ahead.saturating_add(offset);
    let name = stream.word(ahead).ok_or_else(|| stream.shape(ahead))?;
    let bracket = stream.kind(at(1)) == Some(&Kind::LeftBracket);
    if bracket && keyword(name, b"STATE_META") {
        let state = stream.text(at(2)).ok_or_else(|| stream.shape(at(2)))?;
        stream.expect(at(3), &Kind::RightBracket)?;
        stream.expect(at(4), &Kind::LeftBracket)?;
        let key = stream.text(at(5)).ok_or_else(|| stream.shape(at(5)))?;
        stream.expect(at(6), &Kind::RightBracket)?;
        if !valid_state_name(state) {
            return Err(stream.fail(Reason::UnsupportedField, at(2)));
        }
        if !valid_key(key) {
            return Err(stream.fail(Reason::UnsupportedField, at(5)));
        }
        return Ok((Field::state_meta(state, key), at(7)));
    }
    if bracket && keyword(name, b"ATTRIBUTE") {
        let key = stream.text(at(2)).ok_or_else(|| stream.shape(at(2)))?;
        stream.expect(at(3), &Kind::RightBracket)?;
        if !valid_key(key) {
            return Err(stream.fail(Reason::UnsupportedField, at(2)));
        }
        return Ok((Field::attribute(key), at(4)));
    }
    Field::parse(name)
        .map(|field| (field, at(1)))
        .ok_or_else(|| stream.fail(Reason::UnsupportedField, ahead))
}

/// A signed 64-bit integer, or `None` past the range.
pub(super) fn parse_i64(raw: &[u8]) -> Option<i64> {
    let (negative, digits) = match raw {
        [b'-', digits @ ..] if !digits.is_empty() => (true, digits),
        [] => return None,
        digits => (false, digits),
    };
    let mut value: i64 = 0;
    for byte in digits {
        let digit = i64::from(byte.checked_sub(b'0').filter(|digit| *digit <= 9)?);
        value = value.checked_mul(10)?;
        value = if negative {
            value.checked_sub(digit)?
        } else {
            value.checked_add(digit)?
        };
    }
    Some(value)
}
