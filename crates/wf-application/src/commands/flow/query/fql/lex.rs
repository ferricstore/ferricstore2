//! FQL1 tokens, as the Elixir server's native tokenizer reads them: words,
//! quoted texts (`''` for a quote), `@name` parameters, integers and
//! punctuation, each remembering its byte offset for diagnostics.

use super::{Failure, MAX_TOKENS, Reason};

/// One token's kind.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Kind<'q> {
    Word(&'q [u8]),
    Text(Vec<u8>),
    Parameter(&'q [u8]),
    Integer(&'q [u8]),
    Equals,
    LeftParen,
    RightParen,
    LeftBracket,
    RightBracket,
    Comma,
    Semicolon,
}

/// A token and the offset it starts at.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct Token<'q> {
    pub(super) kind: Kind<'q>,
    pub(super) start: usize,
}

const fn whitespace(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\n' | b'\r')
}

const fn word_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

const fn word_continue(byte: u8) -> bool {
    word_start(byte) || byte.is_ascii_digit() || matches!(byte, b'.' | b'-')
}

const fn punctuation(byte: u8) -> Option<Kind<'static>> {
    match byte {
        b'=' => Some(Kind::Equals),
        b'(' => Some(Kind::LeftParen),
        b')' => Some(Kind::RightParen),
        b'[' => Some(Kind::LeftBracket),
        b']' => Some(Kind::RightBracket),
        b',' => Some(Kind::Comma),
        b';' => Some(Kind::Semicolon),
        _ => None,
    }
}

fn syntax(start: usize) -> Failure {
    Failure::at(Reason::InvalidSyntax, start.saturating_add(1))
}

/// The end of the identifier starting at `offset`.
fn identifier_end(query: &[u8], offset: usize) -> usize {
    let length = query
        .get(offset..)
        .unwrap_or_default()
        .iter()
        .take_while(|byte| word_continue(**byte))
        .count();
    offset.saturating_add(length)
}

/// The end of the integer starting at `offset`, past an optional minus.
fn integer_end(query: &[u8], offset: usize) -> usize {
    let digits = if query.get(offset) == Some(&b'-') {
        offset.saturating_add(1)
    } else {
        offset
    };
    let length = query
        .get(digits..)
        .unwrap_or_default()
        .iter()
        .take_while(|byte| byte.is_ascii_digit())
        .count();
    digits.saturating_add(length)
}

/// A quoted text from after its opening quote; the offset past its close.
fn text(query: &[u8], mut offset: usize) -> Option<(Vec<u8>, usize)> {
    let mut value = Vec::new();
    while let Some(&byte) = query.get(offset) {
        if byte != b'\'' {
            value.push(byte);
            offset = offset.saturating_add(1);
        } else if query.get(offset.saturating_add(1)) == Some(&b'\'') {
            value.push(b'\'');
            offset = offset.saturating_add(2);
        } else {
            return Some((value, offset.saturating_add(1)));
        }
    }
    None
}

/// The tokens of `query`.
pub(super) fn tokenize(query: &[u8]) -> Result<Vec<Token<'_>>, Failure> {
    let mut tokens = Vec::with_capacity(48);
    let mut offset = 0;
    loop {
        while query.get(offset).copied().is_some_and(whitespace) {
            offset = offset.saturating_add(1);
        }
        let Some(&byte) = query.get(offset) else {
            return Ok(tokens);
        };
        if tokens.len() >= MAX_TOKENS {
            return Err(Failure::at(
                Reason::UnsupportedQueryShape,
                offset.saturating_add(1),
            ));
        }
        let start = offset;
        let (kind, next) = if let Some(kind) = punctuation(byte) {
            (kind, offset.saturating_add(1))
        } else if byte == b'\'' {
            let (value, next) =
                text(query, offset.saturating_add(1)).ok_or_else(|| syntax(start))?;
            (Kind::Text(value), next)
        } else if byte == b'@' {
            let name_start = offset.saturating_add(1);
            let end = identifier_end(query, name_start);
            let name = query.get(name_start..end).unwrap_or_default();
            if name.is_empty() {
                return Err(syntax(start));
            }
            (Kind::Parameter(name), end)
        } else if byte.is_ascii_digit() || (byte == b'-' && offset.saturating_add(1) < query.len())
        {
            let end = integer_end(query, offset);
            let raw = query.get(offset..end).unwrap_or_default();
            if raw.is_empty() || raw == b"-" {
                return Err(syntax(start));
            }
            (Kind::Integer(raw), end)
        } else if word_start(byte) {
            let end = identifier_end(query, offset);
            (Kind::Word(query.get(offset..end).unwrap_or_default()), end)
        } else {
            return Err(syntax(start));
        };
        tokens.push(Token { kind, start });
        offset = next;
    }
}
