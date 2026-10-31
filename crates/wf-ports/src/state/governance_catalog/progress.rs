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
    if cursor.len() > 65_535 {
        return Err(CodecError::KeyLimit);
    }
    let size = cursor
        .len()
        .checked_add(10)
        .ok_or(CodecError::LengthOverflow)?;
    if size > maximum_wire {
        return Err(CodecError::ByteLimit);
    }
    if size.checked_add(128).ok_or(CodecError::LengthOverflow)? > maximum_owned {
        return Err(CodecError::OwnedLimit);
    }
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(size)
        .map_err(|_| CodecError::Allocation)?;
    bytes.extend_from_slice(b"FWPC\0\x01");
    bytes.extend_from_slice(
        &u32::try_from(cursor.len())
            .map_err(|_| CodecError::LengthOverflow)?
            .to_be_bytes(),
    );
    bytes.extend_from_slice(cursor);
    Ok(bytes)
}
/// Completely inspects stored hint bytes and borrows a valid original cursor.
/// Malformed hints return None; no allocation or implicit reset occurs.
#[must_use]
pub fn inspect_catalog_progress_cursor(bytes: &[u8]) -> Option<&[u8]> {
    if bytes.get(..6)? != b"FWPC\0\x01" {
        return None;
    }
    let n = usize::try_from(u32::from_be_bytes(bytes.get(6..10)?.try_into().ok()?)).ok()?;
    let cursor = bytes.get(10..)?;
    (n <= 65_535 && cursor.len() == n).then_some(cursor)
}
