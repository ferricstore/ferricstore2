//! Complete bounded BasicNode-compatible membership values, without Raft authority.

use std::fmt;

mod value;
pub use value::{AppliedRaftMembership, RaftMembership, raft_membership_owned_bytes};

/// Positive configurable data/ownership ceilings; tiny profiles may refuse work.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RaftMembershipLimits {
    configs: usize,
    voters: usize,
    nodes: usize,
    address: usize,
    wire: usize,
    owned: usize,
}

impl RaftMembershipLimits {
    /// Sets counts, per-address bytes, command wire bytes and typed owner bytes.
    /// # Errors
    /// Refuses any zero ceiling.
    pub fn new(
        maximum_configs: usize,
        maximum_voter_occurrences: usize,
        maximum_nodes: usize,
        maximum_address_bytes: usize,
        maximum_wire_bytes: usize,
        maximum_owned_bytes: usize,
    ) -> Result<Self, RaftMembershipError> {
        if [
            maximum_configs,
            maximum_voter_occurrences,
            maximum_nodes,
            maximum_address_bytes,
            maximum_wire_bytes,
            maximum_owned_bytes,
        ]
        .contains(&0)
        {
            return Err(RaftMembershipError::InvalidLimits);
        }
        Ok(Self {
            configs: maximum_configs,
            voters: maximum_voter_occurrences,
            nodes: maximum_nodes,
            address: maximum_address_bytes,
            wire: maximum_wire_bytes,
            owned: maximum_owned_bytes,
        })
    }
    /// Maximum ordered voter configurations, including repeated configurations.
    #[must_use]
    pub const fn maximum_configs(self) -> usize {
        self.configs
    }
    /// Maximum voter occurrences across all configurations.
    #[must_use]
    pub const fn maximum_voter_occurrences(self) -> usize {
        self.voters
    }
    /// Maximum complete node rows, including learners.
    #[must_use]
    pub const fn maximum_nodes(self) -> usize {
        self.nodes
    }
    /// Maximum UTF8 address bytes per node; empty remains valid.
    #[must_use]
    pub const fn maximum_address_bytes(self) -> usize {
        self.address
    }
    /// Maximum committed command BODY bytes, excluding persisted wrappers.
    #[must_use]
    pub const fn maximum_wire_bytes(self) -> usize {
        self.wire
    }
    /// Maximum logical typed owner, including descriptors and all addresses.
    #[must_use]
    pub const fn maximum_owned_bytes(self) -> usize {
        self.owned
    }
    /// Tests the componentwise supported-data relation without allocating.
    #[must_use]
    pub fn is_within(self, supported: Self) -> bool {
        self.configs <= supported.configs
            && self.voters <= supported.voters
            && self.nodes <= supported.nodes
            && self.address <= supported.address
            && self.wire <= supported.wire
            && self.owned <= supported.owned
    }
}
impl Default for RaftMembershipLimits {
    fn default() -> Self {
        Self {
            configs: 64,
            voters: 1_000_000,
            nodes: 65_536,
            address: 1_048_576,
            wire: 16_777_216,
            owned: 67_108_864,
        }
    }
}

/// Fixed schema/admission failures, never containing an address or command body.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RaftMembershipError {
    /// A required positive profile value is zero.
    InvalidLimits,
    /// A data/owner/wire limit or checked arithmetic was exceeded.
    Limit,
    /// An admitted fallible reservation failed.
    Allocation,
    /// Framing, UTF8, ordering or voter correspondence is invalid.
    Malformed,
    /// The closed frame version is unsupported.
    UnsupportedVersion,
    /// The encoded namespace differs from the independent expected namespace.
    NamespaceMismatch,
}
impl fmt::Display for RaftMembershipError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InvalidLimits => "invalid membership limits",
            Self::Limit => "membership limit",
            Self::Allocation => "membership allocation failed",
            Self::Malformed => "malformed membership",
            Self::UnsupportedVersion => "unsupported membership version",
            Self::NamespaceMismatch => "membership namespace mismatch",
        })
    }
}
impl std::error::Error for RaftMembershipError {}
