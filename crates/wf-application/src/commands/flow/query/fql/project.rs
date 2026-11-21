//! `RETURN RECORD(S) (fields)`: the bounded projection list, with run fields
//! for runs and `event_id`, `fields` and `fields['<name>']` for events.

use super::super::field::{Field, Selector, valid_key};
use super::clause::field_tokens;
use super::tail::is_word;
use super::{Failure, Kind, MAX_RETURN_FIELDS, Reason, Source, Stream};

/// The projection starting `ahead` of the cursor; none when no `(` follows.
pub(super) fn parse(
    source: Source,
    stream: &Stream<'_, '_>,
    ahead: usize,
) -> Result<Option<Vec<Selector>>, Failure> {
    if stream.kind(ahead) != Some(&Kind::LeftParen) {
        stream.terminator(ahead)?;
        return Ok(None);
    }
    let mut offset = ahead.saturating_add(1);
    if stream.kind(offset) == Some(&Kind::RightParen) {
        return Err(stream.shape(offset));
    }
    let mut selectors: Vec<Selector> = Vec::with_capacity(4);
    loop {
        if selectors.len() >= MAX_RETURN_FIELDS {
            return Err(stream.fail(Reason::QueryProjectionLimitExceeded, offset));
        }
        let (selector, next) = selector(source, stream, offset)?;
        if selectors.contains(&selector) {
            return Err(stream.fail(Reason::DuplicateProjectionField, offset));
        }
        selectors.push(selector);
        offset = next;
        match stream.kind(offset) {
            Some(Kind::Comma) => {
                offset = offset.saturating_add(1);
                if stream.kind(offset) == Some(&Kind::RightParen) {
                    return Err(stream.shape(offset));
                }
            }
            Some(Kind::RightParen) => {
                stream.terminator(offset.saturating_add(1))?;
                return Ok(Some(selectors));
            }
            _ => return Err(stream.shape(offset)),
        }
    }
}

/// One selector at `ahead` and the offset after it.
fn selector(
    source: Source,
    stream: &Stream<'_, '_>,
    ahead: usize,
) -> Result<(Selector, usize), Failure> {
    let next = ahead.saturating_add(1);
    let bracket = stream.kind(next) == Some(&Kind::LeftBracket);
    let word = stream.word(ahead);
    if source == Source::Runs {
        if is_word(word, b"ATTRIBUTES") {
            return Ok((Selector::Attributes, next));
        }
        if is_word(word, b"STATE_META") && !bracket {
            return Ok((Selector::StateMeta, next));
        }
        let (field, after) = field_tokens(stream, ahead)?;
        if field == Field::EventId {
            return Err(stream.fail(Reason::UnsupportedField, ahead));
        }
        return Ok((Selector::Field(field), after));
    }
    if word.is_none() {
        return Err(stream.shape(ahead));
    }
    if is_word(word, b"FIELDS") && bracket {
        let name_at = ahead.saturating_add(2);
        let name = stream.text(name_at).ok_or_else(|| stream.shape(name_at))?;
        stream.expect(ahead.saturating_add(3), &Kind::RightBracket)?;
        if !valid_key(name) {
            return Err(stream.fail(Reason::UnsupportedField, name_at));
        }
        return Ok((
            Selector::EventField(String::from_utf8_lossy(name).into_owned()),
            ahead.saturating_add(4),
        ));
    }
    if is_word(word, b"EVENT_ID") {
        Ok((Selector::EventId, next))
    } else if is_word(word, b"FIELDS") {
        Ok((Selector::Fields, next))
    } else {
        Err(stream.fail(Reason::UnsupportedField, ahead))
    }
}
