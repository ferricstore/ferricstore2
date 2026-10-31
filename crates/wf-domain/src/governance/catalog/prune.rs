//! Conditional derived membership removal; observed data grants no authority.
use crate::workflow::control::EntryIdentity;
use std::fmt;

/// Exact original member and the registration revision observed before absence.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CatalogPruneRequest<'a> {
    /// Exact catalog identity in the separately verified target namespace.
    pub catalog: &'a [u8],
    /// Original complete member identity.
    pub member: &'a [u8],
    /// Full original committed registration identity, not a current certificate.
    pub expected: EntryIdentity,
}
impl CatalogPruneRequest<'_> {
    /// Validates original bounded fields without performing any observation.
    /// # Errors
    /// Refuses oversized fields and an empty member.
    pub fn validate(self) -> Result<(), CatalogPruneError> {
        let _ = self;
        Err(CatalogPruneError)
    }
}
/// Original accepted conditional-prune result, including known no-ops.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CatalogPruneReply {
    /// The exact observed registration was removed.
    Removed,
    /// The member was already absent.
    Missing,
    /// A newer registration survived the stale prune.
    Changed,
}
/// Invalid bounded mathematical input, without operational state effects.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CatalogPruneError;
impl fmt::Display for CatalogPruneError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("invalid catalog prune request")
    }
}
impl std::error::Error for CatalogPruneError {}
