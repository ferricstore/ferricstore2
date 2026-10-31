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
mod request;
pub use model::PreparedCatalogProgressRequest;

/// Derives original mathematical intent without admission or commit authority.
/// # Errors
/// Refuses malformed original fields.
pub fn catalog_progress_request_intent(
    command: CatalogProgressChange<'_>,
) -> Result<ClientIntent, E> {
    request::intent(command)
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
    request::encode(scope, request, stamp, command, limits, maximum_owned)
}
/// Validates complete framing/digest/EOF before creating header owners.
/// # Errors
/// Refuses other profiles, malformed bytes and admission/allocation failure.
pub fn prepare_catalog_progress_request(
    encoded: &[u8],
    limits: CodecLimits,
    maximum_owned: usize,
) -> Result<PreparedCatalogProgressRequest<'_>, E> {
    request::prepare(encoded, limits, maximum_owned)
}
/// Borrows an explicit header; complete body audit is still mandatory.
/// # Errors
/// Refuses other profiles, invalid headers and byte limits.
pub fn catalog_progress_recovery_header(
    encoded: &[u8],
    limits: CodecLimits,
) -> Result<(u16, [&[u8]; 3], CommittedTime), E> {
    if encoded.len() > limits.admission().decision_bytes() {
        return Err(E::ByteLimit);
    }
    let header = request::header(&mut super::reader::Reader::new(encoded, limits))?;
    Ok((
        15,
        [header.tenant, header.group, header.recovery],
        header.stamp,
    ))
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
    if intent.version() != 15 || version != 13 {
        return Err(E::UnsupportedVersion);
    }
    if response.len() > admission.decision_bytes() {
        return Err(E::ByteLimit);
    }
    if response != b"FWPR\0\x01" {
        return Err(E::Malformed);
    }
    Ok((RetainedStatus::Accepted, None))
}
