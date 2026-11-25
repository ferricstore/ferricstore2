//! `HELLO` and `STARTUP` bodies (audit gap G63): both negotiate compact
//! replies, and both refuse, as a bad request, what Elixir's
//! `do_execute(@op_hello)` and `do_execute(@op_startup)` refuse.

use crate::codec::Value;

use super::{RequestError, Startup};

/// Every compact reply codec the native protocol defines. A client may ask
/// for any of them; the server answers with those it writes.
const PROTOCOL_CODECS: [&[u8]; 8] = [
    b"flow_claim_jobs_v1",
    b"flow_query_result_v1",
    b"flow_record_v1",
    b"kv_get_v1",
    b"kv_mget_v1",
    b"ok_list_v1",
    b"pipeline_v1",
    b"pubsub_batch_v1",
];
const MAX_CODECS: usize = 32;
const MAX_CODEC_NAME_BYTES: usize = 128;
const MAX_CLIENT_NAME_BYTES: usize = 1024;

/// Parses a `HELLO` or `STARTUP` body (`command` names it in refusals): a
/// typed map, or nothing, whose client and driver names, compression and
/// compact reply choices are checked; other keys are accepted and unused.
///
/// # Errors
/// Refuses, as a bad request, a body that is not a map and any field
/// Elixir refuses.
pub fn handshake(command: &str, body: &[u8]) -> Result<Startup, RequestError> {
    let entries = if body.is_empty() {
        Vec::new()
    } else {
        match Value::decode_exact(body) {
            Ok(Value::Map(entries)) => entries,
            Ok(_) => {
                return Err(RequestError::bad(format!(
                    "ERR native {command} payload must be a map"
                )));
            }
            Err(error) => {
                return Err(RequestError::bad(format!(
                    "ERR native {command} payload is invalid: {error}"
                )));
            }
        }
    };
    let field = |key: &[u8]| {
        entries
            .iter()
            .find(|(name, _)| name.as_slice() == key)
            .map(|(_, value)| value)
    };
    name(field(b"client_name"), "client")?;
    name(field(b"driver_name"), "driver")?;
    compression(field(b"compression"))?;
    Ok(Startup {
        codecs: codecs(field(b"compact_response_codecs"))?,
        compact_flow_responses: matches!(
            field(b"compact_flow_responses"),
            Some(Value::Bool(true) | Value::Int(1))
        ) || matches!(
            field(b"compact_flow_responses"),
            Some(Value::Bin(yes)) if yes.as_slice() == b"true" || yes.as_slice() == b"1"
        ),
    })
}

/// A client or driver name, if given: UTF-8 text of at most 1024 bytes.
fn name(value: Option<&Value>, who: &str) -> Result<(), RequestError> {
    match value {
        Some(Value::Bin(name)) if name.len() > MAX_CLIENT_NAME_BYTES => Err(RequestError::bad(
            format!("ERR native {who} name exceeds {MAX_CLIENT_NAME_BYTES} bytes"),
        )),
        Some(Value::Bin(name)) if std::str::from_utf8(name).is_err() => Err(RequestError::bad(
            format!("ERR native {who} name must be valid UTF-8"),
        )),
        None | Some(Value::Nil | Value::Bin(_)) => Ok(()),
        Some(_) => Err(RequestError::bad(format!(
            "ERR native {who} name must be a binary"
        ))),
    }
}

/// The compression asked for: none, as this server never compresses.
fn compression(value: Option<&Value>) -> Result<(), RequestError> {
    let shown = match value {
        None | Some(Value::Nil) => return Ok(()),
        Some(Value::Bin(name)) if name.as_slice() == b"none" => return Ok(()),
        Some(Value::Bin(name)) if name.as_slice() == b"zlib" => {
            return Err(RequestError::bad(
                "ERR native request compression is disabled",
            ));
        }
        Some(Value::Bin(name)) => String::from_utf8_lossy(name).into_owned(),
        Some(Value::Int(number)) => number.to_string(),
        Some(Value::Bool(flag)) => flag.to_string(),
        Some(_) => return Err(RequestError::bad("ERR native unsupported compression")),
    };
    Err(RequestError::bad(format!(
        "ERR native unsupported compression {shown}"
    )))
}

/// The compact reply codecs asked for: a list of at most 32 distinct
/// names the protocol defines.
fn codecs(value: Option<&Value>) -> Result<Vec<Vec<u8>>, RequestError> {
    let names = match value {
        None | Some(Value::Nil) => return Ok(Vec::new()),
        Some(Value::Arr(names)) if names.len() > MAX_CODECS => {
            return Err(RequestError::bad(format!(
                "ERR native compact_response_codecs accepts at most {MAX_CODECS} entries"
            )));
        }
        Some(Value::Arr(names)) => names,
        Some(_) => {
            return Err(RequestError::bad(
                "ERR native compact_response_codecs must be a list",
            ));
        }
    };
    let mut chosen: Vec<Vec<u8>> = Vec::with_capacity(names.len());
    for name in names {
        match name {
            Value::Bin(name)
                if !name.is_empty()
                    && name.len() <= MAX_CODEC_NAME_BYTES
                    && std::str::from_utf8(name).is_ok() =>
            {
                chosen.push(name.clone());
            }
            _ => {
                return Err(RequestError::bad(format!(
                    "ERR native compact_response_codecs must contain binaries of 1..{MAX_CODEC_NAME_BYTES} bytes"
                )));
            }
        }
    }
    let mut distinct = chosen.clone();
    distinct.sort_unstable();
    distinct.dedup();
    if distinct.len() != chosen.len() {
        return Err(RequestError::bad(
            "ERR native compact_response_codecs must not contain duplicates",
        ));
    }
    let unknown: Vec<String> = distinct
        .iter()
        .filter(|name| !PROTOCOL_CODECS.contains(&name.as_slice()))
        .map(|name| String::from_utf8_lossy(name).into_owned())
        .collect();
    if unknown.is_empty() {
        Ok(chosen)
    } else {
        Err(RequestError::bad(format!(
            "ERR native unsupported response codec: {}",
            unknown.join(", ")
        )))
    }
}
