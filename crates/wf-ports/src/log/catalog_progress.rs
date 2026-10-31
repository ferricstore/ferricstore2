//! Closed original progress profile, distinct from public/generic preparation.
use super::{CodecError as E, CodecLimits, PreparedBytes};
use wf_domain::{
    governance::catalog::CatalogProgressChange,
    workflow::{
        admission::AdmissionLimits,
        request::{ClientIntent, RequestScope, RetainedStatus},
    },
};
use wf_types::{identity::RequestId, version::CommittedTime};
mod model;
pub use model::PreparedCatalogProgressRequest;

/// Derives original mathematical intent without admission or commit authority.
/// # Errors
/// Refuses malformed original fields.
pub fn catalog_progress_request_intent(
    command: CatalogProgressChange<'_>,
) -> Result<ClientIntent, E> {
    let _ = command;
    Err(E::UnsupportedVersion)
}
/// Encodes the complete original private request.
/// # Errors
/// Refuses invalid fields, wire/owner limits and allocation failure.
pub fn encode_catalog_progress_request(
    scope: &RequestScope,
    request: &RequestId,
    stamp: CommittedTime,
    command: CatalogProgressChange<'_>,
    limits: CodecLimits,
    maximum_owned: usize,
) -> Result<PreparedBytes, E> {
    let _ = (scope, request, stamp, command, limits, maximum_owned);
    Err(E::UnsupportedVersion)
}
/// Validates complete framing/digest/EOF before creating header owners.
/// # Errors
/// Refuses other profiles, malformed bytes and admission/allocation failure.
pub fn prepare_catalog_progress_request(
    encoded: &[u8],
    limits: CodecLimits,
    maximum_owned: usize,
) -> Result<PreparedCatalogProgressRequest<'_>, E> {
    let _ = (encoded, limits, maximum_owned);
    Err(E::UnsupportedVersion)
}
/// Borrows an explicit header; complete body audit is still mandatory.
/// # Errors
/// Refuses other profiles, invalid headers and byte limits.
pub fn catalog_progress_recovery_header(
    encoded: &[u8],
    limits: CodecLimits,
) -> Result<(u16, [&[u8]; 3], CommittedTime), E> {
    let _ = (encoded, limits);
    Err(E::UnsupportedVersion)
}
/// Verifies the exact complete private retained success response.
/// # Errors
/// Refuses changed bytes, profiles or wire limits.
pub fn catalog_progress_retained_summary(
    intent: ClientIntent,
    version: u16,
    response: &[u8],
    admission: AdmissionLimits,
) -> Result<(RetainedStatus, Option<usize>), E> {
    let _ = (intent, version, response, admission);
    Err(E::UnsupportedVersion)
}
