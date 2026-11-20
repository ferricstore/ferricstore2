//! Typed-map request bodies as application [`Term`]s (ADR-014).

use wf_application::commands::flow::Term;

use crate::codec::Value;

use super::RequestError;

fn term(value: Value) -> Result<Term, RequestError> {
    Ok(match value {
        Value::Nil => Term::Nil,
        Value::Bool(value) => Term::Bool(value),
        Value::Int(value) => Term::Int(value),
        Value::Unsigned(value) => Term::Int(
            i64::try_from(value)
                .map_err(|_| RequestError::new(format!("ERR integer {value} is out of range")))?,
        ),
        Value::Float(value) => Term::Float(value),
        Value::Bin(bytes) => Term::Bin(bytes),
        Value::Arr(items) => Term::List(items.into_iter().map(term).collect::<Result<_, _>>()?),
        Value::Map(entries) => Term::Map(
            entries
                .into_iter()
                .map(|(key, value)| Ok((key, term(value)?)))
                .collect::<Result<_, RequestError>>()?,
        ),
    })
}

/// Decodes a typed-map body of command `name`.
///
/// # Errors
/// Refuses a body that is not exactly one typed value, or not a map.
pub fn options(name: &str, body: &[u8]) -> Result<Term, RequestError> {
    let value = Value::decode_exact(body).map_err(|error| {
        RequestError::new(format!("ERR native {name} payload is invalid: {error}"))
    })?;
    let Value::Map(_) = value else {
        return Err(RequestError::new(format!(
            "ERR native {name} payload must be a map"
        )));
    };
    term(value)
}
