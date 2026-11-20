//! Routes requests to shard threads and splits claims over them.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;

use wf_application::commands::queue::{ClaimFlows, Partition, ServiceError, shard_for_slot};

use crate::supervision::shard::Request;
use crate::supervision::shard::state::Peeked;
use crate::supervision::waiters::Waiters;

/// The node's connection to one shard thread.
pub(crate) struct ShardLink {
    pub(crate) requests: mpsc::Sender<Request>,
    /// Due-indexed Flows, published after each round.
    pub(crate) ready: Arc<AtomicU64>,
    /// Leased running Flows, published after each round.
    pub(crate) leased: Arc<AtomicU64>,
}

/// Every shard, and the waiter thread. Dropping the last reference closes the
/// shards' request channels.
pub(crate) struct Router {
    pub(crate) shards: Vec<ShardLink>,
    pub(crate) shard_count: u16,
    pub(crate) waiters: Waiters,
}

pub(crate) fn shard_index(partition: &Partition, shards: u16) -> Option<usize> {
    shard_for_slot(partition.slot(), shards).map(usize::from)
}

/// Fails a request that never reached its shard.
fn refuse(request: Request) {
    match request {
        Request::Create { done, .. } | Request::Complete { done, .. } => {
            done(Err(ServiceError::Unavailable));
        }
        Request::Claim { done, .. } => done(Err(ServiceError::Unavailable)),
        Request::Capture { done, .. } => done(Err(ServiceError::Unavailable)),
        Request::Flow { done, .. } => done(Err(ServiceError::Unavailable)),
        Request::Peek { done, .. } => done(Peeked::default()),
    }
}

impl Router {
    pub(crate) fn send(&self, index: usize, request: Request) {
        match self.shards.get(index) {
            Some(shard) => {
                if let Err(returned) = shard.requests.send(request) {
                    refuse(returned.0);
                }
            }
            None => refuse(request),
        }
    }

    pub(crate) fn route(&self, partition: &Partition) -> usize {
        shard_index(partition, self.shard_count).unwrap_or(0)
    }

    /// Shards that may hold Flows the claim can take: the owners of its
    /// partitions, or every shard with a nonzero weight.
    pub(crate) fn holders(
        &self,
        request: &ClaimFlows,
        partitions: Option<&[Partition]>,
    ) -> Vec<(usize, u64)> {
        match partitions {
            Some(partitions) => {
                let mut owners: Vec<usize> = partitions
                    .iter()
                    .map(|partition| self.route(partition))
                    .collect();
                owners.sort_unstable();
                owners.dedup();
                owners.into_iter().map(|owner| (owner, 1)).collect()
            }
            None => self.weights(request),
        }
    }

    /// Per shard, the indexed Flows the claim may take: due-indexed Flows,
    /// plus leased ones when it reclaims. Shards with none are left out.
    fn weights(&self, request: &ClaimFlows) -> Vec<(usize, u64)> {
        let running = request.state.as_deref() == Some(b"running".as_slice());
        self.shards
            .iter()
            .enumerate()
            .map(|(index, shard)| {
                let ready = if running {
                    0
                } else {
                    shard.ready.load(Ordering::Relaxed)
                };
                let leased = if running || request.reclaim.is_some() {
                    shard.leased.load(Ordering::Relaxed)
                } else {
                    0
                };
                (index, ready.saturating_add(leased))
            })
            .filter(|(_, weight)| *weight > 0)
            .collect()
    }
}

/// Splits `limit` over weighted shards: proportional floors, then the rest
/// one at a time in order. Shards with a zero share are left out.
pub(crate) fn split(limit: usize, weights: &[(usize, u64)]) -> Vec<(usize, usize)> {
    let total: u128 = weights.iter().map(|(_, weight)| u128::from(*weight)).sum();
    let wanted = u128::try_from(limit).unwrap_or(0);
    let mut shares: Vec<(usize, u128)> = weights
        .iter()
        .map(|(index, weight)| {
            let share = wanted
                .checked_mul(u128::from(*weight))
                .and_then(|scaled| scaled.checked_div(total))
                .unwrap_or(0);
            (*index, share)
        })
        .collect();
    let assigned: u128 = shares.iter().map(|(_, share)| *share).sum();
    let mut rest = wanted.saturating_sub(assigned);
    for (_, share) in &mut shares {
        if rest == 0 {
            break;
        }
        *share = share.saturating_add(1);
        rest = rest.saturating_sub(1);
    }
    shares
        .into_iter()
        .filter(|(_, share)| *share > 0)
        .filter_map(|(index, share)| Some((index, usize::try_from(share).ok()?)))
        .collect()
}
