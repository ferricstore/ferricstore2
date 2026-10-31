//! Canonical private identified-credit requests, replies and public projections.

use super::{CodecError, CodecLimits, PreparedBytes};
use wf_domain::governance::credit::{CreditLimits, CreditOwner};
use wf_domain::governance::identified::{IdentifiedCreditReply, IdentifiedCreditRequest};
use wf_domain::workflow::admission::AdmissionLimits;
use wf_domain::workflow::request::{ClientIntent, RequestScope, RetainedStatus};
use wf_types::identity::RequestId;
use wf_types::version::CommittedTime;

mod admission;
mod decode;
mod encode;
mod model;
mod scan;
pub use model::PreparedCreditRequest;
mod owner;
mod response;
mod status;
mod view;

/// Canonical private Cleanup identity without a temporary body allocation.
/// # Errors
/// Refuses malformed scope or shard-count bounds.
pub fn credit_cleanup_intent(scope: &[u8], shard_count: u64) -> Result<ClientIntent, CodecError> {
    use sha2::{Digest, Sha256};
    if scope.is_empty()
        || scope.len() > 65535
        || shard_count == 0
        || shard_count > wf_domain::workflow::MAX_EXACT_MILLIS
    {
        return Err(CodecError::Malformed);
    }
    let mut hash = Sha256::new();
    hash.update(b"wf-governing-credit-intent-v1\0");
    hash.update(
        u32::try_from(scope.len())
            .map_err(|_| CodecError::LengthOverflow)?
            .to_be_bytes(),
    );
    hash.update(scope);
    hash.update(shard_count.to_be_bytes());
    hash.update([0, 6]);
    Ok(ClientIntent::governance_credit(hash.finalize().into()))
}

/// Completely inspects an accepted Cleanup reply, borrowing its original owner.
/// Callers prospectively admit18432 bytes of inspection scratch.
/// # Errors
/// Refuses any other profile/decision, corruption, EOF or admission failure.
pub fn credit_cleanup_retained_summary(
    intent: ClientIntent,
    version: u16,
    response: &[u8],
    expected_scope: &[u8],
    credit: CreditLimits,
    admission: AdmissionLimits,
) -> Result<(u16, bool), CodecError> {
    let _ = expected_scope;
    use wf_domain::governance::identified::IdentifiedCreditDecision as D;
    if intent.version() != 12 || version != 10 {
        return Err(CodecError::UnsupportedVersion);
    }
    let (decision, count) = response::inspect(
        response,
        credit,
        admission.decision_bytes(),
        credit.max_owned_bytes(),
    )?
    .summary();
    match decision {
        D::Cleaned { deleted, pending } if count == 0 => Ok((deleted, pending)),
        _ => Err(CodecError::UnsupportedTag),
    }
}

/// Restores one complete original reply and visits only its public fields.
/// The visitor's iteration, copies and allocation are external work. Borrowed
/// reply data cannot escape the callback; no current owner read is performed.
///
/// ```compile_fail
/// use wf_domain::governance::{credit::CreditLimits, view::CreditReplyView};
/// use wf_ports::log::with_public_credit_reply;
/// let escaped = with_public_credit_reply(&[], CreditLimits::default(), 1024, 8192, |view| {
///     match view {
///         CreditReplyView::Owner(owner) => Some(owner.scope),
///         _ => None,
///     }
/// });
/// ```
/// # Errors
/// Refuses malformed, unsupported or limited replies before calling the visitor.
pub fn with_public_credit_reply<R>(
    encoded: &[u8],
    credit: CreditLimits,
    maximum_wire_bytes: usize,
    maximum_owned_bytes: usize,
    visit: impl for<'reply> FnOnce(wf_domain::governance::view::CreditReplyView<'reply>) -> R,
) -> Result<R, CodecError> {
    view::visit(
        encoded,
        credit,
        maximum_wire_bytes,
        maximum_owned_bytes,
        visit,
    )
}

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

/// Verifies a complete original identified frame before creating payload owners.
/// # Errors
/// Refuses unknown framing, tampering, noncanonical inputs, limits or allocation.
pub fn prepare_credit_request(
    encoded: &[u8],
    limits: CodecLimits,
    maximum_owned_bytes: usize,
) -> Result<PreparedCreditRequest<'_>, CodecError> {
    decode::prepare(encoded, limits, maximum_owned_bytes)
}

/// Borrows only explicit private credit header facts for opted-in recovery.
/// Full body, digest, field, scope and EOF validation remains mandatory.
/// # Errors
/// Refuses other profiles, malformed headers or the captured wire ceiling.
pub fn credit_recovery_header(
    encoded: &[u8],
    limits: CodecLimits,
) -> Result<(u16, [&[u8]; 3], CommittedTime), CodecError> {
    if encoded.len() > limits.admission().decision_bytes() {
        return Err(CodecError::ByteLimit);
    }
    let mut reader = crate::log::reader::Reader::new(encoded, limits);
    let header = scan::header(&mut reader)?;
    Ok((
        12,
        [header.tenant, header.group, header.recovery],
        header.stamp,
    ))
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

/// Encodes a sealed plan's borrowed response without copying owner or ID payloads.
/// # Errors
/// Refuses inconsistent decisions, field limits or allocation failure.
pub fn encode_credit_reply_parts(
    decision: wf_domain::governance::identified::IdentifiedCreditDecision,
    owner: Option<&CreditOwner>,
    ids: &[Vec<u8>],
    maximum_wire_bytes: usize,
    maximum_owned_bytes: usize,
) -> Result<Vec<u8>, CodecError> {
    response::encode_parts(
        decision,
        owner,
        ids,
        maximum_wire_bytes,
        maximum_owned_bytes,
    )
}

/// Fully inspects an original private response without materializing its owner.
/// # Errors
/// Refuses inconsistent plans, bytes or inspection scratch limits.
pub fn verify_credit_reply_parts(
    decision: wf_domain::governance::identified::IdentifiedCreditDecision,
    owner: Option<&CreditOwner>,
    ids: &[Vec<u8>],
    encoded: &[u8],
    maximum_owned_bytes: usize,
) -> Result<(), CodecError> {
    response::verify_parts(decision, owner, ids, encoded, maximum_owned_bytes)
}

/// Fully inspects an original private response without materializing its owner.
/// Callers prospectively admit 18432 bytes of bounded inspection scratch.
/// # Errors
/// Rejects other profiles, malformed responses and actual admission ceilings.
pub fn credit_retained_summary(
    intent: ClientIntent,
    response_version: u16,
    response: &[u8],
    admission: AdmissionLimits,
) -> Result<(RetainedStatus, Option<usize>), CodecError> {
    use wf_domain::governance::identified::IdentifiedCreditDecision as D;
    if intent.version() != 12 || response_version != 10 {
        return Err(CodecError::UnsupportedVersion);
    }
    let (decision, count) = response::inspect(
        response,
        CreditLimits::default(),
        admission.decision_bytes(),
        usize::MAX,
    )?
    .summary();
    if count > admission.batch_items() {
        return Err(CodecError::ItemLimit);
    }
    Ok((
        match decision {
            D::Rejected(reason) => RetainedStatus::GovernanceRejected(reason),
            _ => RetainedStatus::Accepted,
        },
        matches!(decision, D::Spent { .. }).then_some(count),
    ))
}
