//! FSNP typed values: `0` nil, `1` true, `2` false, `3` i64, `4` binary,
//! `5` array, `6` map with binary keys, `7` f64 and `8` u64. Lengths and
//! counts are big-endian `u32`.

use std::fmt;

use super::Reader;

/// Deepest accepted nesting of arrays and maps.
const MAX_DEPTH: usize = 32;
/// Most array items plus map entries accepted in one value.
const MAX_ITEMS: usize = 1_000_000;
/// Smallest encoded value: a tag byte.
const MIN_VALUE_BYTES: usize = 1;
/// Smallest encoded map entry: a key length and a value tag.
const MIN_ENTRY_BYTES: usize = 5;

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Value {
    Nil,
    Bool(bool),
    Int(i64),
    Bin(Vec<u8>),
    Arr(Vec<Value>),
    Map(Vec<(Vec<u8>, Value)>),
    Float(f64),
    Unsigned(u64),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DecodeError {
    Truncated,
    UnknownTag(u8),
    TooDeep,
    TooManyItems,
    TrailingBytes,
}

impl fmt::Display for DecodeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Truncated => formatter.write_str("truncated value"),
            Self::UnknownTag(tag) => write!(formatter, "unknown value tag {tag}"),
            Self::TooDeep => formatter.write_str("value nesting too deep"),
            Self::TooManyItems => formatter.write_str("too many collection items"),
            Self::TrailingBytes => formatter.write_str("trailing bytes after value"),
        }
    }
}

/// A length that does not fit the protocol's `u32` length field.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TooLong;

impl Value {
    /// Decodes exactly one value that spans all of `bytes`.
    pub(crate) fn decode_exact(bytes: &[u8]) -> Result<Self, DecodeError> {
        let mut reader = Reader::new(bytes);
        let mut budget = MAX_ITEMS;
        let value = decode(&mut reader, MAX_DEPTH, &mut budget)?;
        if reader.is_empty() {
            Ok(value)
        } else {
            Err(DecodeError::TrailingBytes)
        }
    }

    pub(crate) fn encode(&self, out: &mut Vec<u8>) -> Result<(), TooLong> {
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
                put_bin(bytes, out)?;
            }
            Self::Arr(items) => {
                out.push(5);
                put_len(items.len(), out)?;
                for item in items {
                    item.encode(out)?;
                }
            }
            Self::Map(entries) => {
                out.push(6);
                put_len(entries.len(), out)?;
                for (key, value) in entries {
                    put_bin(key, out)?;
                    value.encode(out)?;
                }
            }
            Self::Float(value) => {
                out.push(7);
                out.extend_from_slice(&value.to_bits().to_be_bytes());
            }
            Self::Unsigned(value) => {
                out.push(8);
                out.extend_from_slice(&value.to_be_bytes());
            }
        }
        Ok(())
    }

    pub(crate) fn as_bytes(&self) -> Option<&[u8]> {
        match self {
            Self::Bin(bytes) => Some(bytes),
            _ => None,
        }
    }

    pub(crate) fn as_i64(&self) -> Option<i64> {
        match self {
            Self::Int(value) => Some(*value),
            Self::Unsigned(value) => i64::try_from(*value).ok(),
            _ => None,
        }
    }

    pub(crate) fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(value) => Some(*value),
            _ => None,
        }
    }
}

/// Appends a `u32` length.
pub(crate) fn put_len(length: usize, out: &mut Vec<u8>) -> Result<(), TooLong> {
    let length = u32::try_from(length).map_err(|_| TooLong)?;
    out.extend_from_slice(&length.to_be_bytes());
    Ok(())
}

/// Appends a length-prefixed binary without a type tag.
pub(crate) fn put_bin(bytes: &[u8], out: &mut Vec<u8>) -> Result<(), TooLong> {
    put_len(bytes.len(), out)?;
    out.extend_from_slice(bytes);
    Ok(())
}

fn decode(reader: &mut Reader<'_>, depth: usize, budget: &mut usize) -> Result<Value, DecodeError> {
    let tag = reader.u8().ok_or(DecodeError::Truncated)?;
    Ok(match tag {
        0 => Value::Nil,
        1 => Value::Bool(true),
        2 => Value::Bool(false),
        3 => Value::Int(reader.i64().ok_or(DecodeError::Truncated)?),
        4 => Value::Bin(reader.bin().ok_or(DecodeError::Truncated)?.to_vec()),
        5 => {
            let (count, inner) = collection(reader, depth, budget, MIN_VALUE_BYTES)?;
            let mut items = Vec::with_capacity(count);
            for _ in 0..count {
                items.push(decode(reader, inner, budget)?);
            }
            Value::Arr(items)
        }
        6 => {
            let (count, inner) = collection(reader, depth, budget, MIN_ENTRY_BYTES)?;
            let mut entries = Vec::with_capacity(count);
            for _ in 0..count {
                let key = reader.bin().ok_or(DecodeError::Truncated)?.to_vec();
                entries.push((key, decode(reader, inner, budget)?));
            }
            Value::Map(entries)
        }
        7 => Value::Float(f64::from_bits(reader.u64().ok_or(DecodeError::Truncated)?)),
        8 => Value::Unsigned(reader.u64().ok_or(DecodeError::Truncated)?),
        other => return Err(DecodeError::UnknownTag(other)),
    })
}

/// Reads a collection count, charging it to the item budget and checking it
/// against the remaining input. Returns the count and the nested depth limit.
fn collection(
    reader: &mut Reader<'_>,
    depth: usize,
    budget: &mut usize,
    min_item_bytes: usize,
) -> Result<(usize, usize), DecodeError> {
    let inner = depth.checked_sub(1).ok_or(DecodeError::TooDeep)?;
    let count = usize::try_from(reader.u32().ok_or(DecodeError::Truncated)?)
        .map_err(|_| DecodeError::TooManyItems)?;
    *budget = budget.checked_sub(count).ok_or(DecodeError::TooManyItems)?;
    if !reader.fits(count, min_item_bytes) {
        return Err(DecodeError::Truncated);
    }
    Ok((count, inner))
}
