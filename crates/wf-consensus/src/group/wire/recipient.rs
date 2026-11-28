//! Whom a peer request is for: a group, known by its member list, and one
//! of its members.

use std::collections::BTreeMap;

use sha2::{Digest, Sha256};

/// Whom a request is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Recipient {
    /// The group's id: [`group_id`] of its member list.
    pub(crate) group: u64,
    /// The member's id.
    pub(crate) member: u64,
}

/// A group's id: the first eight bytes of the SHA-256 of its member list,
/// each member's id and Raft address in id order. Every member is given the
/// same list, so they agree on it; another group lists other addresses.
pub(crate) fn group_id(peers: &BTreeMap<u64, String>) -> u64 {
    let mut digest = Sha256::new();
    for (id, address) in peers {
        digest.update(id.to_be_bytes());
        digest.update(
            u64::try_from(address.len())
                .unwrap_or(u64::MAX)
                .to_be_bytes(),
        );
        digest.update(address.as_bytes());
    }
    u64::from_be_bytes(
        digest
            .finalize()
            .first_chunk::<8>()
            .copied()
            .unwrap_or_default(),
    )
}
