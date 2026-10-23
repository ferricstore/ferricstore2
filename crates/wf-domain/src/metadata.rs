//! Captured original metadata values, without policy or storage authority.
//!
//! Inputs are immutable, finite and acyclic. Map rows have unique source keys
//! under exact Erlang map-key equality and retain actual source reduce order.
//! Proper spines are flattened; improper prefixes are nonempty and terminate in
//! a non-list value. Acquiring this view remains the caller's responsibility.
//! Normalized outputs are ordinary editable values, not proof tokens.

mod attributes;
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
