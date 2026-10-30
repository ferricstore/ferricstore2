//! Explicit credit protocol contracts; rejecting until complete implementation.

use super::{CodecError, CodecLimits, PreparedBytes};
use wf_domain::governance::credit::CreditLimits;
use wf_domain::governance::identified::{IdentifiedCreditReply, IdentifiedCreditRequest};
use wf_domain::workflow::request::RequestScope;
use wf_types::identity::RequestId;
use wf_types::version::CommittedTime;

/// Encodes the exact private identified request, preserving original ID order.
/// # Errors
/// Refuses unsupported/input/resource/allocation failures before exposing bytes.
pub fn encode_credit_request(
    _scope: &RequestScope,
    _request: &RequestId,
    _stamp: CommittedTime,
    _command: IdentifiedCreditRequest<'_>,
    _limits: CodecLimits,
    _maximum_owned_bytes: usize,
) -> Result<PreparedBytes, CodecError> {
    Err(CodecError::UnsupportedVersion)
}

/// Restores a complete original canonical private response under explicit caps.
/// # Errors
/// Refuses unsupported/corrupt/profile/allocation failures.
pub fn decode_credit_reply(
    _encoded: &[u8],
    _credit: CreditLimits,
    _maximum_wire_bytes: usize,
    _maximum_owned_bytes: usize,
) -> Result<IdentifiedCreditReply, CodecError> {
    Err(CodecError::UnsupportedVersion)
}
