//! Raft traffic between members over TCP (ADR-018), separate from client
//! listeners (ADR-001).
//!
//! `OpenRaft` gives each replication stream and each vote its own network
//! client ([`Link`]) and calls it one request at a time. A frame is a `u32`
//! length and a [`wire`] body of at most the member's frame limit
//! ([`MAX_FRAME_BYTES`] by default; every member is given the same). A
//! member serves each connection's requests in order (`server`). An append
//! whose entries do not fit one frame goes as several, each following the
//! one before; a snapshot for a member the log no longer reaches, or one a
//! leader that trims its log fetches, goes as its metadata and length,
//! then its bytes in frames of their own (`frame`), with a longer
//! deadline. Each request names its group and member; a member answers one
//! meant for another with a refusal that names both.

use std::collections::BTreeMap;
use std::future::{Future, ready};
use std::sync::Arc;
use std::time::Duration;

use openraft::AnyError;
use openraft::error::Unreachable;
use openraft::network::RaftNetworkFactory;

use super::Config;
use super::wire::{self, Recipient};

mod ask;
mod frame;
mod link;
mod server;
#[cfg(test)]
mod tests;

pub(super) use ask::{newest, read_index, status};
pub use link::Link;
pub use server::serve;
pub(super) use server::serving;

/// The largest frame a member sends or accepts unless configured
/// otherwise: an append part or a snapshot chunk fits well within it.
pub const MAX_FRAME_BYTES: u32 = 256 * 1024 * 1024;
/// How long a connection attempt may take.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(1);
/// How long sending and installing a snapshot may take.
const SNAPSHOT_TIMEOUT: Duration = Duration::from_secs(120);

fn unreachable(error: &impl std::fmt::Display) -> Unreachable<Config> {
    Unreachable::new(&AnyError::error(error.to_string()))
}

/// The members' Raft addresses: `OpenRaft`'s network factory.
pub struct Peers {
    addresses: Arc<BTreeMap<u64, String>>,
    group: u64,
    limit: u32,
}

impl Peers {
    /// The network to `addresses` (`host:port`, resolved on each
    /// connection), by member id: the group they list.
    #[must_use]
    pub fn new(addresses: BTreeMap<u64, String>) -> Self {
        Self::limited(addresses, MAX_FRAME_BYTES)
    }

    /// As [`Self::new`], in frames of at most `limit` bytes.
    #[must_use]
    pub fn limited(addresses: BTreeMap<u64, String>, limit: u32) -> Self {
        Self {
            group: wire::group_id(&addresses),
            addresses: Arc::new(addresses),
            limit,
        }
    }
}

impl RaftNetworkFactory<Config> for Peers {
    type Network = Link;

    fn new_client(&mut self, target: u64, (): &()) -> impl Future<Output = Link> + Send {
        ready(Link {
            address: self.addresses.get(&target).cloned(),
            recipient: Recipient {
                group: self.group,
                member: target,
            },
            stream: None,
            limit: self.limit,
        })
    }
}
