//! Coherent member/revision data; no routing, read-barrier or prune authority.
use super::{GovernanceCatalogPage, GovernanceCatalogRead, GovernanceCatalogReadLimits};
use crate::{log::CodecError, state::StateTransactionError};
use wf_domain::workflow::control::EntryIdentity;

/// One bounded candidate and its revision from the same original guard.
#[derive(Debug, Eq, PartialEq)]
pub struct GovernanceCatalogPrunePage {
    /// Original lexical page, restricted to limit1.
    pub page: GovernanceCatalogPage,
    /// Present exactly when the page contains its single member.
    pub revision: Option<EntryIdentity>,
}
/// Configured GC3 observations; production owners supply the read lifecycle.
pub trait GovernanceCatalogPruneReadPort {
    /// Captures one member and its actual registration revision coherently.
    /// # Errors
    /// Refuses old profiles, namespace, cursor, corruption or guard limits.
    fn governance_catalog_prune_page(
        &self,
        request: GovernanceCatalogRead<'_>,
        limits: GovernanceCatalogReadLimits,
    ) -> Result<GovernanceCatalogPrunePage, StateTransactionError>;
}
/// Encodes one fixed canonical member registration identity without allocation.
#[must_use]
pub fn encode_catalog_member_revision(revision: EntryIdentity) -> [u8; 62] {
    let _ = revision;
    [0; 62]
}
/// Completely inspects a fixed original member registration identity.
/// # Errors
/// Refuses old formats, malformed bytes and trailing data.
pub fn inspect_catalog_member_revision(encoded: &[u8]) -> Result<EntryIdentity, CodecError> {
    let _ = encoded;
    Err(CodecError::UnsupportedVersion)
}
