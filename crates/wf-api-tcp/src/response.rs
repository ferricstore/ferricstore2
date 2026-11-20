//! FSNP v1 workflow reply bodies (ADR-011): a `u16` status followed by a
//! typed value or a compact payload. Every shape here is byte-identical to
//! the Elixir server for the replies captured in this crate's fixtures.

mod handshake;

use std::fmt;

use wf_application::commands::flow::Term;

use crate::codec::{TooLong, Value, put_bin, put_len};
use crate::request::ClaimShape;

pub use handshake::{COMPACT_CODECS, Handshake, handshake};

/// Success status.
pub const STATUS_OK: u16 = 0;
/// Command error status; the payload is the error map.
pub const STATUS_ERROR: u16 = 1;
/// Malformed request status.
pub const STATUS_BAD_REQUEST: u16 = 6;

const COMPACT_CLAIM_JOBS: u8 = 0x80;
const COMPACT_OK_LIST: u8 = 0x81;
const EMPTY_ATTRIBUTES: [u8; 5] = [6, 0, 0, 0, 0];

/// A reply part longer than the protocol's `u32` length field.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReplyTooLong;

impl fmt::Display for ReplyTooLong {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("reply part exceeds the u32 length limit")
    }
}

impl std::error::Error for ReplyTooLong {}

impl From<TooLong> for ReplyTooLong {
    fn from(_: TooLong) -> Self {
        Self
    }
}

fn typed(status: u16, value: &Value) -> Result<Vec<u8>, ReplyTooLong> {
    let mut body = status.to_be_bytes().to_vec();
    value.encode(&mut body)?;
    Ok(body)
}

fn text(bytes: &[u8]) -> Value {
    Value::Bin(bytes.to_vec())
}

fn value(term: &Term) -> Value {
    match term {
        Term::Nil => Value::Nil,
        Term::Bool(value) => Value::Bool(*value),
        Term::Int(value) => Value::Int(*value),
        Term::Float(value) => Value::Float(*value),
        Term::Bin(bytes) => Value::Bin(bytes.clone()),
        Term::List(items) => Value::Arr(items.iter().map(value).collect()),
        Term::Map(entries) => Value::Map(
            entries
                .iter()
                .map(|(key, entry)| (key.clone(), value(entry)))
                .collect(),
        ),
    }
}

/// A success reply carrying an application [`Term`] (ADR-014).
///
/// # Errors
/// Refuses a part longer than `u32::MAX` bytes.
pub fn term(term: &Term) -> Result<Vec<u8>, ReplyTooLong> {
    typed(STATUS_OK, &value(term))
}

/// A success reply carrying `text`, such as `OK`.
///
/// # Errors
/// Refuses text longer than `u32::MAX` bytes.
pub fn ok_text(bytes: &[u8]) -> Result<Vec<u8>, ReplyTooLong> {
    typed(STATUS_OK, &text(bytes))
}

/// An error reply: `status` and the Elixir error map `{code, message,
/// retry_after_ms, retryable, safe_to_retry}` for a non-retryable error.
///
/// # Errors
/// Refuses a message longer than `u32::MAX` bytes.
pub fn error(status: u16, message: &str) -> Result<Vec<u8>, ReplyTooLong> {
    let map = Value::Map(vec![
        (b"code".to_vec(), text(b"error")),
        (b"message".to_vec(), text(message.as_bytes())),
        (b"retry_after_ms".to_vec(), Value::Int(0)),
        (b"retryable".to_vec(), Value::Bool(false)),
        (b"safe_to_retry".to_vec(), Value::Bool(false)),
    ]);
    typed(status, &map)
}

/// The typed all-succeeded reply for `count` batch items: a list of `OK`.
///
/// # Errors
/// Refuses a count above `u32::MAX`.
pub fn ok_text_list(count: usize) -> Result<Vec<u8>, ReplyTooLong> {
    typed(STATUS_OK, &Value::Arr(vec![text(b"OK"); count]))
}

/// The compact all-succeeded reply for `count` batch items.
///
/// # Errors
/// Refuses a count above `u32::MAX`.
pub fn compact_ok_list(count: usize) -> Result<Vec<u8>, ReplyTooLong> {
    let mut body = STATUS_OK.to_be_bytes().to_vec();
    body.push(COMPACT_OK_LIST);
    put_len(count, &mut body)?;
    Ok(body)
}

/// The outcome of one batch item.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ItemOutcome<'a> {
    /// The item succeeded.
    Ok,
    /// The item failed with this `ERR ...` message.
    Error(&'a str),
}

/// The per-item batch reply: `ok`, or `[error, message]`, per item in order.
///
/// # Errors
/// Refuses more than `u32::MAX` items or an over-long message.
pub fn item_outcomes(outcomes: &[ItemOutcome<'_>]) -> Result<Vec<u8>, ReplyTooLong> {
    let items = outcomes
        .iter()
        .map(|outcome| match outcome {
            ItemOutcome::Ok => text(b"ok"),
            ItemOutcome::Error(message) => {
                Value::Arr(vec![text(b"error"), text(message.as_bytes())])
            }
        })
        .collect();
    typed(STATUS_OK, &Value::Arr(items))
}

/// One claimed Flow in a claim reply.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClaimedJob<'a> {
    /// Flow id.
    pub id: &'a [u8],
    /// Partition name.
    pub partition: &'a [u8],
    /// Lease token.
    pub lease_token: &'a [u8],
    /// Fencing token.
    pub fencing_token: i64,
    /// Run state, sent for the state shapes.
    pub run_state: &'a [u8],
}

/// The compact claim reply (tag `0x80`). Attributes are always an empty map:
/// this server refuses attribute options on create, so no Flow has any.
///
/// # Errors
/// Refuses more than `u32::MAX` jobs or an over-long field.
pub fn compact_claim_jobs(
    jobs: &[ClaimedJob<'_>],
    shape: ClaimShape,
) -> Result<Vec<u8>, ReplyTooLong> {
    let mut body = STATUS_OK.to_be_bytes().to_vec();
    body.push(COMPACT_CLAIM_JOBS);
    put_len(jobs.len(), &mut body)?;
    for job in jobs {
        put_bin(job.id, &mut body)?;
        put_bin(job.partition, &mut body)?;
        put_bin(job.lease_token, &mut body)?;
        body.extend_from_slice(&job.fencing_token.to_be_bytes());
        if matches!(
            shape,
            ClaimShape::JobsState | ClaimShape::JobsStateAttributes
        ) {
            put_bin(job.run_state, &mut body)?;
        }
        if matches!(
            shape,
            ClaimShape::JobsAttributes | ClaimShape::JobsStateAttributes
        ) {
            body.extend_from_slice(&EMPTY_ATTRIBUTES);
        }
    }
    Ok(body)
}

/// The typed claim reply for connections that did not negotiate the compact
/// form: an array of rows in the same field order as the compact reply.
///
/// # Errors
/// Refuses more than `u32::MAX` jobs or an over-long field.
pub fn typed_claim_jobs(
    jobs: &[ClaimedJob<'_>],
    shape: ClaimShape,
) -> Result<Vec<u8>, ReplyTooLong> {
    let rows = jobs
        .iter()
        .map(|job| {
            let mut row = vec![
                text(job.id),
                text(job.partition),
                text(job.lease_token),
                Value::Int(job.fencing_token),
            ];
            if matches!(
                shape,
                ClaimShape::JobsState | ClaimShape::JobsStateAttributes
            ) {
                row.push(text(job.run_state));
            }
            if matches!(
                shape,
                ClaimShape::JobsAttributes | ClaimShape::JobsStateAttributes
            ) {
                row.push(Value::Map(Vec::new()));
            }
            Value::Arr(row)
        })
        .collect();
    typed(STATUS_OK, &Value::Arr(rows))
}
