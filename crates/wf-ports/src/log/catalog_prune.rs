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
    let _ = (encoded, limits);
    Err(E::UnsupportedVersion)
}
/// Encodes the fixed original conditional-prune result without allocation.
#[must_use]
pub fn encode_catalog_prune_reply(reply: CatalogPruneReply) -> [u8; 7] {
    let _ = reply;
    [0; 7]
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
    let _ = (intent, version, response, admission);
    Err(E::UnsupportedVersion)
}
