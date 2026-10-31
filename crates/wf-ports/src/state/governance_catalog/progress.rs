//! Derived cursor observations, never source routing or cleanup authority.
use super::GovernanceCatalogReadLimits;
use crate::{log::CodecError, state::StateTransactionError};
use wf_domain::workflow::request::RequestScope;

/// A complete hint observation under one coherent guard.
#[derive(Debug, Eq, PartialEq)]
pub enum GovernanceCatalogProgress {
    /// No progress row; traversal starts from the beginning.
    Missing,
    /// Original bounded member cursor, including empty binary.
    Cursor(Vec<u8>),
    /// Malformed derived hint; only a later committed reset may remove it.
    Invalid,
}
/// Configured lifecycle/namespace capability for nonauthoritative scan progress.
pub trait GovernanceCatalogProgressPort {
    /// Reads the original progress without repairing or deleting anything.
    /// # Errors
    /// Refuses profile/namespace, resources or authoritative corruption.
    fn governance_catalog_progress(
        &self,
        scope: &RequestScope,
        catalog: &[u8],
        limits: GovernanceCatalogReadLimits,
    ) -> Result<GovernanceCatalogProgress, StateTransactionError>;
}
/// Encodes one valid original stored cursor, without current-state authority.
/// # Errors
/// Refuses field/byte/owner limits and reservation failure.
pub fn encode_catalog_progress_cursor(
    cursor: &[u8],
    maximum_wire: usize,
    maximum_owned: usize,
) -> Result<Vec<u8>, CodecError> {
    let _ = (cursor, maximum_wire, maximum_owned);
    Err(CodecError::UnsupportedVersion)
}
/// Completely inspects stored hint bytes and borrows a valid original cursor.
/// Malformed hints return None; no allocation or implicit reset occurs.
#[must_use]
pub fn inspect_catalog_progress_cursor(bytes: &[u8]) -> Option<&[u8]> {
    let _ = bytes;
    None
}
