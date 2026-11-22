//! The FQL1 parser, ported from the Elixir server's production parser (the
//! native `fql.rs` NIF): a bounded tokenizer and a fixed grammar for point
//! reads, collections, counts and event history, failing at the first
//! offending token with its 1-based byte.
//!
//! ```text
//! [EXPLAIN [ANALYZE]] FROM runs|events WHERE pred [AND pred]...
//!     ( RETURN RECORD [(fields)] | RETURN COUNT
//!     | ORDER BY field ASC|DESC [, field ASC|DESC] LIMIT n [CURSOR @c]
//!       RETURN RECORDS [(fields)] ) [;]
//! pred := field = value | field IN (value, ...) | field IS NULL|MISSING
//!       | field BETWEEN value AND value | field FROM value TO value
//! ```

mod clause;
mod lex;
mod project;
#[cfg(test)]
mod reference_tests;
mod tail;
#[cfg(test)]
mod tests;

use lex::{Kind, Token};

use super::error::{Failure as QueryFailure, Position, Reason};
use super::field::{Field, Selector, ValueType, keyword};

/// Longest query text.
pub const MAX_QUERY_BYTES: usize = 16 * 1024;
/// Most tokens in a query.
pub const MAX_TOKENS: usize = 256;
/// Most predicates.
pub const MAX_PREDICATES: usize = 12;
/// Most values in one `IN`.
pub const MAX_IN_VALUES: usize = 20;
/// Most projected fields.
pub const MAX_RETURN_FIELDS: usize = 32;

/// Plain execution or a plan.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// Run the query.
    Execute,
    /// Plan it.
    Explain,
    /// Plan and run it.
    Analyze,
}

/// What a query reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    /// Flow records.
    Runs,
    /// One Flow's history events.
    Events,
}

/// The request shape the grammar recognised.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shape {
    /// One Flow by id.
    Point,
    /// An ordered page of Flows.
    Collection,
    /// An ordered page of one Flow's events.
    History,
    /// A count of Flows.
    Count,
}

/// A typed literal.
#[derive(Clone, Debug, PartialEq)]
pub enum Literal {
    /// Text.
    Keyword(Vec<u8>),
    /// An integer.
    Integer(i64),
    /// A float, from a parameter.
    Float(f64),
    /// A boolean, from a parameter.
    Boolean(bool),
}

/// A value in a predicate: a literal, or a named parameter of the type its
/// field takes.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    /// A literal.
    Literal(Literal),
    /// `@name`.
    Parameter(ValueType, String),
}

/// A predicate.
#[derive(Clone, Debug, PartialEq)]
pub enum Predicate {
    /// `field = value`.
    Eq(Field, Value),
    /// `field IN (values)`.
    In(Field, Vec<Value>),
    /// `field BETWEEN lower AND upper`, both inclusive.
    Range(Field, Value, Value),
    /// `field FROM lower TO upper`, upper exclusive.
    TimeWindow(Field, Value, Value),
    /// `field IS NULL`.
    IsNull(Field),
    /// `field IS MISSING`.
    IsMissing(Field),
}

impl Predicate {
    /// The field the predicate tests.
    #[must_use]
    pub const fn field(&self) -> &Field {
        match self {
            Self::Eq(field, _)
            | Self::In(field, _)
            | Self::Range(field, ..)
            | Self::TimeWindow(field, ..)
            | Self::IsNull(field)
            | Self::IsMissing(field) => field,
        }
    }
}

/// A sort direction.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Direction {
    /// Ascending.
    Asc,
    /// Descending.
    Desc,
}

/// A parsed query, before its parameters are bound.
#[derive(Clone, Debug, PartialEq)]
pub struct Parsed {
    /// Execute, explain or analyze.
    pub mode: Mode,
    /// Runs or events.
    pub source: Source,
    /// The shape.
    pub shape: Shape,
    /// The predicates, point reads with the partition first.
    pub predicates: Vec<Predicate>,
    /// The sort order.
    pub order_by: Vec<(Field, Direction)>,
    /// The page size; 1 for a point read, none for a count.
    pub limit: Option<usize>,
    /// The cursor parameter.
    pub cursor: Option<Value>,
    /// The projected selectors, if any.
    pub projection: Option<Vec<Selector>>,
}

/// A parser failure: the reason and the 1-based byte of the token.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Failure {
    pub(super) reason: Reason,
    pub(super) byte: usize,
}

impl Failure {
    const fn at(reason: Reason, byte: usize) -> Self {
        Self { reason, byte }
    }

    /// The query failure, with the position of the byte in `query`.
    #[must_use]
    pub fn diagnose(self, query: &[u8]) -> QueryFailure {
        QueryFailure {
            reason: self.reason,
            position: Position::of(query, self.byte),
            missing: Vec::new(),
        }
    }
}

/// The tokens left to parse, read by position relative to the cursor.
struct Stream<'t, 'q> {
    tokens: &'t [Token<'q>],
    at: usize,
    query_len: usize,
}

impl<'t, 'q> Stream<'t, 'q> {
    fn token(&self, ahead: usize) -> Option<&'t Token<'q>> {
        self.tokens.get(self.at.saturating_add(ahead))
    }

    fn kind(&self, ahead: usize) -> Option<&'t Kind<'q>> {
        self.token(ahead).map(|token| &token.kind)
    }

    fn word(&self, ahead: usize) -> Option<&'q [u8]> {
        match self.kind(ahead) {
            Some(Kind::Word(word)) => Some(word),
            _ => None,
        }
    }

    fn is(&self, ahead: usize, expected: &[u8]) -> bool {
        self.word(ahead).is_some_and(|word| keyword(word, expected))
    }

    fn text(&self, ahead: usize) -> Option<&'t [u8]> {
        match self.kind(ahead) {
            Some(Kind::Text(value)) => Some(value),
            _ => None,
        }
    }

    /// The 1-based byte of the token `ahead`, or one past the query.
    fn byte(&self, ahead: usize) -> usize {
        self.token(ahead)
            .map_or(self.query_len.saturating_add(1), |token| {
                token.start.saturating_add(1)
            })
    }

    fn fail(&self, reason: Reason, ahead: usize) -> Failure {
        Failure::at(reason, self.byte(ahead))
    }

    fn shape(&self, ahead: usize) -> Failure {
        self.fail(Reason::UnsupportedQueryShape, ahead)
    }

    fn expect(&self, ahead: usize, expected: &Kind<'_>) -> Result<(), Failure> {
        if self.kind(ahead) == Some(expected) {
            Ok(())
        } else {
            Err(self.shape(ahead))
        }
    }

    fn skip(&mut self, count: usize) {
        self.at = self.at.saturating_add(count);
    }

    /// `validate_terminator/2` at `ahead`: the end, or one final `;`.
    fn terminator(&self, ahead: usize) -> Result<(), Failure> {
        match self.kind(ahead) {
            None => Ok(()),
            Some(Kind::Semicolon) if self.kind(ahead.saturating_add(1)).is_none() => Ok(()),
            Some(Kind::Semicolon) => Err(self.shape(ahead.saturating_add(1))),
            Some(_) => Err(self.shape(ahead)),
        }
    }
}

/// Parses `query`.
///
/// # Errors
/// The first failure, with its byte.
pub fn parse(query: &[u8]) -> Result<Parsed, Failure> {
    if query.len() > MAX_QUERY_BYTES {
        return Err(Failure::at(
            Reason::QueryTooLarge,
            MAX_QUERY_BYTES.saturating_add(1),
        ));
    }
    let tokens = lex::tokenize(query)?;
    let mut stream = Stream {
        tokens: &tokens,
        at: 0,
        query_len: query.len(),
    };
    let mode = if stream.is(0, b"EXPLAIN") {
        if stream.is(1, b"ANALYZE") {
            stream.skip(2);
            Mode::Analyze
        } else {
            stream.skip(1);
            Mode::Explain
        }
    } else {
        Mode::Execute
    };
    if !stream.is(0, b"FROM") {
        return Err(stream.fail(Reason::InvalidSyntax, 0));
    }
    let source_word = stream
        .word(1)
        .ok_or_else(|| stream.fail(Reason::InvalidSyntax, 1))?;
    let source = parse_source(source_word).map_err(|reason| stream.fail(reason, 1));
    if !stream.is(2, b"WHERE") {
        source?;
        return Err(stream.shape(2));
    }
    let source = source?;
    stream.skip(3);
    let predicates = clause::predicates(&mut stream)?;
    tail::parse(mode, source, predicates, &mut stream)
}

fn parse_source(word: &[u8]) -> Result<Source, Reason> {
    if keyword(word, b"RUNS") {
        Ok(Source::Runs)
    } else if keyword(word, b"EVENTS") {
        Ok(Source::Events)
    } else {
        Err(Reason::UnsupportedSource)
    }
}
