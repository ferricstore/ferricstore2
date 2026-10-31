//! Selected bounded release maintenance; ACK is not remote-release evidence.
use super::{CodecError as E, CodecLimits, PreparedBytes};
use wf_domain::{
    governance::release::{ReleaseMaintenanceReply as R, ReleaseMaintenanceRequest as C},
    workflow::{
        admission::AdmissionLimits,
        request::{ClientIntent, RequestScope, RetainedStatus},
    },
};
use wf_types::{identity::RequestId, version::CommittedTime};
mod model;
mod request;
mod response;
pub use model::PreparedReleaseMaintenanceRequest;

/// Canonical complete original command, without current authority or admission.
/// # Errors
/// Refuses private scalar/cohort controls.
pub fn release_maintenance_request_intent(command: &C) -> Result<ClientIntent, E> {
    request::intent(command)
}
/// Encodes one original bounded maintenance command.
/// # Errors
/// Refuses scalar/cohort fields, byte/owner bounds or allocation failure.
pub fn encode_release_maintenance_request(
    scope: &RequestScope,
    id: &RequestId,
    stamp: CommittedTime,
    command: &C,
    limits: CodecLimits,
    maximum_owned: usize,
) -> Result<PreparedBytes, E> {
    request::encode(scope, id, stamp, command, limits, maximum_owned)
}
/// Fully scans original bytes/digest/EOF before allocating owned fields.
/// # Errors
/// Refuses framing, original controls, digest mismatch or resource admission.
pub fn prepare_release_maintenance_request(
    encoded: &[u8],
    limits: CodecLimits,
    maximum_owned: usize,
) -> Result<PreparedReleaseMaintenanceRequest<'_>, E> {
    request::prepare(encoded, limits, maximum_owned)
}

/// Compares the complete original frame and returns borrowed identity facts.
/// No header, sequence spine or encoded payload owner is copied.
/// # Errors
/// Refuses any scope/stamp/command/EOF mismatch or byte/profile violation.
pub fn verify_release_maintenance_request<'a>(
    encoded: &'a [u8],
    scope: &RequestScope,
    stamp: CommittedTime,
    command: &C,
    limits: CodecLimits,
) -> Result<
    (
        &'a [u8],
        ClientIntent,
        wf_domain::workflow::control::PreparedDigest,
    ),
    E,
> {
    request::verify(encoded, scope, stamp, command, limits)
}
/// Borrows only explicitly selected recovery header facts; full preparation is mandatory.
/// # Errors
/// Refuses other native profiles, invalid headers or byte limits.
pub fn release_maintenance_recovery_header(
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
/// Encodes the exact original25-byte neutral reply.
/// # Errors
/// Refuses invalid neutral data, limits or allocation failure.
pub fn encode_release_maintenance_reply(
    reply: R,
    maximum: usize,
    maximum_owned: usize,
) -> Result<Vec<u8>, E> {
    response::encode(reply, maximum, maximum_owned)
}
/// Scans one complete original reply, without queue freshness or release authority.
/// # Errors
/// Refuses malformed framing/meta/counts, EOF or resource limits.
pub fn decode_release_maintenance_reply(
    encoded: &[u8],
    maximum: usize,
    maximum_owned: usize,
) -> Result<R, E> {
    response::decode(encoded, maximum, maximum_owned)
}
/// Compares the complete neutral result with original response bytes, without copies.
/// # Errors
/// Refuses invalid neutral data or any complete byte mismatch.
pub fn verify_release_maintenance_reply(reply: R, encoded: &[u8]) -> Result<(), E> {
    if response::bytes(reply)?.as_slice() != encoded {
        return Err(E::Malformed);
    }
    Ok(())
}
/// Explicit18/15 receipt grammar; no workflow item count is inferred.
/// The caller reserves1024 logical scratch bytes before scanning.
/// # Errors
/// Refuses other profiles, malformed responses or configured byte ceilings.
pub fn release_maintenance_retained_summary(
    intent: ClientIntent,
    version: u16,
    encoded: &[u8],
    admission: AdmissionLimits,
) -> Result<(RetainedStatus, Option<usize>), E> {
    if intent.version() != 18 || version != 15 {
        return Err(E::UnsupportedVersion);
    }
    let status = match response::decode(encoded, admission.decision_bytes(), 1024)? {
        R::Accepted { .. } => RetainedStatus::Accepted,
        R::Rejected { reason, .. } => RetainedStatus::WorkflowReleaseRejected(reason),
    };
    Ok((status, None))
}
