//! Explicit private catalog membership profile; generic decoders stay closed.

use super::{CodecError as E, CodecLimits, PreparedBytes};
use wf_domain::{
    governance::catalog::CatalogMembershipChange,
    workflow::{
        admission::AdmissionLimits,
        request::{ClientIntent, RequestScope, RetainedStatus},
    },
};
use wf_types::{identity::RequestId, version::CommittedTime};

mod model;
mod request;
pub use model::PreparedCatalogMembershipRequest;

/// Derives neutral canonical original command identity, without commit authority.
/// # Errors
/// Refuses invalid original fields and batch ceilings.
pub fn catalog_membership_request_intent(
    command: CatalogMembershipChange<'_>,
) -> Result<ClientIntent, E> {
    request::intent(command)
}

/// Encodes the complete original private command and intent.
/// # Errors
/// Refuses malformed fields, byte/owner limits or allocation failure.
pub fn encode_catalog_membership_request(
    scope: &RequestScope,
    request: &RequestId,
    stamp: CommittedTime,
    command: CatalogMembershipChange<'_>,
    limits: CodecLimits,
    maximum_owned: usize,
) -> Result<PreparedBytes, E> {
    request::encode(scope, request, stamp, command, limits, maximum_owned)
}

/// Completely validates body, digest and EOF before allocating payload owners.
/// # Errors
/// Refuses unsupported frames, invalid content or admission failures.
pub fn prepare_catalog_membership_request(
    encoded: &[u8],
    limits: CodecLimits,
    maximum_owned: usize,
) -> Result<PreparedCatalogMembershipRequest<'_>, E> {
    request::prepare(encoded, limits, maximum_owned)
}

/// Borrows only the explicitly selected header; full body audit remains required.
/// # Errors
/// Refuses malformed headers, other profiles or byte limits.
pub fn catalog_membership_recovery_header(
    encoded: &[u8],
    limits: CodecLimits,
) -> Result<(u16, [&[u8]; 3], CommittedTime), E> {
    if encoded.len() > limits.admission().decision_bytes() {
        return Err(E::ByteLimit);
    }
    let header = request::header(&mut super::reader::Reader::new(encoded, limits))?;
    Ok((
        14,
        [header.tenant, header.group, header.recovery],
        header.stamp,
    ))
}

/// Checks the complete fixed success reply under its exact intent/response pair.
/// # Errors
/// Refuses unsupported profiles, altered bytes or wire limits.
pub fn catalog_membership_retained_summary(
    intent: ClientIntent,
    version: u16,
    response: &[u8],
    admission: AdmissionLimits,
) -> Result<(RetainedStatus, Option<usize>), E> {
    if intent.version() != 14 || version != 12 {
        return Err(E::UnsupportedVersion);
    }
    if response.len() > admission.decision_bytes() {
        return Err(E::ByteLimit);
    }
    if response != b"FWGR\0\x01" {
        return Err(E::Malformed);
    }
    Ok((RetainedStatus::Accepted, None))
}
