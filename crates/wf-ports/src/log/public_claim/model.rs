//! Actual server preparation, with no paid-credit or execution authority.
use crate::log::PreparedAtomicClaim;

/// Server-owned original caller binding and immutable credit capability.
#[derive(Clone, Copy)]
pub struct PublicClaimReplyLimits {
    /// Captured response byte/count/inline limits.
    pub admission: wf_domain::workflow::admission::AdmissionLimits,
    /// Original schedule/whole-caller work profile.
    pub coordination: wf_domain::claim::coordination::ClaimCoordinationLimits,
    /// Actual configured immutable shard count, rechecked by the adapter.
    pub credit_shard_count: u64,
}

/// Generated committed inputs, independent of the original public V1 caller.
#[derive(Clone, Copy)]
pub struct PublicClaimPreparation {
    /// Source token algorithm version.
    pub token_version: u16,
    /// First available state-entry ordinal.
    pub first_ordinal: u32,
    /// Whole-parent candidate scan budget.
    pub scan_budget: usize,
    /// Whole-parent lane-head budget.
    pub head_budget: usize,
    /// One original trusted16-byte funding batch; retries retain these bytes.
    pub batch: [u8; 16],
}

/// Fully parsed actual op19 original parent, never a synthetic prepaid frame.
pub struct PreparedPublicClaim<'a> {
    pub(super) inner: PreparedAtomicClaim<'a>,
}
impl<'a> PreparedPublicClaim<'a> {
    /// Borrows the actual root and its original fully verified caller/header.
    #[must_use]
    pub const fn prepared(&self) -> &PreparedAtomicClaim<'a> {
        &self.inner
    }
}
