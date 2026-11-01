//! Captured original metadata values, without policy or storage authority.
//!
//! Inputs are immutable, finite and acyclic. Map rows have unique source keys
//! under exact Erlang map-key equality and retain actual source reduce order.
//! Proper spines are flattened; improper prefixes are nonempty and terminate in
//! a non-list value. Acquiring this view remains the caller's responsibility.
//! Normalized outputs are ordinary editable values, not proof tokens.

mod attributes;
pub mod record;
mod state_meta;

pub use attributes::{
    normalize_original_attributes, valid_original_attribute_list, valid_original_attribute_scalar,
    valid_original_attributes,
};
pub use state_meta::{normalize_original_state_meta, valid_original_state_meta};

/// A complete captured class for the selected original metadata helpers.
///
/// Descriptor containers borrow `'rows`; payloads borrow `'leaf`. Nil and bool
/// spellings in `Atom` are aliases, as are equal mathematical integers, and
/// cannot occur as distinct keys of a captured map.
#[derive(Clone, Copy, Debug)]
pub enum OriginalMetadataInput<'rows, 'leaf> {
    /// The nil atom, distinct from an empty proper list.
    Nil,
    /// A boolean atom.
    Boolean(bool),
    /// A mathematical integer represented directly.
    Integer(i128),
    /// An arbitrary mathematical integer, allowing padding and negative zero.
    BigInteger {
        /// Mathematical sign; zero remains zero under either sign.
        negative: bool,
        /// Borrowed magnitude in little-endian order.
        little_endian: &'leaf [u8],
    },
    /// A floating-point value, retaining its IEEE bits.
    Float(f64),
    /// Borrowed atom spelling, without interning.
    Atom(&'leaf str),
    /// Opaque binary bytes.
    Binary(&'leaf [u8]),
    /// Unique source-key rows in actual captured reduce order.
    Map(&'rows [(Self, Self)]),
    /// A flattened proper list.
    List(&'rows [Self]),
    /// A nonempty cons prefix with an actual non-list terminal tail.
    ImproperList {
        /// Borrowed, nonempty prefix descriptors.
        prefix: &'rows [Self],
        /// Actual terminal tail; the nil atom is not a proper-list terminator.
        tail: &'rows Self,
    },
    /// Remaining classes indistinguishable across the six selected helpers.
    Other,
}

/// A normalized scalar; binary backing remains borrowed.
#[derive(Clone, Copy, Debug)]
pub enum OriginalMetadataScalar<'leaf> {
    /// The nil atom.
    Nil,
    /// A boolean atom.
    Boolean(bool),
    /// A signed 64-bit integer.
    Integer(i64),
    /// A finite floating-point value emitted by a successful normalizer.
    Float(f64),
    /// Opaque binary bytes, including normalized atom spellings.
    Binary(&'leaf [u8]),
}

/// A normalized attribute scalar or owned list of borrowed binary values.
#[derive(Debug)]
pub enum OriginalAttributeValue<'leaf> {
    /// One normalized scalar.
    Scalar(OriginalMetadataScalar<'leaf>),
    /// Borrowed values in surviving-last-occurrence order.
    List(Vec<&'leaf [u8]>),
}

/// Owned attribute rows borrowing only leaf data.
#[derive(Debug)]
pub struct OriginalAttributes<'leaf> {
    /// Unique rows in ascending surviving-last-source-index order.
    pub entries: Vec<(&'leaf str, OriginalAttributeValue<'leaf>)>,
}

/// Owned state and scalar rows borrowing only leaf data.
#[derive(Debug)]
pub struct OriginalStateMeta<'leaf> {
    /// Unique outer and inner rows in surviving-last-source-index order.
    pub states: Vec<(&'leaf str, Vec<(&'leaf str, OriginalMetadataScalar<'leaf>)>)>,
}

/// Distinct source-returned, source-raised and operational outcomes.
#[derive(Debug)]
pub enum OriginalMetadataError {
    /// The original static business-error bytes.
    SourceReturned(&'static str),
    /// The source raised an argument error without a Rust panic.
    RaisedArgumentError,
    /// The actual fallible output-reservation failure.
    ReserveFailed(std::collections::TryReserveError),
}

#[derive(Clone, Copy)]
enum NameKind {
    Attribute,
    State,
    StateKey,
}

const MAX_ATTRIBUTES: usize = 16;
const MAX_NAME_BYTES: usize = 64;
const MAX_VALUE_BYTES: usize = 256;
const MAX_ATTRIBUTE_LIST_VALUES: usize = 16;
const MAX_ATTRIBUTE_BYTES: usize = 2_048;
const MAX_STATES: usize = 64;
const MAX_STATE_ENTRIES: usize = 16;
const MAX_STATE_META_BYTES: usize = 16_384;

fn atom_spelling<'leaf>(value: OriginalMetadataInput<'_, 'leaf>) -> Option<&'leaf str> {
    match value {
        OriginalMetadataInput::Nil => Some("nil"),
        OriginalMetadataInput::Boolean(true) => Some("true"),
        OriginalMetadataInput::Boolean(false) => Some("false"),
        OriginalMetadataInput::Atom(name) => Some(name),
        _ => None,
    }
}

fn integer_i64(value: OriginalMetadataInput<'_, '_>) -> Option<i64> {
    match value {
        OriginalMetadataInput::Integer(value) => i64::try_from(value).ok(),
        OriginalMetadataInput::BigInteger {
            negative,
            little_endian,
        } => {
            let mut magnitude = 0_u64;
            for (index, &byte) in little_endian.iter().enumerate() {
                if index < 8 {
                    magnitude |= u64::from(byte) << (8 * index);
                } else if byte != 0 {
                    return None;
                }
            }
            if negative && magnitude == i64::MIN.unsigned_abs() {
                Some(i64::MIN)
            } else {
                i64::try_from(magnitude)
                    .ok()
                    .map(|value| if negative { -value } else { value })
            }
        }
        _ => None,
    }
}

fn normalize_name<'leaf>(
    value: OriginalMetadataInput<'_, 'leaf>,
    kind: NameKind,
) -> Result<&'leaf str, OriginalMetadataError> {
    let (non_string, invalid_utf8, empty, large, reserved) = match kind {
        NameKind::Attribute => (
            "ERR flow attribute key must be a string",
            "ERR flow attribute key must be valid UTF-8",
            "ERR flow attribute key must not be empty",
            "ERR flow attribute key too large",
            Some("ERR flow attribute key is reserved"),
        ),
        NameKind::State => (
            "ERR flow state_meta state must be a string",
            "ERR flow state_meta state must be valid UTF-8",
            "ERR flow state_meta state must not be empty",
            "ERR flow state_meta state too large",
            None,
        ),
        NameKind::StateKey => (
            "ERR flow state_meta key must be a string",
            "ERR flow state_meta key must be valid UTF-8",
            "ERR flow state_meta key must not be empty",
            "ERR flow state_meta key too large",
            Some("ERR flow state_meta key is reserved"),
        ),
    };
    let name = match value {
        OriginalMetadataInput::Binary(bytes) => std::str::from_utf8(bytes)
            .map_err(|_| OriginalMetadataError::SourceReturned(invalid_utf8))?,
        _ => atom_spelling(value).ok_or(OriginalMetadataError::SourceReturned(non_string))?,
    };
    let name = name.trim();
    if name.is_empty() {
        return Err(OriginalMetadataError::SourceReturned(empty));
    }
    if name.len() > MAX_NAME_BYTES {
        return Err(OriginalMetadataError::SourceReturned(large));
    }
    if name.starts_with("__")
        && let Some(error) = reserved
    {
        return Err(OriginalMetadataError::SourceReturned(error));
    }
    Ok(name)
}

fn canonical_name(value: OriginalMetadataInput<'_, '_>, kind: NameKind) -> bool {
    let OriginalMetadataInput::Binary(bytes) = value else {
        return false;
    };
    if bytes.is_empty() || bytes.len() > MAX_NAME_BYTES {
        return false;
    }
    normalize_name(value, kind).is_ok_and(|name| name.as_bytes() == bytes)
}

fn normalize_scalar<'leaf>(
    value: OriginalMetadataInput<'_, 'leaf>,
    attributes: bool,
) -> Result<OriginalMetadataScalar<'leaf>, OriginalMetadataError> {
    let (large, integer, float, shape) = if attributes {
        (
            "ERR flow attribute value too large",
            "ERR flow attribute integer must fit in signed 64 bits",
            "ERR flow attribute float must be finite",
            "ERR flow attribute value must be scalar or string list",
        )
    } else {
        (
            "ERR flow state_meta value too large",
            "ERR flow state_meta integer must fit in signed 64 bits",
            "ERR flow state_meta float must be finite",
            "ERR flow state_meta value must be scalar",
        )
    };
    match value {
        OriginalMetadataInput::Nil | OriginalMetadataInput::Atom("nil") => {
            Ok(OriginalMetadataScalar::Nil)
        }
        OriginalMetadataInput::Boolean(value) => Ok(OriginalMetadataScalar::Boolean(value)),
        OriginalMetadataInput::Atom("true") => Ok(OriginalMetadataScalar::Boolean(true)),
        OriginalMetadataInput::Atom("false") => Ok(OriginalMetadataScalar::Boolean(false)),
        OriginalMetadataInput::Integer(_) | OriginalMetadataInput::BigInteger { .. } => {
            integer_i64(value)
                .map(OriginalMetadataScalar::Integer)
                .ok_or(OriginalMetadataError::SourceReturned(integer))
        }
        OriginalMetadataInput::Float(value) if value.is_finite() => {
            Ok(OriginalMetadataScalar::Float(value))
        }
        OriginalMetadataInput::Float(_) => Err(OriginalMetadataError::SourceReturned(float)),
        OriginalMetadataInput::Binary(bytes) if bytes.len() <= MAX_VALUE_BYTES => {
            Ok(OriginalMetadataScalar::Binary(bytes))
        }
        OriginalMetadataInput::Atom(name) if name.len() <= MAX_VALUE_BYTES => {
            Ok(OriginalMetadataScalar::Binary(name.as_bytes()))
        }
        OriginalMetadataInput::Binary(_) | OriginalMetadataInput::Atom(_) => {
            Err(OriginalMetadataError::SourceReturned(large))
        }
        _ => Err(OriginalMetadataError::SourceReturned(shape)),
    }
}

fn canonical_scalar(value: OriginalMetadataInput<'_, '_>) -> bool {
    match value {
        OriginalMetadataInput::Nil
        | OriginalMetadataInput::Boolean(_)
        | OriginalMetadataInput::Atom("nil" | "true" | "false") => true,
        OriginalMetadataInput::Integer(_) | OriginalMetadataInput::BigInteger { .. } => {
            integer_i64(value).is_some()
        }
        OriginalMetadataInput::Float(value) => value.is_finite(),
        OriginalMetadataInput::Binary(bytes) => bytes.len() <= MAX_VALUE_BYTES,
        _ => false,
    }
}

fn scalar_size(value: OriginalMetadataScalar<'_>) -> usize {
    match value {
        OriginalMetadataScalar::Nil => 0,
        OriginalMetadataScalar::Boolean(_) => 1,
        OriginalMetadataScalar::Integer(_) | OriginalMetadataScalar::Float(_) => 8,
        OriginalMetadataScalar::Binary(bytes) => bytes.len(),
    }
}

fn is_last_name(
    rows: &[(OriginalMetadataInput<'_, '_>, OriginalMetadataInput<'_, '_>)],
    index: usize,
    kind: NameKind,
) -> Result<bool, OriginalMetadataError> {
    let name = normalize_name(rows[index].0, kind)?;
    for &(key, _) in &rows[index + 1..] {
        if normalize_name(key, kind)? == name {
            return Ok(false);
        }
    }
    Ok(true)
}
