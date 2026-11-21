//! What a node's shards do (ADR-018): a single node decides requests and
//! logs them itself; in a replicated group the leader decides and proposes,
//! and a follower only applies the leader's committed rounds and refuses
//! client requests as not led here. The node's shards share one role.

use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};

const SINGLE: u8 = 0;
const LEADER: u8 = 1;
const FOLLOWER: u8 = 2;

/// The shards' shared role.
#[derive(Clone)]
pub(crate) struct Role(Arc<AtomicU8>);

impl Role {
    /// A node that is not replicated.
    pub(crate) fn single() -> Self {
        Self(Arc::new(AtomicU8::new(SINGLE)))
    }

    /// A group member, following until it is promoted.
    pub(crate) fn follower() -> Self {
        Self(Arc::new(AtomicU8::new(FOLLOWER)))
    }

    /// Whether the shards decide client requests.
    pub(crate) fn serves(&self) -> bool {
        self.0.load(Ordering::Acquire) != FOLLOWER
    }

    pub(crate) fn lead(&self) {
        self.0.store(LEADER, Ordering::Release);
    }

    pub(crate) fn follow(&self) {
        self.0.store(FOLLOWER, Ordering::Release);
    }
}
