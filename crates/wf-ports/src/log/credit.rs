//! Explicit credit protocol contracts; rejecting until complete implementation.

use super::{CodecError, CodecLimits, PreparedBytes};
use wf_domain::governance::credit::{CreditLimits, CreditOwner};
use wf_domain::governance::identified::{IdentifiedCreditReply, IdentifiedCreditRequest};
use wf_domain::workflow::request::RequestScope;
use wf_types::identity::RequestId;
use wf_types::version::CommittedTime;

mod admission;
mod encode;
mod owner;
mod response;
mod status;

/// Encodes the exact private identified request, preserving original ID order.
/// # Errors
/// Refuses unsupported/input/resource/allocation failures before exposing bytes.
pub fn encode_credit_request(
    scope: &RequestScope,
    request: &RequestId,
    stamp: CommittedTime,
    command: IdentifiedCreditRequest<'_>,
    limits: CodecLimits,
    maximum_owned_bytes: usize,
) -> Result<PreparedBytes, CodecError> {
    encode::request(scope, request, stamp, command, limits, maximum_owned_bytes)
}

/// Restores a complete original canonical private response under explicit caps.
/// # Errors
/// Refuses unsupported/corrupt/profile/allocation failures.
pub fn decode_credit_reply(
    encoded: &[u8],
    credit: CreditLimits,
    maximum_wire_bytes: usize,
    maximum_owned_bytes: usize,
) -> Result<IdentifiedCreditReply, CodecError> {
    response::inspect(encoded, credit, maximum_wire_bytes, maximum_owned_bytes)?
        .into_owned(maximum_owned_bytes)
}

/// Encodes one canonical detached mathematical owner, without storage authority.
/// # Errors
/// Refuses embedded IDs, field/metadata limits or allocation failure.
pub fn encode_credit_owner(
    owner: &CreditOwner,
    maximum_wire_bytes: usize,
    maximum_owned_bytes: usize,
) -> Result<Vec<u8>, CodecError> {
    owner::encode(owner, maximum_wire_bytes, maximum_owned_bytes)
}

/// Restores a complete detached mathematical owner after borrowed inspection.
/// # Errors
/// Refuses framing, canonical invariants, profiles or allocation failure.
pub fn decode_credit_owner(
    encoded: &[u8],
    credit: CreditLimits,
    maximum_wire_bytes: usize,
    maximum_owned_bytes: usize,
) -> Result<CreditOwner, CodecError> {
    owner::inspect(encoded, credit, maximum_wire_bytes, maximum_owned_bytes)?
        .into_owned(maximum_owned_bytes)
}

/// Encodes the exact original private decision and represented response owners.
/// # Errors
/// Refuses inconsistent shape, count/byte ceilings or allocation failure.
pub fn encode_credit_reply(
    reply: &IdentifiedCreditReply,
    maximum_wire_bytes: usize,
    maximum_owned_bytes: usize,
) -> Result<Vec<u8>, CodecError> {
    response::encode(reply, maximum_wire_bytes, maximum_owned_bytes)
}
