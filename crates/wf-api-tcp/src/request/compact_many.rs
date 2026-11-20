//! Compact batch mutation bodies (ADR-014), read into the options map the
//! typed form would carry so the application reads both alike.
//!
//! Tags: `0x92`/`0x93` complete or fail, `0x97`/`0x98` retry, `0x99`/`0x9A`
//! cancel, `0x9B`/`0x9C` transition; the second of each pair asks for a
//! single `OK` on success. Layout after the tag: transition's from and to
//! states, the batch partition (`u32::MAX` length for none), `now_ms`,
//! retry's and transition's `run_at_ms`, an independence byte (`0` default,
//! `1` false, `2` true, `3`/`4` true/false and terminal-local-only), the item
//! count, then per item the id, its partition, its lease token (none for
//! cancel) and its fence; a transition item has its fence, then an optional
//! lease token.

use wf_application::commands::flow::Term;

use crate::codec::Reader;
use crate::frame::FLAG_CUSTOM_PAYLOAD;

use super::RequestError;

/// Smallest item: id, partition and fence.
const MIN_ITEM_BYTES: usize = 16;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Lease {
    Required,
    Optional,
    None,
}

fn layout(name: &str, tag: u8) -> Option<(bool, bool, Lease, bool)> {
    // (ok on success, has run_at, lease, has states)
    Some(match (name, tag) {
        ("FLOW.COMPLETE_MANY" | "FLOW.FAIL_MANY", 0x92) => (false, false, Lease::Required, false),
        ("FLOW.COMPLETE_MANY" | "FLOW.FAIL_MANY", 0x93) => (true, false, Lease::Required, false),
        ("FLOW.RETRY_MANY", 0x97) => (false, true, Lease::Required, false),
        ("FLOW.RETRY_MANY", 0x98) => (true, true, Lease::Required, false),
        ("FLOW.CANCEL_MANY", 0x99) => (false, false, Lease::None, false),
        ("FLOW.CANCEL_MANY", 0x9A) => (true, false, Lease::None, false),
        ("FLOW.TRANSITION_MANY", 0x9B) => (false, true, Lease::Optional, true),
        ("FLOW.TRANSITION_MANY", 0x9C) => (true, true, Lease::Optional, true),
        _ => return None,
    })
}

fn bin(bytes: &[u8]) -> Term {
    Term::Bin(bytes.to_vec())
}

fn entry(key: &str, value: Term) -> (Vec<u8>, Term) {
    (key.as_bytes().to_vec(), value)
}

fn item(reader: &mut Reader<'_>, lease: Lease) -> Option<Term> {
    let mut fields = vec![entry("id", bin(reader.bin()?))];
    if let Some(partition) = reader.optional_bin().ok()? {
        fields.push(entry("partition_key", bin(partition)));
    }
    // Transition items put the fence before their optional lease token.
    let (token, fence) = match lease {
        Lease::Required => (Some(reader.bin()?), reader.i64()?),
        Lease::Optional => {
            let fence = reader.i64()?;
            (reader.optional_bin().ok()?, fence)
        }
        Lease::None => (None, reader.i64()?),
    };
    if let Some(token) = token {
        fields.push(entry("lease_token", bin(token)));
    }
    fields.push(entry("fencing_token", Term::Int(fence)));
    Some(Term::Map(fields))
}

fn read(name: &str, body: &[u8]) -> Option<Term> {
    let mut reader = Reader::new(body);
    let (ok_on_success, run_at, lease, states) = layout(name, reader.u8()?)?;
    let mut options = Vec::new();
    if states {
        options.push(entry("from_state", bin(reader.bin()?)));
        options.push(entry("to_state", bin(reader.bin()?)));
    }
    if let Some(partition) = reader.optional_bin().ok()? {
        options.push(entry("partition_key", bin(partition)));
    }
    options.push(entry("now_ms", Term::Int(reader.i64()?)));
    if run_at {
        options.push(entry("run_at_ms", Term::Int(reader.i64()?)));
    }
    match reader.u8()? {
        0 => {}
        1 | 4 => options.push(entry("independent", Term::Bool(false))),
        2 | 3 => options.push(entry("independent", Term::Bool(true))),
        _ => return None,
    }
    if ok_on_success {
        options.push(entry("return", Term::text("ok_on_success")));
    }
    let count = reader.count(MIN_ITEM_BYTES)?;
    let mut items = Vec::with_capacity(count);
    for _ in 0..count {
        items.push(item(&mut reader, lease)?);
    }
    options.push(entry("items", Term::List(items)));
    reader.is_empty().then_some(Term::Map(options))
}

/// Whether a frame body is compact.
#[must_use]
pub fn is_compact(flags: u8) -> bool {
    flags & FLAG_CUSTOM_PAYLOAD != 0
}

/// Reads a compact batch mutation body of command `name`.
///
/// # Errors
/// Refuses an unknown tag for the command and a truncated or over-long body.
pub fn many_options(name: &str, body: &[u8]) -> Result<Term, RequestError> {
    read(name, body)
        .ok_or_else(|| RequestError::new(format!("ERR native compact {name} payload is invalid")))
}
