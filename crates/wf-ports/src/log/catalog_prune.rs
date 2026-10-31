//! Closed conditional prune request and complete original replies.
use super::{CodecError as E, CodecLimits, PreparedBytes};
use wf_domain::{
    governance::catalog::{CatalogPruneReply, CatalogPruneRequest},
    workflow::{
        admission::AdmissionLimits,
        request::{ClientIntent, RequestScope},
    },
};
use wf_types::{identity::RequestId, version::CommittedTime};
mod model;
mod request;
pub use model::PreparedCatalogPruneRequest;
/// Derives original conditional-prune identity from bounded canonical fields.
/// # Errors
/// Refuses invalid scope/member bounds.
pub fn catalog_prune_request_intent(command: CatalogPruneRequest<'_>) -> Result<ClientIntent, E> {
    request::intent(command)
}
/// Encodes exact private conditional-prune bytes.
/// # Errors
/// Refuses framing, resource and allocation failures.
pub fn encode_catalog_prune_request(
    scope: &RequestScope,
    id: &RequestId,
    stamp: CommittedTime,
    command: CatalogPruneRequest<'_>,
    limits: CodecLimits,
    owned: usize,
) -> Result<PreparedBytes, E> {
    request::encode(scope, id, stamp, command, limits, owned)
}
/// Verifies complete framing, original digest and EOF before ownership allocation.
/// # Errors
/// Refuses unsupported/corrupt or operationally inadmissible bytes.
pub fn prepare_catalog_prune_request(
    encoded: &[u8],
    limits: CodecLimits,
    owned: usize,
) -> Result<PreparedCatalogPruneRequest<'_>, E> {
    request::prepare(encoded, limits, owned)
}
/// Borrows an explicitly selected header; a full body audit is still required.
/// # Errors
/// Refuses unsupported/corrupt or oversized headers.
pub fn catalog_prune_recovery_header(
    encoded: &[u8],
    limits: CodecLimits,
) -> Result<(u16, [&[u8]; 3], CommittedTime), E> {
    if encoded.len() > limits.admission().decision_bytes() {
        return Err(E::ByteLimit);
    }
    let header = request::header(&mut super::reader::Reader::new(encoded, limits))?;
    Ok((
        16,
        [header.tenant, header.group, header.recovery],
        header.stamp,
    ))
}
/// Encodes the fixed original conditional-prune result without allocation.
#[must_use]
pub fn encode_catalog_prune_reply(reply: CatalogPruneReply) -> [u8; 7] {
    let tag = match reply {
        CatalogPruneReply::Removed => 0,
        CatalogPruneReply::Missing => 1,
        CatalogPruneReply::Changed => 2,
    };
    [b'F', b'W', b'X', b'R', 0, 1, tag]
}
/// Fully verifies the selected original private reply.
/// # Errors
/// Refuses other profiles, noncanonical decisions, EOF and byte limits.
pub fn catalog_prune_retained_summary(
    intent: ClientIntent,
    version: u16,
    response: &[u8],
    admission: AdmissionLimits,
) -> Result<CatalogPruneReply, E> {
    if intent.version() != 16 || version != 14 {
        return Err(E::UnsupportedVersion);
    }
    if response.len() > admission.decision_bytes() {
        return Err(E::ByteLimit);
    }
    if response.len() != 7 || response.get(..6) != Some(b"FWXR\0\x01") {
        return Err(E::Malformed);
    }
    match response[6] {
        0 => Ok(CatalogPruneReply::Removed),
        1 => Ok(CatalogPruneReply::Missing),
        2 => Ok(CatalogPruneReply::Changed),
        _ => Err(E::UnsupportedTag),
    }
}
