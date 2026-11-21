//! Why a Flow query failed, as `Ferricstore.Flow.Query.Error` and
//! `ErrorDiagnostic` report it: a code, an `ERR ...` message, a wire status,
//! and for parser failures the line, column and byte, a detail, a hint and
//! context.

use super::super::term::{Term, map};
use super::field::{EVENT_PROJECTION_FIELDS, RUN_PROJECTION_FIELDS, SUPPORTED_FIELDS};
use super::fql::MAX_RETURN_FIELDS;

/// A query failure's reason.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reason {
    /// `duplicate_projection_field`.
    DuplicateProjectionField,
    /// `invalid_parameters`.
    InvalidParameters,
    /// `invalid_parameter_type`.
    InvalidParameterType,
    /// `invalid_syntax`; also Elixir's answer to reasons it has no code for.
    InvalidSyntax,
    /// `missing_parameter`.
    MissingParameter,
    /// `query_cursor_expired`.
    QueryCursorExpired,
    /// `query_cursor_invalid`.
    QueryCursorInvalid,
    /// `query_cursor_too_large`.
    QueryCursorTooLarge,
    /// `query_projection_limit_exceeded`.
    QueryProjectionLimitExceeded,
    /// `query_too_large`.
    QueryTooLarge,
    /// `query_value_too_large`.
    QueryValueTooLarge,
    /// `unexpected_parameter`.
    UnexpectedParameter,
    /// `unsupported_field`.
    UnsupportedField,
    /// `unsupported_query_shape`.
    UnsupportedQueryShape,
    /// `unsupported_query_version`.
    UnsupportedQueryVersion,
    /// `unsupported_source`.
    UnsupportedSource,
}

impl Reason {
    /// The wire code and message.
    #[must_use]
    pub const fn code_and_message(self) -> (&'static str, &'static str) {
        match self {
            Self::DuplicateProjectionField => (
                "duplicate_projection_field",
                "ERR FQL1 return projection contains a duplicate field",
            ),
            Self::InvalidParameters => (
                "invalid_parameters",
                "ERR FQL1 parameters must be an object",
            ),
            Self::InvalidParameterType => (
                "invalid_parameter_type",
                "ERR FQL1 parameter has an invalid type",
            ),
            Self::InvalidSyntax => ("invalid_syntax", "ERR FQL1 invalid syntax"),
            Self::MissingParameter => ("missing_parameter", "ERR FQL1 parameter is missing"),
            Self::QueryCursorExpired => ("query_cursor_expired", "ERR Flow query cursor expired"),
            Self::QueryCursorInvalid => {
                ("query_cursor_invalid", "ERR Flow query cursor is invalid")
            }
            Self::QueryCursorTooLarge => (
                "query_cursor_too_large",
                "ERR Flow query cursor exceeds the byte limit",
            ),
            Self::QueryProjectionLimitExceeded => (
                "query_projection_limit_exceeded",
                "ERR FQL1 return projection exceeds the field limit",
            ),
            Self::QueryTooLarge => ("query_too_large", "ERR FQL1 query exceeds the byte limit"),
            Self::QueryValueTooLarge => (
                "query_value_too_large",
                "ERR FQL1 value exceeds the byte limit",
            ),
            Self::UnexpectedParameter => (
                "unexpected_parameter",
                "ERR FQL1 received an unexpected parameter",
            ),
            Self::UnsupportedField => ("unsupported_field", "ERR FQL1 field is not supported"),
            Self::UnsupportedQueryShape => (
                "unsupported_query_shape",
                "ERR FQL1 query shape is not supported",
            ),
            Self::UnsupportedQueryVersion => {
                ("unsupported_query_version", "ERR unsupported FQL version")
            }
            Self::UnsupportedSource => ("unsupported_source", "ERR FQL1 source is not supported"),
        }
    }
}

/// Where in the query text a failure was found, 1-based.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Position {
    /// Byte offset.
    pub byte: usize,
    /// Line.
    pub line: usize,
    /// Column, in characters.
    pub column: usize,
}

impl Position {
    /// The position of 1-based `byte` in `query`, counting UTF-8
    /// characters for the column as `ErrorDiagnostic.source_position/2`.
    #[must_use]
    pub fn of(query: &[u8], byte: usize) -> Option<Self> {
        if byte == 0 || byte > query.len().saturating_add(1) {
            return None;
        }
        let (mut line, mut column) = (1_usize, 1_usize);
        let mut remaining = byte.saturating_sub(1);
        let mut rest = query;
        while remaining > 0 {
            let Some(&lead) = rest.first() else {
                break;
            };
            if lead == b'\n' {
                line = line.saturating_add(1);
                column = 1;
                rest = rest.get(1..).unwrap_or_default();
                remaining = remaining.saturating_sub(1);
                continue;
            }
            // A valid UTF-8 character is one column; a stray byte is too.
            let width = (1..=4)
                .find(|width| {
                    rest.get(..*width)
                        .is_some_and(|chunk| std::str::from_utf8(chunk).is_ok())
                })
                .unwrap_or(1);
            if width > remaining {
                // The byte falls inside a character: count its bytes.
                return Some(Self {
                    byte,
                    line,
                    column: column.saturating_add(remaining),
                });
            }
            column = column.saturating_add(1);
            rest = rest.get(width..).unwrap_or_default();
            remaining = remaining.saturating_sub(width);
        }
        Some(Self { byte, line, column })
    }
}

/// A failed query.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Failure {
    /// Why.
    pub reason: Reason,
    /// Where, for a parser failure.
    pub position: Option<Position>,
}

impl Failure {
    /// A failure with no position.
    #[must_use]
    pub const fn new(reason: Reason) -> Self {
        Self {
            reason,
            position: None,
        }
    }

    /// The wire status: Elixir answers every query failure this server
    /// raises as a bad request.
    #[must_use]
    pub const fn status(&self) -> u16 {
        6
    }

    /// The `ERR ...` message.
    #[must_use]
    pub const fn message(&self) -> &'static str {
        self.reason.code_and_message().1
    }

    fn diagnostic(&self) -> Option<(&'static str, String, Option<Term>)> {
        let texts =
            |items: &[&str]| Term::List(items.iter().map(|item| Term::text(item)).collect());
        Some(match self.reason {
            Reason::InvalidSyntax => (
                "FQL1 could not parse the query near the reported position.",
                "Use FROM runs|events WHERE ..., optionally prefixed by EXPLAIN or EXPLAIN ANALYZE; collection reads require ORDER BY, LIMIT, and RETURN RECORDS.".to_owned(),
                None,
            ),
            Reason::UnsupportedField => (
                "The field at the reported position is not available in FQL1.",
                format!(
                    "Use a supported predicate field or one of the source-specific result fields in context. Valid fields: {}.",
                    SUPPORTED_FIELDS.join(", ")
                ),
                Some(map(vec![
                    ("supported_fields", texts(&SUPPORTED_FIELDS)),
                    (
                        "supported_projection_fields",
                        map(vec![
                            ("events", texts(&EVENT_PROJECTION_FIELDS)),
                            ("runs", texts(&RUN_PROJECTION_FIELDS)),
                        ]),
                    ),
                ])),
            ),
            Reason::UnsupportedSource => (
                "The source at the reported position is not available in FQL1.",
                "Use FROM runs or FROM events.".to_owned(),
                Some(map(vec![("supported_sources", texts(&["events", "runs"]))])),
            ),
            Reason::UnsupportedQueryShape => (
                "The query parsed, but its clauses do not match a supported FQL1 request shape.",
                "Point reads require run_id, plus partition_key for explicitly partitioned runs, and RETURN RECORD. Collection reads require a partition_key predicate, integer ORDER BY, LIMIT, and RETURN RECORDS; counts use RETURN COUNT without ordering or limits. Event history requires run_id, optional partition_key, ORDER BY event_id, LIMIT, and RETURN RECORDS.".to_owned(),
                None,
            ),
            Reason::QueryCursorInvalid => (
                "EXPLAIN and EXPLAIN ANALYZE plan a fresh query and do not accept a pagination cursor.",
                "Remove CURSOR, or execute the query without an EXPLAIN prefix to resume a page.".to_owned(),
                None,
            ),
            Reason::DuplicateProjectionField => (
                "The return projection names the same result field more than once.",
                "Remove the duplicate field; projection order does not change the returned map.".to_owned(),
                None,
            ),
            Reason::QueryProjectionLimitExceeded => (
                "The return projection exceeds FQL1's bounded field limit.",
                "Select fewer fields or use bare RETURN RECORD or RETURN RECORDS for the complete allowlisted record.".to_owned(),
                Some(map(vec![("max_projection_fields", Term::int(MAX_RETURN_FIELDS as u64))])),
            ),
            _ => return None,
        })
    }

    /// The error map Elixir replies with; a parser failure adds its detail,
    /// hint, context and position.
    #[must_use]
    pub fn payload(&self) -> Term {
        let (code, message) = self.reason.code_and_message();
        let mut entries = Vec::new();
        // Only reasons with a diagnostic report where they failed.
        if let (Some(position), Some((detail, hint, context))) = (self.position, self.diagnostic())
        {
            if let Some(context) = context {
                entries.push(("context", context));
            }
            entries.push(("detail", Term::text(detail)));
            entries.push(("hint", Term::Bin(hint.into_bytes())));
            entries.push((
                "position",
                map(vec![
                    ("byte", Term::int(position.byte as u64)),
                    ("column", Term::int(position.column as u64)),
                    ("line", Term::int(position.line as u64)),
                ]),
            ));
        }
        entries.extend([
            ("code", Term::text(code)),
            ("message", Term::text(message)),
            ("retry_after_ms", Term::Int(0)),
            ("retryable", Term::Bool(false)),
            ("safe_to_retry", Term::Bool(false)),
        ]);
        entries.sort_by_key(|(key, _)| *key);
        map(entries)
    }
}
