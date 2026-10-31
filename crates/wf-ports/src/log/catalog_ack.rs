//! Explicit private catalog ACK framing; old generic profiles remain closed.

use super::{CodecError, CodecLimits, PreparedBytes};
use wf_domain::governance::catalog::{CreditCatalogAckReply, CreditCatalogAckRequest};
use wf_domain::workflow::{
    admission::AdmissionLimits,
    request::{ClientIntent, RequestScope, RetainedStatus},
};
use wf_types::{identity::RequestId, version::CommittedTime};

mod model;
mod request;
mod response;
pub use model::PreparedCatalogAckRequest;

/// Encodes exact original ACK controls and their canonical intent.
/// # Errors
/// Refuses invalid fields, byte/owner limits or allocation failure.
pub fn encode_catalog_ack_request(
    scope: &RequestScope,
    request: &RequestId,
    stamp: CommittedTime,
    command: CreditCatalogAckRequest,
    limits: CodecLimits,
    maximum_owned_bytes: usize,
) -> Result<PreparedBytes, CodecError> {
    request::encode(scope, request, stamp, command, limits, maximum_owned_bytes)
}

/// Fully inspects the frame and digest before creating any payload owners.
/// # Errors
/// Refuses malformed/unsupported frames, limits or allocation failure.
pub fn prepare_catalog_ack_request(
    encoded: &[u8],
    limits: CodecLimits,
    maximum_owned_bytes: usize,
) -> Result<PreparedCatalogAckRequest<'_>, CodecError> {
    request::prepare(encoded, limits, maximum_owned_bytes)
}

/// Borrows only header facts for explicitly selected catalog recovery.
/// Full body/digest/EOF validation remains mandatory before recovery.
/// # Errors
/// Refuses other profiles, bad headers or byte ceilings.
pub fn catalog_ack_recovery_header(
    encoded: &[u8],
    limits: CodecLimits,
) -> Result<(u16, [&[u8]; 3], CommittedTime), CodecError> {
    if encoded.len() > limits.admission().decision_bytes() {
        return Err(CodecError::ByteLimit);
    }
    let header = request::header(&mut super::reader::Reader::new(encoded, limits))?;
    Ok((13, [header.tenant,header.group,header.recovery], header.stamp))
}

/// Encodes one canonical25-byte private ACK response.
/// # Errors
/// Refuses inconsistent reply fields, limits or allocation failure.
pub fn encode_catalog_ack_reply(
    reply: CreditCatalogAckReply,
    maximum_wire_bytes: usize,
    maximum_owned_bytes: usize,
) -> Result<Vec<u8>, CodecError> {
    response::encode(reply, maximum_wire_bytes, maximum_owned_bytes)
}

/// Decodes one complete fixed-size reply under fresh explicit limits.
/// # Errors
/// Refuses bad framing/EOF/scalars or byte/owner ceilings.
pub fn decode_catalog_ack_reply(
    encoded: &[u8],
    maximum_wire_bytes: usize,
    maximum_owned_bytes: usize,
) -> Result<CreditCatalogAckReply, CodecError> {
    response::decode(encoded, maximum_wire_bytes, maximum_owned_bytes)
}

/// Checks exact response bytes against the complete neutral reply, without clones.
/// # Errors
/// Refuses inconsistent fields or any byte mismatch.
pub fn verify_catalog_ack_reply(
    reply: CreditCatalogAckReply,
    encoded: &[u8],
) -> Result<(), CodecError> {
    if response::bytes(reply)? != encoded {
        return Err(CodecError::Malformed);
    }
    Ok(())
}

/// Fully inspects the original13/11 response; no workflow item count is inferred.
/// The caller prospectively reserves1024 logical scratch bytes.
/// # Errors
/// Refuses other profiles, malformed replies or actual wire limits.
pub fn catalog_ack_retained_summary(
    intent: ClientIntent,
    response_version: u16,
    response: &[u8],
    admission: AdmissionLimits,
) -> Result<(RetainedStatus, Option<usize>), CodecError> {
    if intent.version() != 13 || response_version != 11 {
        return Err(CodecError::UnsupportedVersion);
    }
    let status = match response::decode(response, admission.decision_bytes(), 1024)? {
        CreditCatalogAckReply::Accepted { .. } => RetainedStatus::Accepted,
        CreditCatalogAckReply::Rejected { reason, .. } => {
            RetainedStatus::CreditCatalogRejected(reason)
        }
    };
    Ok((status, None))
}
