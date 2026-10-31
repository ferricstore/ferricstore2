//! Original immutable parent capture; this data alone authorizes no phase effects.

use super::super::GovernedClaimPaymentRef;
use super::{CodecError, caller::AtomicClaimCaller};
use wf_domain::{
    claim::caller::{ClaimCallerLimits, NormalizedClaimDue},
    workflow::{
        control::{EntryIdentity, PreparedDigest},
        request::gate::CommittedHeader,
        request::{ClientIntent, RequestScope},
    },
};
use wf_types::{
    identity::RequestId,
    version::{CommittedTime, LogIdentity},
};

/// Complete original parent frame, admitted before any owned caller spines.
pub struct PreparedAtomicClaim<'a> {
    pub(super) encoded: &'a [u8],
    pub(super) scope: RequestScope,
    pub(super) request: RequestId,
    pub(super) intent: ClientIntent,
    pub(super) stamp: CommittedTime,
    pub(super) digest: PreparedDigest,
    pub(super) owned_bytes: usize,
    pub(super) isolation: Vec<u8>,
    pub(super) caller: AtomicClaimCaller<'a>,
    pub(super) caller_bytes: &'a [u8],
    pub(super) token_version: u16,
    pub(super) first_ordinal: u32,
    pub(super) scan_budget: usize,
    pub(super) head_budget: usize,
    pub(super) payment: Option<GovernedClaimPaymentRef<'a>>,
    pub(super) funding_batch: Option<[u8; 16]>,
}
impl<'a> PreparedAtomicClaim<'a> {
    /// Original actual committed bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &'a [u8] {
        self.encoded
    }
    /// Exact decoded original caller, re-admitted under the supplied caller profile.
    /// # Errors
    /// Refuses narrower preparation limits or fallible reference ownership.
    pub fn caller(&self, limits: ClaimCallerLimits) -> Result<NormalizedClaimDue<'_>, CodecError> {
        self.caller.normalize(limits)
    }
    /// Original server-bound scope.
    #[must_use]
    pub const fn scope(&self) -> &RequestScope {
        &self.scope
    }
    /// Original request identity.
    #[must_use]
    pub const fn request(&self) -> &RequestId {
        &self.request
    }
    /// Canonical original-caller identity.
    #[must_use]
    pub const fn intent(&self) -> ClientIntent {
        self.intent
    }
    /// Whole actual prepared frame digest, including generated inputs.
    #[must_use]
    pub const fn digest(&self) -> PreparedDigest {
        self.digest
    }
    /// Conservative complete preparation/caller-normalization owner bound.
    #[must_use]
    pub const fn owned_bytes(&self) -> usize {
        self.owned_bytes
    }
    /// Actual original payment; no live eligibility is implied.
    #[must_use]
    pub const fn payment(&self) -> Option<GovernedClaimPaymentRef<'a>> {
        self.payment
    }
    /// Original public funding nonce, without paid-credit or execution authority.
    #[must_use]
    pub const fn funding_batch(&self) -> Option<[u8; 16]> {
        self.funding_batch
    }
    /// Committed token algorithm and first ordinal.
    #[must_use]
    pub const fn token(&self) -> (u16, u32) {
        (self.token_version, self.first_ordinal)
    }
    /// Whole-parent scan and head allowances.
    #[must_use]
    pub const fn budgets(&self) -> (usize, usize) {
        (self.scan_budget, self.head_budget)
    }
    /// Actual original header bound to a separately assigned committed log identity.
    #[must_use]
    pub fn header(&self, log: LogIdentity) -> CommittedHeader<'_> {
        CommittedHeader {
            format_version: 1,
            scope: &self.scope,
            request: &self.request,
            intent: self.intent,
            identity: EntryIdentity::new(log, self.digest),
            stamp: self.stamp,
            encoded_entry: self.encoded,
            encoded_isolation_scope: &self.isolation,
        }
    }
    /// Actual canonical state key under this verified parent's isolation scope.
    /// # Errors
    /// Refuses key representation or configured hard-size overflow.
    pub fn state_key(
        &self,
        partition: &wf_types::identity::PartitionId,
        id: &wf_types::identity::RecordId,
    ) -> Result<Vec<u8>, CodecError> {
        super::super::keys::state(&self.isolation, partition.as_bytes(), id.as_bytes())
    }
    /// Encoded namespace length for prospective canonical-key accounting.
    #[must_use]
    pub fn isolation_bytes(&self) -> usize {
        self.isolation.len()
    }
}
