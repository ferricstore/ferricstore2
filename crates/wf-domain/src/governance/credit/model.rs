//! Canonical mathematical owner data; immutable views establish no provenance.

use crate::workflow::control::PreparedDigest;
use std::fmt;
use wf_types::version::TimestampMillis;

/// Source reservation amount or an inert captured legacy non-amount value.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CreditReservationValue {
    /// Identified mathematical amount, including ignored zero legacy values.
    Amount(u64),
    /// Preserved captured bytes; this value cannot release any credits.
    Opaque(Vec<u8>),
}

/// Complete editable lease capture, with ascending unique reservation IDs.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CreditLeaseData {
    /// Original shard identity.
    pub shard_id: u64,
    /// Positive owner-wide generation assigned on a new lease.
    pub epoch: u64,
    /// Actual lease deadline, inclusive for expiry.
    pub expires_at_ms: u64,
    /// Unspent credits owned by this lease.
    pub available: u64,
    /// Credits held by active work, including anonymous compatibility spends.
    pub in_use: u64,
    /// Requested idle credits, bounded by available.
    pub pending_reclaim: u64,
    /// Captured nonnegative finite binary64 drain rate, preserved as data.
    pub drain_rate_bits: u64,
    /// Last new successful spend time, independent of duplicate lookup.
    pub last_spend_at_ms: Option<u64>,
    /// Actual last detached reservation page; zero means none.
    pub reservation_page: u16,
    /// Actual final-page fill count.
    pub reservation_page_fill: u16,
    /// Complete relevant identified values, ascending by opaque ID bytes.
    pub reservations: Vec<(Vec<u8>, CreditReservationValue)>,
}

/// Complete editable owner capture; callers must restore before decisions.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CreditOwnerData {
    /// Original opaque governance scope.
    pub scope: Vec<u8>,
    /// Current configured global credit ceiling.
    pub limit: u64,
    /// Unallocated capacity; over-limit in-use credit requires zero free.
    pub free: u64,
    /// Largest allocated lease generation.
    pub epoch: u64,
    /// Monotone configuration generation.
    pub config_version: u64,
    /// Captured mathematical policy fingerprint; no current-policy proof.
    pub policy_fingerprint: Option<PreparedDigest>,
    /// First pending detached-reservation cleanup entry.
    pub cleanup_head: u64,
    /// Last allocated cleanup entry; empty is head 1 / tail 0.
    pub cleanup_tail: u64,
    /// Complete leases in strictly ascending shard order.
    pub leases: Vec<CreditLeaseData>,
}

/// Sealed canonical mathematical owner; grants no storage or remote authority.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CreditOwner {
    pub(super) data: CreditOwnerData,
}

impl CreditOwner {
    /// Borrows the unchanged canonical data owner.
    #[must_use]
    pub const fn view(&self) -> &CreditOwnerData {
        &self.data
    }

    /// Borrows one exact current shard capture.
    #[must_use]
    pub fn lease(&self, shard_id: u64) -> Option<&CreditLeaseData> {
        self.data
            .leases
            .binary_search_by_key(&shard_id, |lease| lease.shard_id)
            .ok()
            .map(|index| &self.data.leases[index])
    }

    /// Moves data without creating a new confirmed or durable owner.
    #[must_use]
    pub fn into_data(self) -> CreditOwnerData {
        self.data
    }
}

/// Positive logical resource profile, separate from source business ceilings.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CreditLimits {
    pub(super) leases: usize,
    pub(super) reservations: usize,
    pub(super) owned_bytes: usize,
    pub(super) work: u64,
}

impl Default for CreditLimits {
    fn default() -> Self {
        Self {
            leases: 65_536,
            reservations: 1_048_576,
            owned_bytes: 128 * 1024 * 1024,
            work: 128 * 1024 * 1024,
        }
    }
}

impl CreditLimits {
    /// Selects positive independent count, owned-byte and work ceilings.
    /// # Errors
    /// Refuses zero ceilings or descriptor-count overflow.
    pub fn new(
        leases: usize,
        reservations: usize,
        owned_bytes: usize,
        work: u64,
    ) -> Result<Self, CreditError> {
        if [leases, reservations, owned_bytes].contains(&0)
            || work == 0
            || leases
                .checked_mul(std::mem::size_of::<CreditLeaseData>())
                .is_none()
            || reservations
                .checked_mul(std::mem::size_of::<(Vec<u8>, CreditReservationValue)>())
                .is_none()
        {
            return Err(CreditError::InvalidLimits);
        }
        Ok(Self {
            leases,
            reservations,
            owned_bytes,
            work,
        })
    }
    /// Maximum complete lease count.
    #[must_use]
    pub const fn max_leases(self) -> usize {
        self.leases
    }
    /// Maximum complete identified-entry count across leases.
    #[must_use]
    pub const fn max_reservations(self) -> usize {
        self.reservations
    }
    /// Maximum simultaneous named logical owners, not native process RSS.
    #[must_use]
    pub const fn max_owned_bytes(self) -> usize {
        self.owned_bytes
    }
    /// Maximum continuous admitted logical work.
    #[must_use]
    pub const fn max_work(self) -> u64 {
        self.work
    }
}

/// Explicit time and immutable caller values for one pure owner decision.
#[derive(Clone, Copy)]
pub enum CreditOperation<'a> {
    /// Source monotone configuration, with an optional mathematical fingerprint.
    Configure {
        /// Requested nonnegative ceiling.
        limit: u64,
        /// Requested monotone configuration generation.
        config_version: u64,
        /// Optional captured mathematical fingerprint.
        policy_fingerprint: Option<PreparedDigest>,
    },
    /// Partial grant from global free capacity, after expiry.
    Grant {
        /// Requested shard.
        shard_id: u64,
        /// Positive maximum partial grant.
        amount: u64,
        /// Explicit trusted model time.
        now: TimestampMillis,
        /// Positive lease duration.
        ttl_ms: u64,
    },
    /// New spend or complete identified retry, after expiry.
    Spend {
        /// Existing shard lease.
        shard_id: u64,
        /// Positive requested count.
        amount: u64,
        /// Explicit trusted model time.
        now: TimestampMillis,
        /// Optional positive expiry extension.
        ttl_ms: Option<u64>,
        /// Unique IDs, or empty for anonymous helper compatibility.
        reservation_ids: &'a [&'a [u8]],
    },
    /// Extends an existing unexpired lease.
    Renew {
        /// Existing shard lease.
        shard_id: u64,
        /// Explicit trusted model time.
        now: TimestampMillis,
        /// Positive requested lease duration.
        ttl_ms: u64,
    },
    /// Source identified release; absent/invalid ID lists are a no-op.
    Release {
        /// Captured shard identity.
        shard_id: u64,
        /// Source requested count.
        amount: u64,
        /// Actual identified list, distinct from absence.
        reservation_ids: Option<&'a [&'a [u8]]>,
    },
    /// Amount already identified by an external authoritative row operation.
    ReleaseIdentifiedAmount {
        /// Shard whose external reservation rows were already classified.
        shard_id: u64,
        /// Nonnegative externally identified release amount.
        amount: u64,
    },
    /// Drops leases with deadline at or before explicit time and rebalances.
    ReclaimExpired {
        /// Explicit expiry boundary.
        now: TimestampMillis,
    },
}

/// Known source business failure; an adapter must establish durable outcome.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CreditRejection {
    /// Original structured exhaustion values.
    LimitExceeded {
        /// Original requested count.
        requested: u64,
        /// Actual global free credits after expiry.
        free: u64,
        /// Actual configured ceiling.
        limit: u64,
    },
    /// New lease generation cannot be allocated.
    GenerationExhausted,
    /// Equal configuration generation carries conflicting limit or fingerprint.
    ConfigurationConflict,
    /// Nonempty identified spend list does not match its amount or uniqueness.
    ReservationIdsMismatch,
    /// Only some supplied spend IDs already exist.
    ReservationConflict,
    /// New spend time precedes this lease's last successful spend.
    BackwardTime,
    /// Renewal's lease no longer exists after expiry.
    LeaseExpired,
}

impl CreditRejection {
    /// Original source code for exhaustion; other rejections are typed separately.
    #[must_use]
    pub const fn code(self) -> Option<&'static str> {
        if matches!(self, Self::LimitExceeded { .. }) {
            Some("GOVERNANCE_LIMIT_EXCEEDED")
        } else {
            None
        }
    }
}

/// Complete business classification, with the resulting owner kept separately.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CreditDecision {
    /// Requested configuration was applied or is already current/stale.
    Configured,
    /// Lease grant succeeded, possibly partially.
    Granted {
        /// Actual shard whose partial grant succeeded.
        shard_id: u64,
    },
    /// Spend succeeded or all identified IDs were already present.
    Spent {
        /// Actual spent or already identified shard.
        shard_id: u64,
    },
    /// Existing lease deadline was extended or already later.
    Renewed {
        /// Actual renewed shard.
        shard_id: u64,
    },
    /// Identified release completed, including an idempotent no-op.
    Released,
    /// Explicit expiry/rebalance completed.
    Reclaimed,
    /// Known rejection, preserving source expiry/reclaim-mark side effects.
    Denied(CreditRejection),
}

/// Mathematical owner plus business result; not a committed transition token.
#[derive(Debug, Eq, PartialEq)]
pub struct CreditTransition {
    pub(super) owner: CreditOwner,
    pub(super) decision: CreditDecision,
}

impl CreditTransition {
    /// Borrows the complete resulting owner.
    #[must_use]
    pub const fn owner(&self) -> &CreditOwner {
        &self.owner
    }
    /// Reads the original business classification.
    #[must_use]
    pub const fn decision(&self) -> CreditDecision {
        self.decision
    }
    /// Moves both values for a later real transaction adapter.
    #[must_use]
    pub fn into_parts(self) -> (CreditOwner, CreditDecision) {
        (self.owner, self.decision)
    }
}

/// Operational failure; never automatically retained as a business rejection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CreditError {
    /// Invalid selected positive ceilings.
    InvalidLimits,
    /// Captured canonical state/order/capacity is inconsistent.
    InvalidState,
    /// Explicit operation fields violate their scalar representation.
    InvalidInput,
    /// Complete operation exceeds its count/owned/work profile.
    Limit,
    /// Fallible output reservation failed.
    Allocation,
}

impl fmt::Display for CreditError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InvalidLimits => "invalid credit owner limits",
            Self::InvalidState => "invalid canonical credit owner",
            Self::InvalidInput => "invalid credit owner operation",
            Self::Limit => "credit owner resource limit",
            Self::Allocation => "credit owner reservation failed",
        })
    }
}
impl std::error::Error for CreditError {}
