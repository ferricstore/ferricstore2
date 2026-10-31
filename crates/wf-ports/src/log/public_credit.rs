//! Public credit caller identity and explicit trusted preparation binding.
use super::{CodecError as E, CodecLimits, PreparedBytes, PreparedCreditRequest};
use wf_domain::{
    governance::caller::{CreditPolicyInput, NormalizedCreditCall},
    workflow::{
        control::PreparedDigest,
        request::{ClientIntent, RequestScope},
    },
};
use wf_types::{identity::RequestId, version::CommittedTime};
mod decode;
mod encode;
mod ids;
mod policy;
/// Original response operands; generated IDs may be unknown during retained lookup.
#[derive(Clone, Copy)]
pub struct PublicCreditReplyExpectation<'a> {
    /// Exact original credit owner identity.
    pub scope: &'a [u8],
    /// Original public operation.
    pub kind: wf_domain::governance::caller::CreditCallKind,
    /// Original selected shard, when applicable.
    pub shard: Option<u64>,
    /// Original requested spend count, when applicable.
    pub amount: Option<u64>,
    /// Exact generated IDs for submitted/retried spends, in original order.
    pub ids: Option<wf_domain::governance::caller::CreditIdList<'a>>,
}
/// Inspects complete original bytes without restoring or cloning an owner.
/// Caller must admit18432 bytes of inspection scratch in its enclosing budget.
/// # Errors
/// Refuses other profiles, malformed bytes or original operand mismatches.
pub fn inspect_public_credit_reply(
    intent: ClientIntent,
    version: u16,
    response: &[u8],
    expected: PublicCreditReplyExpectation<'_>,
    credit: wf_domain::governance::credit::CreditLimits,
    admission: wf_domain::workflow::admission::AdmissionLimits,
) -> Result<wf_domain::workflow::request::RetainedStatus, E> {
    let _ = (intent, version, response, expected, credit, admission);
    Err(E::UnsupportedTag)
}
/// Explicit spend generation and entropy captured by the authorized preparer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CreditSpendPreparation {
    /// Original observed source lease generation.
    pub lease_epoch: u64,
    /// One trusted random batch, reused for every ID in this attempt.
    pub batch: [u8; 16],
}
/// Hashes canonical caller fields, excluding every server-generated operand.
/// # Errors
/// Refuses invalid typed captures or bounded encoding failures.
pub fn public_credit_intent(call: NormalizedCreditCall<'_>) -> Result<ClientIntent, E> {
    encode::intent(call)
}
/// Fingerprints source external-term integer/binary policy data without a payload copy.
/// # Errors
/// Refuses invalid finite versions or binary size bounds.
pub fn fingerprint_credit_policy(value: CreditPolicyInput<'_>) -> Result<PreparedDigest, E> {
    policy::fingerprint(value)
}
/// Generates the original ordered source IDs from explicit trusted preparation.
/// # Errors
/// Refuses epoch/amount, prospective owner budget or allocation failures.
pub fn generate_public_credit_ids(
    amount: u64,
    spend: CreditSpendPreparation,
    maximum_owned: usize,
) -> Result<Vec<Vec<u8>>, E> {
    ids::generate(amount, spend, maximum_owned)
}
/// Encodes the complete public caller and its trusted prepared operands.
/// # Errors
/// Refuses malformed preparation, framing or prospective owner limits.
pub fn encode_public_credit_request(
    scope: &RequestScope,
    id: &RequestId,
    call: NormalizedCreditCall<'_>,
    stamp: CommittedTime,
    spend: Option<CreditSpendPreparation>,
    limits: CodecLimits,
    maximum_owned: usize,
) -> Result<PreparedBytes, E> {
    encode::request(scope, id, call, stamp, spend, limits, maximum_owned)
}
/// Completely verifies public framing before materializing private command operands.
/// # Errors
/// Refuses private/unknown formats, malformed callers, tampering and owner limits.
pub fn prepare_public_credit_request(
    encoded: &[u8],
    limits: CodecLimits,
    maximum_owned: usize,
) -> Result<PreparedCreditRequest<'_>, E> {
    decode::prepare(encoded, limits, maximum_owned)
}
/// Borrows explicitly selected public header facts; complete audit remains mandatory.
/// # Errors
/// Refuses other formats, malformed headers and bounded wire limits.
pub fn public_credit_recovery_header(
    encoded: &[u8],
    limits: CodecLimits,
) -> Result<(u16, [&[u8]; 3], CommittedTime), E> {
    decode::recovery_header(encoded, limits)
}
