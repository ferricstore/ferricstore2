//! Borrowed public credit data; views establish no durable or current authority.

use super::credit::{CreditError, CreditLeaseData};
use super::identified::{IdentifiedCreditRejection, IdentifiedCreditReply};
use std::fmt::{self, Write};

/// Unpadded base64url representation of one captured 32-byte fingerprint.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CreditPolicyVersionHash([u8; 43]);

impl CreditPolicyVersionHash {
    /// Borrows the exact43 ASCII bytes, without allocating a string.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 43] {
        &self.0
    }
}

impl fmt::Display for CreditPolicyVersionHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in self.0 {
            f.write_char(char::from(byte))?;
        }
        Ok(())
    }
}

/// Only the eight source public lease fields; detached metadata stays private.
///
/// ```compile_fail
/// fn hidden(lease: wf_domain::governance::view::CreditLeaseView) {
///     let _ = lease.reservations;
/// }
/// ```
/// ```compile_fail
/// fn hidden(lease: wf_domain::governance::view::CreditLeaseView) {
///     let _ = lease.reservation_page;
/// }
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CreditLeaseView {
    /// Original shard identity.
    pub shard_id: u64,
    /// Original lease generation.
    pub epoch: u64,
    /// Original lease deadline.
    pub expires_at_ms: u64,
    /// Original unspent capacity.
    pub available: u64,
    /// Original active capacity.
    pub in_use: u64,
    /// Original requested idle capacity.
    pub pending_reclaim: u64,
    /// Original finite nonnegative binary64 drain rate.
    pub drain_rate: f64,
    /// Original last successful spend time.
    pub last_spend_at_ms: Option<u64>,
}

impl CreditLeaseView {
    fn from_lease(lease: &CreditLeaseData) -> Self {
        Self {
            shard_id: lease.shard_id,
            epoch: lease.epoch,
            expires_at_ms: lease.expires_at_ms,
            available: lease.available,
            in_use: lease.in_use,
            pending_reclaim: lease.pending_reclaim,
            drain_rate: f64::from_bits(lease.drain_rate_bits),
            last_spend_at_ms: lease.last_spend_at_ms,
        }
    }
}

/// Source public owner fields, borrowing the original canonical lease cohort.
///
/// ```compile_fail
/// fn hidden(owner: wf_domain::governance::view::CreditOwnerView<'_>) {
///     let _ = owner.cleanup_head;
/// }
/// ```
/// ```compile_fail
/// fn hidden(owner: wf_domain::governance::view::CreditOwnerView<'_>) {
///     let _ = owner.policy_version;
/// }
/// ```
pub struct CreditOwnerView<'a> {
    /// Original opaque scope bytes.
    pub scope: &'a [u8],
    /// Original capacity ceiling.
    pub limit: u64,
    /// Original free capacity.
    pub free: u64,
    /// Original greatest generation.
    pub epoch: u64,
    /// Original configuration generation.
    pub config_version: u64,
    /// Optional captured fingerprint, encoded for the public response.
    pub policy_version_hash: Option<CreditPolicyVersionHash>,
    leases: &'a [CreditLeaseData],
}

impl CreditOwnerView<'_> {
    /// Visits projected leases in their original ascending shard order.
    /// Iteration and any caller copies belong to the caller's resource account.
    pub fn leases(
        &self,
    ) -> impl ExactSizeIterator<Item = CreditLeaseView> + DoubleEndedIterator + '_ {
        self.leases.iter().map(CreditLeaseView::from_lease)
    }
}

/// Public source reply shapes, borrowing only the decoded original reply.
pub enum CreditReplyView<'a> {
    /// Successful lease or renewal, with public owner and selected lease.
    Lease {
        /// Original public owner.
        owner: CreditOwnerView<'a>,
        /// Original selected lease.
        lease: CreditLeaseView,
    },
    /// Successful identified spend, including its original ID order.
    Spent {
        /// Original public owner.
        owner: CreditOwnerView<'a>,
        /// Original selected lease.
        lease: CreditLeaseView,
        /// Exact original reservation IDs, without payload copies.
        reservation_ids: &'a [Vec<u8>],
    },
    /// Successful Get or Release.
    Owner(CreditOwnerView<'a>),
    /// Bounded cleanup result, without exposing the internal response owner.
    Cleaned {
        /// Actual deleted IDs.
        deleted: u16,
        /// Whether queued work remains.
        pending: bool,
    },
    /// Original deterministic business diagnosis.
    Rejected(IdentifiedCreditRejection),
}

/// Projects cross-field response shape; this does not validate or admit ID data.
/// Complete untrusted replies require fresh codec validation before projection.
/// # Errors
/// Refuses inconsistent decision/owner/ID response shapes.
pub fn project_credit_reply(
    _reply: &IdentifiedCreditReply,
) -> Result<CreditReplyView<'_>, CreditError> {
    Err(CreditError::InvalidState)
}

/// Original context for compensating an uncertain spend, without commit proof.
#[derive(Debug, Eq, PartialEq)]
pub struct CreditSpendUnknownView<'a> {
    /// Stable source diagnosis code.
    pub code: &'static str,
    /// Original opaque credit scope.
    pub scope: &'a [u8],
    /// Original selected shard.
    pub shard_id: u64,
    /// Exact original IDs; never regenerate these after uncertainty.
    pub reservation_ids: &'a [Vec<u8>],
    /// Source compensation instruction.
    pub message: &'static str,
}
