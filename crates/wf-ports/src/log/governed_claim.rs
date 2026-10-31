//! Private paid Claim framing; actual eligibility belongs to the consumed writer.
use super::{CodecError as E, CodecLimits, DecodedRequest, PreparedBytes};
use wf_domain::{governance::release::LimitEnforcement, workflow::control::PreparedDigest};
mod payment;
mod request;
pub(super) mod response;
pub use super::claim::apply::{inspect_governed_claim_point, reject_unavailable_governed_claim};
pub use request::{encode_governed_claim_request, prepare_governed_claim_request};
pub use response::{
    GovernedClaimReplyRef, governed_claim_response_matches, governed_claim_retained_summary,
    inspect_governed_claim_reply,
};

/// Original spent ID owners; encoding neither spends nor authorizes them.
#[derive(Clone, Copy)]
pub struct GovernedClaimPayment<'a> {
    /// Opaque original credit scope.
    pub scope: &'a [u8],
    /// Actual paid shard.
    pub shard_id: u64,
    /// Original positive paid generation.
    pub epoch: u64,
    /// Captured original enforcement.
    pub enforcement: LimitEnforcement,
    /// Complete original spend order, including unused suffix IDs.
    pub reservation_ids: &'a [Vec<u8>],
}
/// Completely scanned borrowed payment; it conveys no current credit authority.
#[derive(Clone, Copy)]
pub struct GovernedClaimPaymentRef<'a> {
    body: &'a [u8],
    scope: &'a [u8],
    shard: u64,
    epoch: u64,
    enforcement: LimitEnforcement,
    count: usize,
    ids: &'a [u8],
}
impl<'a> GovernedClaimPaymentRef<'a> {
    /// Original opaque scope.
    #[must_use]
    pub const fn scope(self) -> &'a [u8] {
        self.scope
    }
    /// Original shard.
    #[must_use]
    pub const fn shard_id(self) -> u64 {
        self.shard
    }
    /// Original paid generation.
    #[must_use]
    pub const fn epoch(self) -> u64 {
        self.epoch
    }
    /// Original captured enforcement.
    #[must_use]
    pub const fn enforcement(self) -> LimitEnforcement {
        self.enforcement
    }
    /// Complete original ID count, including unused suffix IDs.
    #[must_use]
    pub const fn count(self) -> usize {
        self.count
    }
    /// Borrows IDs in the original order without copying payloads or identifiers.
    pub fn reservation_ids(self) -> impl Iterator<Item = Result<&'a [u8], E>> {
        let mut bytes = self.ids;
        (0..self.count).map(move |_| {
            let length = u32::from_be_bytes(
                bytes
                    .get(..4)
                    .ok_or(E::Malformed)?
                    .try_into()
                    .map_err(|_| E::Malformed)?,
            );
            let length = usize::try_from(length).map_err(|_| E::LengthOverflow)?;
            let id = bytes.get(4..4 + length).ok_or(E::Malformed)?;
            bytes = bytes.get(4 + length..).ok_or(E::Malformed)?;
            Ok(id)
        })
    }
    /// Exact original payment correspondence, including all ID order and presence.
    #[must_use]
    pub fn matches(self, other: Self) -> bool {
        self.body == other.body
    }
}
/// Concrete payment captured from a completely verified committed frame.
#[derive(Clone, Copy)]
pub struct CapturedGovernedClaimPayment<'a> {
    payment: GovernedClaimPaymentRef<'a>,
    prepared: PreparedDigest,
    requested_limit: usize,
}
impl<'a> CapturedGovernedClaimPayment<'a> {
    /// Borrows original payment data, without fresh eligibility authority.
    #[must_use]
    pub const fn payment(self) -> GovernedClaimPaymentRef<'a> {
        self.payment
    }
    /// Digest of the complete actual frame, including original paid IDs.
    #[must_use]
    pub const fn prepared(self) -> PreparedDigest {
        self.prepared
    }
    /// Exact query limit captured from the same complete frame.
    #[must_use]
    pub const fn requested_limit(self) -> usize {
        self.requested_limit
    }
}
/// Existing Claim query owner plus its exact borrowed original paid capture.
pub struct PreparedGovernedClaim<'a> {
    decoded: DecodedRequest<'a>,
    capture: CapturedGovernedClaimPayment<'a>,
}
impl<'a> PreparedGovernedClaim<'a> {
    pub(super) fn into_parts(self) -> (DecodedRequest<'a>, CapturedGovernedClaimPayment<'a>) {
        (self.decoded, self.capture)
    }
    /// Original decoded header/query; no synthetic trusted-parent projection.
    #[must_use]
    pub const fn decoded(&self) -> &DecodedRequest<'a> {
        &self.decoded
    }
    /// Original fully verified capture, before any current payment observation.
    #[must_use]
    pub const fn capture(&self) -> CapturedGovernedClaimPayment<'a> {
        self.capture
    }
}
