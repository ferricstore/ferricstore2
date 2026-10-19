//! Original owner transfer and complete voter/node validation.

use super::{RaftMembershipError as Error, RaftMembershipLimits};
use wf_domain::workflow::control::{EntryIdentity, EntryStamp};
use wf_types::version::LogIdentity;

/// Ordered joint voter sets and the complete sorted node/address map.
#[derive(Eq, PartialEq)]
pub struct RaftMembership {
    configs: Vec<Vec<u64>>,
    nodes: Vec<(u64, String)>,
}
impl RaftMembership {
    /// Validates and moves the original owners without sorting or copying.
    /// # Errors
    /// Refuses empty commands, ordering, missing voter nodes or limits.
    pub fn new(
        configs: Vec<Vec<u64>>,
        nodes: Vec<(u64, String)>,
        limits: RaftMembershipLimits,
    ) -> Result<Self, Error> {
        if configs.is_empty() {
            return Err(Error::Malformed);
        }
        let value = Self { configs, nodes };
        raft_membership_owned_bytes(&value, limits)?;
        Ok(value)
    }
    /// Borrows outer configuration order and every voter occurrence.
    #[must_use]
    pub fn configs(&self) -> &[Vec<u64>] {
        &self.configs
    }
    /// Borrows complete node metadata, including learner nodes.
    #[must_use]
    pub fn nodes(&self) -> &[(u64, String)] {
        &self.nodes
    }
    /// Moves both original containers without conversion allocation.
    #[must_use]
    pub fn into_parts(self) -> (Vec<Vec<u64>>, Vec<(u64, String)>) {
        (self.configs, self.nodes)
    }
}

/// Measures both initialized and explicit initial data; grants no origin proof.
/// # Errors
/// Refuses malformed complete data, limits or checked-size overflow.
pub fn raft_membership_owned_bytes(
    value: &RaftMembership,
    limits: RaftMembershipLimits,
) -> Result<usize, Error> {
    if value.configs.len() > limits.maximum_configs() || value.nodes.len() > limits.maximum_nodes()
    {
        return Err(Error::Limit);
    }
    if value.nodes.windows(2).any(|w| w[0].0 >= w[1].0)
        || (value.configs.is_empty() && !value.nodes.is_empty())
    {
        return Err(Error::Malformed);
    }
    let mut bytes = 1024_usize;
    let mut voters = 0_usize;
    for config in &value.configs {
        voters = voters.checked_add(config.len()).ok_or(Error::Limit)?;
        if voters > limits.maximum_voter_occurrences() {
            return Err(Error::Limit);
        }
        if config.is_empty() || config.windows(2).any(|w| w[0] >= w[1]) {
            return Err(Error::Malformed);
        }
        bytes = bytes
            .checked_add(512)
            .and_then(|n| n.checked_add(config.len().checked_mul(8)?))
            .ok_or(Error::Limit)?;
        if config
            .iter()
            .any(|id| value.nodes.binary_search_by_key(id, |node| node.0).is_err())
        {
            return Err(Error::Malformed);
        }
    }
    for (_, address) in &value.nodes {
        if address.len() > limits.maximum_address_bytes() {
            return Err(Error::Limit);
        }
        bytes = bytes
            .checked_add(512)
            .and_then(|n| n.checked_add(address.len()))
            .ok_or(Error::Limit)?;
    }
    if bytes > limits.maximum_owned_bytes() {
        return Err(Error::Limit);
    }
    Ok(bytes)
}

/// Last APPLIED membership and its original complete identity/stamp.
#[derive(Eq, PartialEq)]
pub struct AppliedRaftMembership {
    entry: Option<EntryIdentity>,
    stamp: Option<EntryStamp>,
    membership: RaftMembership,
}
impl AppliedRaftMembership {
    /// Builds explicitly uninitialized local state, not an empty membership command.
    #[must_use]
    pub fn initial() -> Self {
        Self {
            entry: None,
            stamp: None,
            membership: RaftMembership {
                configs: Vec::new(),
                nodes: Vec::new(),
            },
        }
    }
    /// Moves mathematical values; construction grants no durable origin proof.
    #[must_use]
    pub const fn new(entry: EntryIdentity, stamp: EntryStamp, membership: RaftMembership) -> Self {
        Self {
            entry: Some(entry),
            stamp: Some(stamp),
            membership,
        }
    }
    /// Original membership event identity, distinct from current global control.
    #[must_use]
    pub const fn entry(&self) -> Option<EntryIdentity> {
        self.entry
    }
    /// Full original membership log identity.
    #[must_use]
    pub fn log_id(&self) -> Option<LogIdentity> {
        self.entry.map(EntryIdentity::log)
    }
    /// Original explicit stamp; None means uninitialized.
    #[must_use]
    pub const fn stamp(&self) -> Option<EntryStamp> {
        self.stamp
    }
    /// Complete owned voter/node data.
    #[must_use]
    pub const fn membership(&self) -> &RaftMembership {
        &self.membership
    }
    /// Moves scalar metadata and the same membership owner.
    #[must_use]
    pub fn into_parts(self) -> (Option<EntryIdentity>, Option<EntryStamp>, RaftMembership) {
        (self.entry, self.stamp, self.membership)
    }
}
