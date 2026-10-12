//! Distinct nonwrapping committed positions and authoritative revisions.

/// A group-local log position, meaningful only within one recovery namespace.
///
/// This is a client-visible scoped index, not complete durable log identity.
/// Adapters compare [`LogIdentity`] plus group/recovery and prepared entry bytes;
/// equal indexes alone never prove equal log entries across any namespace.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct AppliedPosition(u64);

impl AppliedPosition {
    /// Preserves a position, including zero before the first entry.
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    /// Reads the exact group-local position.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }

    /// Advances without wrapping.
    #[must_use]
    pub const fn checked_next(self) -> Option<Self> {
        match self.0.checked_add(1) {
            Some(next) => Some(Self(next)),
            None => None,
        }
    }
}

/// A coherent authoritative snapshot revision within one group/recovery scope.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Revision(u64);

impl Revision {
    /// Preserves a revision, including zero.
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    /// Reads the explicit revision.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }

    /// Advances without wrapping.
    #[must_use]
    pub const fn checked_next(self) -> Option<Self> {
        match self.0.checked_add(1) {
            Some(next) => Some(Self(next)),
            None => None,
        }
    }
}

/// Consensus-neutral lossless initial term/leader-node/index identity.
///
/// Domain/application envelopes additionally bind group and recovery identity.
/// The leader-node component preserves default multi-leader term semantics;
/// adapters must not discard it or infer entry identity from an index alone.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct LogIdentity {
    term: u64,
    leader_node: u64,
    index: AppliedPosition,
}

impl LogIdentity {
    /// Preserves an explicitly committed identity without choosing a leader.
    #[must_use]
    pub const fn new(term: u64, leader_node: u64, index: AppliedPosition) -> Self {
        Self {
            term,
            leader_node,
            index,
        }
    }
    /// Reads the committed term.
    #[must_use]
    pub const fn term(self) -> u64 {
        self.term
    }
    /// Reads the committed leader/node identity.
    #[must_use]
    pub const fn leader_node(self) -> u64 {
        self.leader_node
    }
    /// Reads the group/recovery scoped client-visible index.
    #[must_use]
    pub const fn index(self) -> AppliedPosition {
        self.index
    }
}
