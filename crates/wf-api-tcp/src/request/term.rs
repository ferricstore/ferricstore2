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

/// One command of a typed `PIPELINE` body.
#[derive(Clone, Debug, PartialEq)]
pub struct PipelineCommand {
    /// The command's opcode.
    pub opcode: u16,
    /// Lane named by the client, echoed in the reply.
    pub lane_id: i64,
    /// Request id named by the client, echoed in the reply.
    pub request_id: i64,
    /// The command's typed-map body.
    pub body: Term,
}

/// Decodes a typed `PIPELINE` body: `{atomicity, commands, return}` where
/// each command is `{opcode, lane_id, request_id, body}`.
///
/// # Errors
/// Refuses a body that is not that shape.
pub fn pipeline(body: &[u8]) -> Result<Vec<PipelineCommand>, RequestError> {
    let invalid = || RequestError::new("ERR native PIPELINE payload is invalid");
    let Term::Map(entries) = options("PIPELINE", body)? else {
        return Err(invalid());
    };
    let Some((_, Term::List(commands))) = entries.iter().find(|(key, _)| key == b"commands") else {
        return Err(invalid());
    };
    commands
        .iter()
        .map(|command| {
            let number = |key: &[u8]| match command.get(key) {
                Some(Term::Int(value)) => Some(*value),
                _ => None,
            };
            Some(PipelineCommand {
                opcode: u16::try_from(number(b"opcode")?).ok()?,
                lane_id: number(b"lane_id").unwrap_or(0),
                request_id: number(b"request_id").unwrap_or(0),
                body: command
                    .get(b"body")
                    .cloned()
                    .unwrap_or(Term::Map(Vec::new())),
            })
        })
        .collect::<Option<Vec<_>>>()
        .ok_or_else(invalid)
}
