//! Elixir-shaped values exchanged with transports (ADR-014).
//!
//! A request's options arrive as a map of these values and every reply is one.
//! They mirror the terms the Elixir server decodes from and encodes to its
//! typed wire values: nil, booleans, 64-bit integers, floats, binaries, lists
//! and maps with binary keys in insertion order.

/// One value.
#[derive(Clone, Debug, PartialEq)]
pub enum Term {
    /// Elixir `nil`.
    Nil,
    /// A boolean.
    Bool(bool),
    /// A signed integer.
    Int(i64),
    /// A float.
    Float(f64),
    /// A binary.
    Bin(Vec<u8>),
    /// A list.
    List(Vec<Term>),
    /// A map with binary keys, in insertion order.
    Map(Vec<(Vec<u8>, Term)>),
}

impl Term {
    /// A binary from text.
    #[must_use]
    pub fn text(value: &str) -> Self {
        Self::Bin(value.as_bytes().to_vec())
    }

    /// A binary, or nil when absent.
    #[must_use]
    pub fn optional_bin(value: Option<&[u8]>) -> Self {
        value.map_or(Self::Nil, |bytes| Self::Bin(bytes.to_vec()))
    }

    /// An integer, or nil when absent; values above `i64::MAX` saturate.
    #[must_use]
    pub fn optional_int(value: Option<u64>) -> Self {
        value.map_or(Self::Nil, Self::int)
    }

    /// An integer; values above `i64::MAX` saturate.
    #[must_use]
    pub fn int(value: u64) -> Self {
        Self::Int(i64::try_from(value).unwrap_or(i64::MAX))
    }

    /// The value under `key` when this is a map.
    #[must_use]
    pub fn get(&self, key: &[u8]) -> Option<&Self> {
        match self {
            Self::Map(entries) => entries
                .iter()
                .find(|(name, _)| name.as_slice() == key)
                .map(|(_, value)| value),
            _ => None,
        }
    }
}

/// Builds a map term from static keys.
#[must_use]
pub fn map(entries: Vec<(&str, Term)>) -> Term {
    Term::Map(
        entries
            .into_iter()
            .map(|(key, value)| (key.as_bytes().to_vec(), value))
            .collect(),
    )
}

/// Deepest nesting [`Term::decode`] accepts.
const MAX_DEPTH: usize = 32;

impl Term {
    /// Appends the binary form: a tag byte (`0` nil, `1` true, `2` false,
    /// `3` integer, `4` binary, `5` list, `6` map, `7` float), then
    /// big-endian fields, with `u32` lengths and counts. `None` when a length
    /// does not fit.
    pub fn encode(&self, out: &mut Vec<u8>) -> Option<()> {
        match self {
            Self::Nil => out.push(0),
            Self::Bool(true) => out.push(1),
            Self::Bool(false) => out.push(2),
            Self::Int(value) => {
                out.push(3);
                out.extend_from_slice(&value.to_be_bytes());
            }
            Self::Bin(bytes) => {
                out.push(4);
                put_bin(out, bytes)?;
            }
            Self::List(items) => {
                out.push(5);
                out.extend_from_slice(&u32::try_from(items.len()).ok()?.to_be_bytes());
                for item in items {
                    item.encode(out)?;
                }
            }
            Self::Map(entries) => {
                out.push(6);
                out.extend_from_slice(&u32::try_from(entries.len()).ok()?.to_be_bytes());
                for (key, value) in entries {
                    put_bin(out, key)?;
                    value.encode(out)?;
                }
            }
            Self::Float(value) => {
                out.push(7);
                out.extend_from_slice(&value.to_bits().to_be_bytes());
            }
        }
        Some(())
    }

    /// Decodes one value from the front of `bytes`; returns it and the rest.
    #[must_use]
    pub fn decode(bytes: &[u8]) -> Option<(Self, &[u8])> {
        decode(bytes, MAX_DEPTH)
    }
}

fn put_bin(out: &mut Vec<u8>, bytes: &[u8]) -> Option<()> {
    out.extend_from_slice(&u32::try_from(bytes.len()).ok()?.to_be_bytes());
    out.extend_from_slice(bytes);
    Some(())
}

fn take(bytes: &[u8], length: usize) -> Option<(&[u8], &[u8])> {
    bytes.split_at_checked(length)
}

fn take_u32(bytes: &[u8]) -> Option<(usize, &[u8])> {
    let (head, rest) = take(bytes, 4)?;
    let length = u32::from_be_bytes(head.try_into().ok()?);
    Some((usize::try_from(length).ok()?, rest))
}

fn take_bin(bytes: &[u8]) -> Option<(Vec<u8>, &[u8])> {
    let (length, rest) = take_u32(bytes)?;
    let (value, rest) = take(rest, length)?;
    Some((value.to_vec(), rest))
}

fn decode(bytes: &[u8], depth: usize) -> Option<(Term, &[u8])> {
    let depth = depth.checked_sub(1)?;
    let (&tag, rest) = bytes.split_first()?;
    match tag {
        0 => Some((Term::Nil, rest)),
        1 => Some((Term::Bool(true), rest)),
        2 => Some((Term::Bool(false), rest)),
        3 => {
            let (value, rest) = take(rest, 8)?;
            Some((Term::Int(i64::from_be_bytes(value.try_into().ok()?)), rest))
        }
        4 => take_bin(rest).map(|(value, rest)| (Term::Bin(value), rest)),
        5 => {
            let (count, mut rest) = take_u32(rest)?;
            let mut items = Vec::new();
            for _ in 0..count {
                let (item, after) = decode(rest, depth)?;
                items.push(item);
                rest = after;
            }
            Some((Term::List(items), rest))
        }
        6 => {
            let (count, mut rest) = take_u32(rest)?;
            let mut entries = Vec::new();
            for _ in 0..count {
                let (key, after) = take_bin(rest)?;
                let (value, after) = decode(after, depth)?;
                entries.push((key, value));
                rest = after;
            }
            Some((Term::Map(entries), rest))
        }
        7 => {
            let (value, rest) = take(rest, 8)?;
            let bits = u64::from_be_bytes(value.try_into().ok()?);
            Some((Term::Float(f64::from_bits(bits)), rest))
        }
        _ => None,
    }
}
