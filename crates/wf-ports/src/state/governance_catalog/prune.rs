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
    let mut bytes = [0; 62];
    bytes[..6].copy_from_slice(b"FWGM\0\x02");
    let log = revision.log();
    for (chunk, n) in bytes[6..30].as_chunks_mut::<8>().0.iter_mut().zip([
        log.term(),
        log.leader_node(),
        log.index().get(),
    ]) {
        chunk.copy_from_slice(&n.to_be_bytes());
    }
    bytes[30..].copy_from_slice(revision.prepared().as_bytes());
    bytes
}
/// Completely inspects a fixed original member registration identity.
/// # Errors
/// Refuses old formats, malformed bytes and trailing data.
pub fn inspect_catalog_member_revision(encoded: &[u8]) -> Result<EntryIdentity, CodecError> {
    use wf_domain::workflow::control::PreparedDigest;
    use wf_types::version::{AppliedPosition, LogIdentity};
    if encoded.len() != 62 || encoded.get(..6) != Some(b"FWGM\0\x02") {
        return Err(CodecError::Malformed);
    }
    let mut numbers = [0; 3];
    for (n, chunk) in numbers
        .iter_mut()
        .zip(encoded[6..30].as_chunks::<8>().0.iter())
    {
        *n = u64::from_be_bytes(*chunk);
    }
    let digest = PreparedDigest::new(
        encoded[30..]
            .try_into()
            .map_err(|_| CodecError::Malformed)?,
    );
    Ok(EntryIdentity::new(
        LogIdentity::new(numbers[0], numbers[1], AppliedPosition::new(numbers[2])),
        digest,
    ))
}
