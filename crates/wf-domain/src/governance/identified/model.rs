//! Borrowed original ID order and canonical private response values.

use crate::governance::credit::{CreditOwner, CreditRejection};
use crate::workflow::control::PreparedDigest;

/// Explicit owner configuration; no current-policy or routing proof.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CreditConfiguration {
    /// Nonnegative global capacity ceiling.
    pub limit: u64,
    /// Monotone generation; absence can only preserve an existing limit.
    pub config_version: Option<u64>,
    /// Actual captured fingerprint, independent of public input normalization.
    pub policy_fingerprint: Option<PreparedDigest>,
}

/// Private owner mutation, with original identified payload owners borrowed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IdentifiedCreditMutation<'a> {
    /// Partial lease grant from global free capacity.
    Lease {
        /// Actual owner shard.
        shard_id: u64,
        /// Positive requested credits, at most1000.
        amount: u64,
        /// Positive lease duration.
        ttl_ms: u64,
    },
    /// Spend the exact original identified batch in the expected generation.
    Spend {
        /// Actual owner shard.
        shard_id: u64,
        /// Original observed lease generation.
        lease_epoch: u64,
        /// Positive credits; must equal the unique ID count.
        amount: u64,
        /// Optional positive lease extension.
        ttl_ms: Option<u64>,
        /// Exact original IDs and order, not regenerated after an unknown result.
        reservation_ids: &'a [Vec<u8>],
    },
    /// Release only actual active members in the current lease generation.
    Release {
        /// Actual owner shard.
        shard_id: u64,
        /// Positive credits; must equal the unique ID count.
        amount: u64,
        /// Exact original reservation IDs.
        reservation_ids: &'a [Vec<u8>],
    },
    /// Extend a live owner lease without shortening its deadline.
    Renew {
        /// Actual owner shard.
        shard_id: u64,
        /// Positive lease duration.
        ttl_ms: u64,
    },
    /// Reclaim expired leases and return the actual owner.
    Get,
    /// Reclaim expiry, then delete at most one queued reservation page.
    Cleanup,
}

/// Mathematical request fields; the adapter must bind its actual revision.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct IdentifiedCreditRequest<'a> {
    /// Actual opaque governance scope, distinct from tenant/group/recovery.
    pub scope: &'a [u8],
    /// Actual configured owner shard count, rechecked by the real store.
    pub shard_count: u64,
    /// Optional complete owner configuration.
    pub configuration: Option<CreditConfiguration>,
    /// Exact private operation.
    pub mutation: IdentifiedCreditMutation<'a>,
}

/// Business-only identified protocol diagnosis; operational failures are separate.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IdentifiedCreditRejection {
    /// Existing pure capacity/configuration/spend diagnosis.
    Credit(CreditRejection),
    /// No authoritative owner exists for this operation.
    NotFound,
    /// Creating a new owner requires its capacity configuration.
    ConfigurationRequired,
    /// Changing an existing capacity requires a generation.
    ConfigurationVersionRequired,
    /// Retirement/expiry changed the expected lease generation.
    LeaseGenerationChanged,
    /// Appending would exceed256 pages of256 IDs in this epoch.
    ReservationHistoryFull,
    /// Expiry cannot enqueue all required tasks within the256-task bound.
    CleanupBacklogFull,
}

/// Private business classification, without a durable-success certificate.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IdentifiedCreditDecision {
    /// A partial or full grant succeeded.
    Granted {
        /// Actual selected shard.
        shard_id: u64,
    },
    /// A new or all-known identified spend succeeded.
    Spent {
        /// Actual selected shard.
        shard_id: u64,
    },
    /// A live lease renewal succeeded.
    Renewed {
        /// Actual selected shard.
        shard_id: u64,
    },
    /// Identified release completed, including an already absent/released ID.
    Released,
    /// The current post-expiry owner is returned.
    Owner,
    /// One cleanup page, or the actual empty queue, was processed.
    Cleaned {
        /// Actual deleted reservation count, at most256.
        deleted: u16,
        /// Whether another cleanup page/task remains.
        pending: bool,
    },
    /// A deterministic business refusal, independent of operational failures.
    Rejected(IdentifiedCreditRejection),
}

/// Original private response; public mapping must omit internal owner fields.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IdentifiedCreditReply {
    /// Exact original classification.
    pub decision: IdentifiedCreditDecision,
    /// Original represented owner, never proof of CURRENT ownership.
    pub owner: Option<CreditOwner>,
    /// Original spent IDs in caller order; empty for other operations.
    pub reservation_ids: Vec<Vec<u8>>,
}
