//! Explicit private catalog ACK framing; old generic profiles remain closed.

use super::{CodecError, CodecLimits, PreparedBytes};
use wf_domain::governance::catalog::{CreditCatalogAckReply, CreditCatalogAckRequest};
use wf_domain::workflow::{
    admission::AdmissionLimits,
    request::{ClientIntent, RequestScope, RetainedStatus},
};
use wf_types::{identity::RequestId, version::CommittedTime};

mod model;
pub use model::PreparedCatalogAckRequest;

/// Encodes exact original ACK controls and their canonical intent.
/// # Errors
/// Refuses invalid fields, byte/owner limits or allocation failure.
pub fn encode_catalog_ack_request(
    _scope: &RequestScope,
    _request: &RequestId,
    _stamp: CommittedTime,
    _command: CreditCatalogAckRequest,
    _limits: CodecLimits,
    _maximum_owned_bytes: usize,
) -> Result<PreparedBytes, CodecError> {
    Err(CodecError::UnsupportedVersion)
}

/// Fully inspects the frame and digest before creating any payload owners.
/// # Errors
/// Refuses malformed/unsupported frames, limits or allocation failure.
pub fn prepare_catalog_ack_request(
    _encoded: &[u8],
    _limits: CodecLimits,
    _maximum_owned_bytes: usize,
) -> Result<PreparedCatalogAckRequest<'_>, CodecError> {
    Err(CodecError::UnsupportedVersion)
}

/// Borrows only header facts for explicitly selected catalog recovery.
/// Full body/digest/EOF validation remains mandatory before recovery.
/// # Errors
/// Refuses other profiles, bad headers or byte ceilings.
pub fn catalog_ack_recovery_header(
    _encoded: &[u8],
    _limits: CodecLimits,
) -> Result<(u16, [&[u8]; 3], CommittedTime), CodecError> {
    Err(CodecError::UnsupportedVersion)
}

/// Encodes one canonical25-byte private ACK response.
/// # Errors
/// Refuses inconsistent reply fields, limits or allocation failure.
pub fn encode_catalog_ack_reply(
    _reply: CreditCatalogAckReply,
    _maximum_wire_bytes: usize,
    _maximum_owned_bytes: usize,
) -> Result<Vec<u8>, CodecError> {
    Err(CodecError::UnsupportedVersion)
}

/// Decodes one complete fixed-size reply under fresh explicit limits.
/// # Errors
/// Refuses bad framing/EOF/scalars or byte/owner ceilings.
pub fn decode_catalog_ack_reply(
    _encoded: &[u8],
    _maximum_wire_bytes: usize,
    _maximum_owned_bytes: usize,
) -> Result<CreditCatalogAckReply, CodecError> {
    Err(CodecError::UnsupportedVersion)
}

/// Checks exact response bytes against the complete neutral reply, without clones.
/// # Errors
/// Refuses inconsistent fields or any byte mismatch.
pub fn verify_catalog_ack_reply(
    _reply: CreditCatalogAckReply,
    _encoded: &[u8],
) -> Result<(), CodecError> {
    Err(CodecError::UnsupportedVersion)
}

/// Fully inspects the original13/11 response; no workflow item count is inferred.
/// The caller prospectively reserves1024 logical scratch bytes.
/// # Errors
/// Refuses other profiles, malformed replies or actual wire limits.
pub fn catalog_ack_retained_summary(
    _intent: ClientIntent,
    _response_version: u16,
    _response: &[u8],
    _admission: AdmissionLimits,
) -> Result<(RetainedStatus, Option<usize>), CodecError> {
    Err(CodecError::UnsupportedVersion)
}
