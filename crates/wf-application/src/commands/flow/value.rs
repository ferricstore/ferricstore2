//! `FLOW.VALUE.PUT` and `FLOW.VALUE.MGET` (ADR-014 step 5), read as
//! `Ferricstore.Flow.ValueStore` reads them, and value reference parsing.
//!
//! A put with both `owner_flow_id` and `name` writes a named value of that
//! Flow; any other put stores a shared value under a new reference, with an
//! optional time to live. A read takes references of any kind: a Flow's
//! payload, result or error (`p`, `r`, `e`), a named value (`n`) or a shared
//! value (`s`). The hash tag between braces routes each reference.

use super::FlowError;
use super::opts::Opts;
use super::record::DEFAULT_PAYLOAD_MAX_BYTES;
use super::term::Term;
use crate::routing::base64url_decode;

/// A value write.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValuePut {
    /// The bytes.
    pub value: Vec<u8>,
    /// Partition, if sent.
    pub partition_key: Option<Vec<u8>>,
    /// Owner and name of a named value.
    pub named: Option<(Vec<u8>, Vec<u8>)>,
    /// Replace a named value with different bytes.
    pub override_value: bool,
    /// Client clock.
    pub now_ms: Option<u64>,
    /// Lifetime of a shared value.
    pub ttl_ms: Option<u64>,
    /// The new reference of a shared value, chosen by the node before
    /// routing.
    pub shared_ref: Option<Vec<u8>>,
}

/// A value read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValueMget {
    /// References, in request order.
    pub refs: Vec<Vec<u8>>,
    /// Largest value returned inline.
    pub max_bytes: u64,
}

/// What a reference names.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ValueRef {
    /// A Flow's payload (`p`), result (`r`) or error (`e`) at a version.
    Flow {
        /// Kind letter.
        kind: u8,
        /// Flow id.
        id: Vec<u8>,
        /// Version that wrote it.
        version: u64,
    },
    /// A named value of a Flow at a version.
    Named {
        /// Owner Flow id.
        owner: Vec<u8>,
        /// Value name.
        name: Vec<u8>,
        /// Value version.
        version: u64,
    },
    /// A shared value, by its whole reference.
    Shared,
}

/// A parsed reference: its hash tag (between the braces) and target.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParsedRef {
    /// Hash tag content, which routes the reference.
    pub tag: Vec<u8>,
    /// What it names.
    pub target: ValueRef,
}

fn split_last(bytes: &[u8]) -> Option<(&[u8], u64)> {
    let colon = bytes.iter().rposition(|byte| *byte == b':')?;
    let (head, tail) = bytes.split_at(colon);
    let digits = tail.get(1..)?;
    let version = std::str::from_utf8(digits).ok()?.parse().ok()?;
    Some((head, version))
}

/// Parses `f:{<tag>}:v:<kind>:...`; `None` for anything else.
#[must_use]
pub fn parse_ref(reference: &[u8]) -> Option<ParsedRef> {
    let rest = reference.strip_prefix(b"f:{")?;
    let close = rest.iter().position(|byte| *byte == b'}')?;
    let (tag, rest) = rest.split_at(close);
    let rest = rest.strip_prefix(b"}:v:")?;
    let (&kind, rest) = rest.split_first()?;
    let rest = rest.strip_prefix(b":")?;
    let target = match kind {
        b'p' | b'r' | b'e' => {
            let (id, version) = split_last(rest)?;
            ValueRef::Flow {
                kind,
                id: id.to_vec(),
                version,
            }
        }
        b'n' => {
            let (head, version) = split_last(rest)?;
            let colon = head.iter().position(|byte| *byte == b':')?;
            let (owner, name) = head.split_at(colon);
            ValueRef::Named {
                owner: base64url_decode(owner)?,
                name: base64url_decode(name.get(1..)?)?,
                version,
            }
        }
        b's' => ValueRef::Shared,
        _ => return None,
    };
    (!tag.is_empty()).then(|| ParsedRef {
        tag: tag.to_vec(),
        target,
    })
}

const PUT_ALLOWED: &[&str] = &[
    "value",
    "partition_key",
    "owner_flow_id",
    "name",
    "override",
    "ttl_ms",
    "now_ms",
    "return",
];

/// Reads `FLOW.VALUE.PUT`.
///
/// # Errors
/// The Elixir server's refusal text for an invalid option.
pub fn parse_put(args: &Term) -> Result<ValuePut, FlowError> {
    let opts = Opts::new("FLOW.VALUE.PUT", args, PUT_ALLOWED)?;
    let value = match opts.get("value") {
        Some(Term::Bin(value)) => value.clone(),
        _ => return Err(FlowError::flow("value", "must be a binary")),
    };
    let partition_key = super::create::partition_key(&opts)?;
    let owner = opts.binary_or_nil("owner_flow_id")?;
    let name = opts.binary_or_nil("name")?;
    for (key, value) in [("owner_flow_id", &owner), ("name", &name)] {
        if value.as_ref().is_some_and(|value| value.len() > 256) {
            return Err(FlowError::rejected(format!(
                "ERR flow {key} too large (max 256 bytes)"
            )));
        }
    }
    let override_value = opts.boolean("override", false)?;
    let now_ms = opts.non_neg_integer("now_ms")?;
    let ttl_ms = opts.positive_integer("ttl_ms")?;
    let named = owner.zip(name);
    if named.is_some() && ttl_ms.is_some() {
        return Err(FlowError::rejected(
            "ERR flow ttl_ms is not supported for named values".to_owned(),
        ));
    }
    Ok(ValuePut {
        value,
        partition_key,
        named,
        override_value,
        now_ms,
        ttl_ms,
        shared_ref: None,
    })
}

/// A shared value's reference id from node-local entropy: the URL-safe
/// base64 of the first 18 bytes of its SHA-256, 24 characters as in Elixir.
#[must_use]
pub fn shared_id(seed: &[u8]) -> Vec<u8> {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(seed);
    crate::routing::base64url_nopad(digest.get(..18).unwrap_or_default()).into_bytes()
}

/// Reads `FLOW.VALUE.MGET`.
///
/// # Errors
/// The Elixir server's refusal text for an invalid option.
pub fn parse_mget(args: &Term) -> Result<ValueMget, FlowError> {
    let opts = Opts::new("FLOW.VALUE.MGET", args, &["refs", "max_bytes"])?;
    let refs = match opts.get("refs") {
        Some(Term::List(items)) => items
            .iter()
            .map(|item| match item {
                Term::Bin(reference) => Ok(reference.clone()),
                _ => Err(FlowError::rejected(
                    "ERR flow refs must be a list".to_owned(),
                )),
            })
            .collect::<Result<Vec<_>, _>>()?,
        _ => {
            return Err(FlowError::rejected(
                "ERR flow refs must be a list".to_owned(),
            ));
        }
    };
    let max_bytes = match opts.get("max_bytes") {
        None | Some(Term::Nil) => DEFAULT_PAYLOAD_MAX_BYTES,
        Some(Term::Int(value)) if *value >= 0 => u64::try_from(*value).unwrap_or(0),
        Some(_) => {
            return Err(FlowError::flow(
                "max_bytes",
                "must be a non-negative integer",
            ));
        }
    };
    Ok(ValueMget { refs, max_bytes })
}

/// One read value as Elixir replies: the bytes, nil when missing, or the
/// reference with its size when above `max_bytes`.
#[must_use]
pub fn read_term(reference: &[u8], value: Option<&[u8]>, max_bytes: u64) -> Term {
    match value {
        None => Term::Nil,
        Some(bytes) if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > max_bytes => {
            Term::Map(vec![
                (b"ref".to_vec(), Term::Bin(reference.to_vec())),
                (
                    b"value_size".to_vec(),
                    Term::int(u64::try_from(bytes.len()).unwrap_or(u64::MAX)),
                ),
                (b"value_omitted".to_vec(), Term::Bool(true)),
            ])
        }
        Some(bytes) => Term::Bin(bytes.to_vec()),
    }
}
