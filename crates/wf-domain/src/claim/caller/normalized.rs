//! Normalized caller meaning with original leaf bytes and three owned spines.

use super::{ClaimPartitionMode, ClaimReplyMode, ClaimValuesMode};

/// Public caller comparison value, without policy, routing or apply authority.
///
/// Only three reference spines are owned. Getter slices borrow this object;
/// their binary leaves remain borrowed from the original caller owners.
pub struct NormalizedClaimDue<'a> {
    pub(super) workflow_type: &'a [u8],
    pub(super) worker: &'a [u8],
    pub(super) states_any: bool,
    pub(super) state_names: Vec<&'a [u8]>,
    pub(super) partition_mode: ClaimPartitionMode,
    pub(super) partition_keys: Vec<&'a [u8]>,
    pub(super) values_mode: ClaimValuesMode,
    pub(super) value_names: Vec<&'a [u8]>,
    pub(super) lease_ms: u64,
    pub(super) limit: u64,
    pub(super) priority: Option<u64>,
    pub(super) reclaim_expired: bool,
    pub(super) reclaim_ratio: u64,
    pub(super) reply_mode: ClaimReplyMode,
    pub(super) payload_enabled: bool,
    pub(super) payload_max_bytes: Option<u64>,
    pub(super) governance_limit_scope: Option<&'a [u8]>,
    pub(super) governance_shard_id: Option<u64>,
    pub(super) requested_now_ms: Option<u64>,
    pub(super) block_ms: Option<u64>,
}

impl<'a> NormalizedClaimDue<'a> {
    /// Original workflow-type bytes.
    #[must_use]
    pub const fn workflow_type(&self) -> &'a [u8] {
        self.workflow_type
    }

    /// Original worker bytes.
    #[must_use]
    pub const fn worker(&self) -> &'a [u8] {
        self.worker
    }

    /// Any flag and keep-last ordered explicit states, including mixed input.
    #[must_use]
    pub fn states(&self) -> (bool, &[&'a [u8]]) {
        (self.states_any, &self.state_names)
    }

    /// Caller partition meaning and keep-first ordered literal keys.
    #[must_use]
    pub fn partitions(&self) -> (ClaimPartitionMode, &[&'a [u8]]) {
        (self.partition_mode, &self.partition_keys)
    }

    /// Caller projection meaning and keep-first ordered value names.
    #[must_use]
    pub fn values(&self) -> (ClaimValuesMode, &[&'a [u8]]) {
        (self.values_mode, &self.value_names)
    }

    /// Caller lease with the fixed source default of 30000ms.
    #[must_use]
    pub const fn lease_ms(&self) -> u64 {
        self.lease_ms
    }

    /// Caller count with the fixed source default of one.
    #[must_use]
    pub const fn limit(&self) -> u64 {
        self.limit
    }

    /// Caller priority, without filling absence from current policy.
    #[must_use]
    pub const fn priority(&self) -> Option<u64> {
        self.priority
    }

    /// Caller reclaim flag with the fixed source default of true.
    #[must_use]
    pub const fn reclaim_expired(&self) -> bool {
        self.reclaim_expired
    }

    /// Caller reclaim ratio with the fixed source default of 25.
    #[must_use]
    pub const fn reclaim_ratio(&self) -> u64 {
        self.reclaim_ratio
    }

    /// Caller reply projection with the fixed records default.
    #[must_use]
    pub const fn reply_mode(&self) -> ClaimReplyMode {
        self.reply_mode
    }

    /// Payload flag after explicit payload, legacy full and reply defaults.
    #[must_use]
    pub const fn payload_enabled(&self) -> bool {
        self.payload_enabled
    }

    /// Caller cap; absent remains dependent on a later configured policy.
    #[must_use]
    pub const fn payload_max_bytes(&self) -> Option<u64> {
        self.payload_max_bytes
    }

    /// Original optional governance scope, including an empty binary.
    #[must_use]
    pub const fn governance_limit_scope(&self) -> Option<&'a [u8]> {
        self.governance_limit_scope
    }

    /// Optional governance shard, including zero and partial pairs.
    #[must_use]
    pub const fn governance_shard_id(&self) -> Option<u64> {
        self.governance_shard_id
    }

    /// Optional caller identity value, never a trusted committed clock.
    #[must_use]
    pub const fn requested_now_ms(&self) -> Option<u64> {
        self.requested_now_ms
    }

    /// Optional attempt wait, excluded from the mutation client intent.
    #[must_use]
    pub const fn block_ms(&self) -> Option<u64> {
        self.block_ms
    }
}
