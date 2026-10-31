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
pub use model::PreparedCatalogMembershipRequest;

/// Derives neutral canonical original command identity, without commit authority.
/// # Errors
/// Refuses invalid original fields and batch ceilings.
pub fn catalog_membership_request_intent(
    _command: CatalogMembershipChange<'_>,
) -> Result<ClientIntent, E> {
    Err(E::UnsupportedVersion)
}

/// Encodes the complete original private command and intent.
/// # Errors
/// Refuses malformed fields, byte/owner limits or allocation failure.
pub fn encode_catalog_membership_request(
    _scope: &RequestScope,
    _request: &RequestId,
    _stamp: CommittedTime,
    _command: CatalogMembershipChange<'_>,
    _limits: CodecLimits,
    _maximum_owned: usize,
) -> Result<PreparedBytes, E> {
    Err(E::UnsupportedVersion)
}

/// Completely validates body, digest and EOF before allocating payload owners.
/// # Errors
/// Refuses unsupported frames, invalid content or admission failures.
pub fn prepare_catalog_membership_request(
    _encoded: &[u8],
    _limits: CodecLimits,
    _maximum_owned: usize,
) -> Result<PreparedCatalogMembershipRequest<'_>, E> {
    Err(E::UnsupportedVersion)
}

/// Borrows only the explicitly selected header; full body audit remains required.
/// # Errors
/// Refuses malformed headers, other profiles or byte limits.
pub fn catalog_membership_recovery_header(
    _encoded: &[u8],
    _limits: CodecLimits,
) -> Result<(u16, [&[u8]; 3], CommittedTime), E> {
    Err(E::UnsupportedVersion)
}

/// Checks the complete fixed success reply under its exact intent/response pair.
/// # Errors
/// Refuses unsupported profiles, altered bytes or wire limits.
pub fn catalog_membership_retained_summary(
    _intent: ClientIntent,
    _version: u16,
    _response: &[u8],
    _admission: AdmissionLimits,
) -> Result<(RetainedStatus, Option<usize>), E> {
    Err(E::UnsupportedVersion)
}
