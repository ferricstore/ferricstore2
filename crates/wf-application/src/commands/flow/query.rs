//! `FLOW.QUERY` (ADR-014 step 12): FQL1 over Flow records and history, as
//! the Elixir server's native parser, `Binder`, `Request.validate` and the
//! reference evaluator define it.
//!
//! A request carries `version` (`FQL1`), the `query` text and named
//! `params`. Point reads name a `run_id` (and its `partition_key` unless
//! the Flow is in an auto bucket); collections and counts name exactly one
//! `partition_key`; event history names a `run_id`. The node answers from
//! the partition's shard: candidates come from its catalog, the predicates
//! and order run here, and a page resumes from an opaque cursor bound to
//! the query. Results are authoritative and current, where Elixir's come
//! from an asynchronous projection.

pub mod answer;
mod bind;
pub mod cursor;
mod error;
#[cfg(test)]
mod error_tests;
pub mod field;
pub mod fql;
pub mod indexes;
#[cfg(test)]
mod reference_bind_tests;
pub mod run;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod text_tests;
#[cfg(test)]
mod validation_tests;

pub use error::{Failure, Position, Reason};

use sha2::{Digest, Sha256};

use super::FlowError;
use super::term::Term;
use field::{Field, Selector};
use fql::{Direction, Literal, Mode, Predicate, Shape, Source, Value};

/// The only language version.
const VERSION: &[u8] = b"FQL1";
/// Most named parameters in a request.
const MAX_PARAMETERS: usize = 64;
const PAYLOAD_KEYS: [&str; 5] = [
    "version",
    "query",
    "params",
    "deadline_ms",
    "request_context",
];

/// A parsed, bound query.
#[derive(Clone, Debug, PartialEq)]
pub struct Query {
    /// Execute, explain or analyze.
    pub mode: Mode,
    /// Runs or events.
    pub source: Source,
    /// Point, collection, history or count.
    pub shape: Shape,
    /// Predicates with literal values.
    pub predicates: Vec<Predicate>,
    /// Sort order.
    pub order_by: Vec<(Field, Direction)>,
    /// Page size.
    pub limit: Option<usize>,
    /// The cursor token to resume from.
    pub cursor: Option<Vec<u8>>,
    /// Projected selectors.
    pub projection: Option<Vec<Selector>>,
    /// The query text, for its fingerprint.
    pub text: Vec<u8>,
    /// What a cursor of this query binds to.
    pub digest: [u8; 32],
}

fn keyword_eq<'a>(predicates: &'a [Predicate], field: &Field) -> Option<&'a [u8]> {
    predicates.iter().find_map(|predicate| match predicate {
        Predicate::Eq(found, Value::Literal(Literal::Keyword(value))) if found == field => {
            Some(value.as_slice())
        }
        _ => None,
    })
}

impl Query {
    /// The `partition_key` the query names.
    #[must_use]
    pub fn partition_key(&self) -> Option<&[u8]> {
        keyword_eq(&self.predicates, &Field::PartitionKey)
    }

    /// The `run_id` the query names.
    #[must_use]
    pub fn run_id(&self) -> Option<&[u8]> {
        keyword_eq(&self.predicates, &Field::RunId)
    }

    /// The values `field` must take, from an equality or an `IN`; `None`
    /// when unconstrained.
    #[must_use]
    pub fn only(&self, field: &Field) -> Option<Vec<&[u8]>> {
        self.predicates.iter().find_map(|predicate| {
            let values = match predicate {
                Predicate::Eq(found, value) if found == field => vec![value],
                Predicate::In(found, values) if found == field => values.iter().collect(),
                _ => return None,
            };
            Some(
                values
                    .into_iter()
                    .filter_map(|value| match value {
                        Value::Literal(Literal::Keyword(text)) => Some(text.as_slice()),
                        _ => None,
                    })
                    .collect(),
            )
        })
    }
}

fn bad_request(text: &str) -> FlowError {
    FlowError::BadRequest(text.to_owned())
}

fn failed(failure: Failure) -> FlowError {
    FlowError::Query(Box::new(failure))
}

/// Reads a `FLOW.QUERY` request.
///
/// # Errors
/// The native layer's bad-request text for a malformed payload, or the
/// query failure.
pub fn parse(args: &Term) -> Result<Query, FlowError> {
    let Term::Map(entries) = args else {
        return Err(bad_request("ERR native FLOW.QUERY payload must be a map"));
    };
    if entries.iter().any(|(key, _)| {
        !PAYLOAD_KEYS
            .iter()
            .any(|name| name.as_bytes() == key.as_slice())
    }) {
        return Err(bad_request(
            "ERR native FLOW.QUERY contains unsupported fields",
        ));
    }
    let Some(Term::Bin(version)) = args.get(b"version") else {
        return Err(bad_request("ERR native field version must be binary"));
    };
    let Some(Term::Bin(text)) = args.get(b"query") else {
        return Err(bad_request("ERR native field query must be binary"));
    };
    let params: &[(Vec<u8>, Term)] = match args.get(b"params") {
        None => &[],
        Some(Term::Map(params)) if params.len() <= MAX_PARAMETERS => params,
        Some(Term::Map(_)) => {
            return Err(bad_request(
                "ERR native FLOW.QUERY accepts at most 64 named parameters",
            ));
        }
        Some(_) => return Err(bad_request("ERR native field params must be a map")),
    };
    if version.as_slice() != VERSION {
        return Err(failed(Failure::new(Reason::UnsupportedQueryVersion)));
    }
    let parsed = fql::parse(text).map_err(|failure| failed(failure.diagnose(text)))?;
    let parsed = bind::bind(parsed, params).map_err(|reason| failed(Failure::new(reason)))?;
    Ok(finish(parsed, text))
}

/// Reads `FLOW.QUERY` in its text form (audit gap G39), as Elixir's native
/// parser reads it: `version query [name value ...]`. A value decodes to
/// the type its field declares (`Binder.bind_text`: integers from their
/// digits, all else as text), and a missing parameter is named in the
/// failure.
///
/// # Errors
/// Elixir's text for malformed arguments, or the query failure.
pub fn parse_text(args: &[Term]) -> Result<Query, FlowError> {
    let [Term::Bin(version), Term::Bin(text), rest @ ..] = args else {
        return Err(bad_request(
            "ERR wrong number of arguments for 'flow.query' command",
        ));
    };
    let mut pairs: Vec<(Vec<u8>, Vec<u8>)> = Vec::new();
    for pair in rest.chunks(2) {
        let [Term::Bin(name), Term::Bin(value)] = pair else {
            return Err(bad_request(
                "ERR FLOW.QUERY parameters must be name/value pairs",
            ));
        };
        if name.is_empty() {
            return Err(bad_request(
                "ERR FLOW.QUERY parameters must be name/value pairs",
            ));
        }
        if pairs.len() >= MAX_PARAMETERS {
            return Err(bad_request(
                "ERR FLOW.QUERY accepts at most 64 named parameters",
            ));
        }
        if pairs.iter().any(|(seen, _)| seen == name) {
            return Err(bad_request("ERR FLOW.QUERY parameter names must be unique"));
        }
        pairs.push((name.clone(), value.clone()));
    }
    if version.as_slice() != VERSION {
        return Err(failed(Failure::new(Reason::UnsupportedQueryVersion)));
    }
    let parsed = fql::parse(text).map_err(|failure| failed(failure.diagnose(text)))?;
    let declared = bind::declared(&parsed);
    let missing: Vec<String> = declared
        .iter()
        .filter(|(name, _)| {
            !pairs
                .iter()
                .any(|(given, _)| given.as_slice() == name.as_bytes())
        })
        .map(|(name, _)| name.clone())
        .collect();
    let params: Vec<(Vec<u8>, Term)> = pairs
        .into_iter()
        .map(|(name, value)| {
            let integer = declared.iter().any(|(declared, kind)| {
                declared.as_bytes() == name.as_slice() && *kind == field::ValueType::Integer
            });
            // A value that is not an integer's digits binds as nil, which
            // the binder refuses as the wrong type after its key checks.
            let term = if integer {
                std::str::from_utf8(&value)
                    .ok()
                    .and_then(|digits| digits.parse::<i64>().ok())
                    .map_or(Term::Nil, Term::Int)
            } else {
                Term::Bin(value)
            };
            (name, term)
        })
        .collect();
    let parsed = bind::bind(parsed, &params).map_err(|reason| {
        let mut failure = Failure::new(reason);
        if reason == Reason::MissingParameter {
            failure.missing = missing;
        }
        failed(failure)
    })?;
    Ok(finish(parsed, text))
}

/// The bound query with its cursor and digest.
fn finish(parsed: fql::Parsed, text: &[u8]) -> Query {
    let cursor = match parsed.cursor {
        Some(Value::Literal(Literal::Keyword(token))) => Some(token),
        _ => None,
    };
    let digest = Sha256::new()
        .chain_update(format!(
            "{:?}|{:?}|{:?}|{:?}",
            parsed.source, parsed.shape, parsed.predicates, parsed.order_by
        ))
        .finalize()
        .into();
    Query {
        mode: parsed.mode,
        source: parsed.source,
        shape: parsed.shape,
        predicates: parsed.predicates,
        order_by: parsed.order_by,
        limit: parsed.limit,
        cursor,
        projection: parsed.projection,
        text: text.to_vec(),
        digest,
    }
}
