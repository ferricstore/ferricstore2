//! Neutral complete member identities; encoded bytes never grant routing authority.

use crate::log::CodecError;
use wf_domain::workflow::request::RequestScope;

/// Borrowed original identity; tenant is supplied by the verified catalog scope.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CreditCatalogMemberRef<'a> {
    /// Original group bytes, requiring authoritative routing verification.
    pub group: &'a [u8],
    /// Original recovery bytes, not a current-epoch certificate.
    pub recovery: &'a [u8],
    /// Exact original credit scope.
    pub credit_scope: &'a [u8],
}

/// One bounded flat byte owner with ranges; no per-member payload allocation.
pub struct EncodedCreditCatalogMembers {
    bytes: Vec<u8>,
    ranges: Vec<(usize, usize)>,
    owned_bytes: usize,
}
impl EncodedCreditCatalogMembers {
    /// Visits complete encoded members, borrowing their one original byte owner.
    pub fn members(&self) -> impl ExactSizeIterator<Item = &[u8]> {
        self.ranges.iter().map(|(a, b)| &self.bytes[*a..*b])
    }
    /// Logical prospective flat/range/reference cohort size, without payment proof.
    #[must_use]
    pub const fn owned_bytes(&self) -> usize {
        self.owned_bytes
    }
}

/// Encodes at most256 original identities in one prospectively bounded cohort.
/// # Errors
/// Refuses invalid fields, wire/owner ceilings or allocation failure.
pub fn encode_credit_catalog_members(
    _source: &RequestScope,
    _scopes: &[&[u8]],
    _maximum_wire_bytes: usize,
    _maximum_owned_bytes: usize,
) -> Result<EncodedCreditCatalogMembers, CodecError> {
    Err(CodecError::UnsupportedVersion)
}

/// Fully inspects one member and borrows its original fields without allocation.
/// # Errors
/// Refuses unsupported/noncanonical/truncated/trailing or oversized bytes.
pub fn decode_credit_catalog_member(
    _encoded: &[u8],
    _maximum_wire_bytes: usize,
) -> Result<CreditCatalogMemberRef<'_>, CodecError> {
    Err(CodecError::UnsupportedVersion)
}
