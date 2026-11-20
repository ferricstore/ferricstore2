//! The queue node (ADR-011).
//!
//! Opening the node opens the group log, replays every logged change into
//! the shards, then starts one thread per shard and the completion thread.
//! The node implements the queue service by routing each Flow to the shard
//! that owns its partition. A claim without a partition filter is split over
//! the shards by their counts of due-indexed Flows.

mod gather;

use std::fmt;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, mpsc};
use std::thread::JoinHandle;

use wf_application::commands::queue::{
    ClaimFlows, ClaimedFlow, CompleteFlow, CreateFlows, ItemOutcome, Partition, QueueService,
    Reply, SLOTS, ServiceError, shard_for_slot,
};
use wf_domain::workflow::Outcome;
use wf_storage::log::{GroupLog, GroupLogError, GroupLogLimits};

use crate::supervision::shard::command::Logged;
use crate::supervision::shard::state::ShardState;
use crate::supervision::shard::{self, Links, Request};
use crate::supervision::{self, completer};
use gather::{ClaimGather, ItemGather};

/// Node settings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct QueueNodeConfig {
    /// Shard threads, 1 through 1024.
    pub shards: u16,
    /// Group log limits; a record must hold the largest protocol body.
    pub log: GroupLogLimits,
}

impl Default for QueueNodeConfig {
    fn default() -> Self {
        Self {
            shards: 16,
            log: GroupLogLimits {
                max_record_bytes: 32 * 1024 * 1024,
                ..GroupLogLimits::default()
            },
        }
    }
}

/// Why the node did not open.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NodeError {
    /// The shard count is 0 or above 1024.
    InvalidShards,
    /// The group log refused to open.
    Log(GroupLogError),
    /// A logged change at this position does not replay.
    Replay {
        /// Position among the recovered records.
        record: usize,
    },
    /// A runtime thread did not start.
    Thread,
}

impl fmt::Display for NodeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidShards => formatter.write_str("shard count must be 1 through 1024"),
            Self::Log(error) => write!(formatter, "group log: {error}"),
            Self::Replay { record } => write!(formatter, "logged change {record} does not replay"),
            Self::Thread => formatter.write_str("a runtime thread did not start"),
        }
    }
}

impl std::error::Error for NodeError {}

impl From<GroupLogError> for NodeError {
    fn from(error: GroupLogError) -> Self {
        Self::Log(error)
    }
}

struct Shard {
    requests: Option<mpsc::Sender<Request>>,
    thread: Option<JoinHandle<()>>,
    ready: Arc<AtomicU64>,
    leased: Arc<AtomicU64>,
}

/// The running queue node. Dropping it stops the shards, releases their
/// pending replies and closes the log.
pub struct QueueNode {
    shards: Vec<Shard>,
    completer: Option<JoinHandle<()>>,
    shard_count: u16,
}

fn replay(states: &mut [ShardState], records: &[Vec<u8>], shards: u16) -> Result<(), NodeError> {
    for (position, record) in records.iter().enumerate() {
        let failed = NodeError::Replay { record: position };
        let logged = Logged::decode(record).ok_or(failed)?;
        let index = shard_index(logged.partition(), shards).ok_or(failed)?;
        let state = states.get_mut(index).ok_or(failed)?;
        if state.apply(&logged) != Ok(Outcome::Changed) {
            return Err(failed);
        }
    }
    Ok(())
}

fn shard_index(partition: &Partition, shards: u16) -> Option<usize> {
    shard_for_slot(partition.slot(), shards).map(usize::from)
}

/// Fails a request that never reached its shard.
fn refuse(request: Request) {
    match request {
        Request::Create { done, .. } | Request::Complete { done, .. } => {
            done(Err(ServiceError::Unavailable));
        }
        Request::Claim { done, .. } => done(Err(ServiceError::Unavailable)),
    }
}

impl QueueNode {
    /// Opens the log in `directory`, replays it and starts the threads.
    ///
    /// # Errors
    /// Refuses an invalid shard count, a log that does not open, a logged
    /// change that does not replay and a thread that does not start.
    pub fn open(directory: &Path, config: QueueNodeConfig) -> Result<Self, NodeError> {
        if config.shards == 0 || config.shards > SLOTS {
            return Err(NodeError::InvalidShards);
        }
        let (log, recovered) = GroupLog::open(directory, config.log)?;
        let log = Arc::new(log);
        let mut states: Vec<ShardState> =
            (0..config.shards).map(|_| ShardState::default()).collect();
        replay(&mut states, &recovered.records, config.shards)?;
        let (completion_sender, completion_receiver) = mpsc::channel();
        let completer_log = Arc::clone(&log);
        let completer = supervision::spawn("wf-completer".into(), move || {
            completer::run(&completer_log, &completion_receiver);
        })
        .ok_or(NodeError::Thread)?;
        let mut shards = Vec::with_capacity(states.len());
        for (index, state) in states.into_iter().enumerate() {
            let (sender, receiver) = mpsc::channel();
            let ready = Arc::new(AtomicU64::new(state.ready()));
            let leased = Arc::new(AtomicU64::new(state.leased()));
            let links = Links {
                log: Arc::clone(&log),
                completions: completion_sender.clone(),
                ready: Arc::clone(&ready),
                leased: Arc::clone(&leased),
                max_append_bytes: config.log.max_block_bytes.checked_div(2).unwrap_or(0),
            };
            let thread = supervision::spawn(format!("wf-shard-{index}"), move || {
                shard::run(state, &receiver, &links);
            })
            .ok_or(NodeError::Thread)?;
            shards.push(Shard {
                requests: Some(sender),
                thread: Some(thread),
                ready,
                leased,
            });
        }
        Ok(Self {
            shards,
            completer: Some(completer),
            shard_count: config.shards,
        })
    }

    fn send(&self, index: usize, request: Request) {
        match self
            .shards
            .get(index)
            .and_then(|shard| shard.requests.as_ref())
        {
            Some(sender) => {
                if let Err(returned) = sender.send(request) {
                    refuse(returned.0);
                }
            }
            None => refuse(request),
        }
    }

    fn route(&self, partition: &Partition) -> usize {
        shard_index(partition, self.shard_count).unwrap_or(0)
    }
}

impl QueueService for QueueNode {
    fn create(&self, request: CreateFlows, reply: Reply<Vec<ItemOutcome>>) {
        let CreateFlows {
            flow_type,
            state,
            run_at_ms,
            priority,
            flows,
        } = request;
        let total = flows.len();
        let mut per_shard = vec![Vec::new(); self.shards.len()];
        for (position, flow) in flows.into_iter().enumerate() {
            let partition = Partition::for_new_flow(&flow.id, flow.partition.as_deref());
            if let Some(items) = per_shard.get_mut(self.route(&partition)) {
                items.push((position, flow, partition));
            }
        }
        let parts = per_shard.iter().filter(|items| !items.is_empty()).count();
        if parts == 0 {
            reply(Ok(Vec::new()));
            return;
        }
        let gather = Arc::new(ItemGather::new(total, parts, reply));
        for (index, items) in per_shard
            .into_iter()
            .enumerate()
            .filter(|(_, items)| !items.is_empty())
        {
            let part = Arc::clone(&gather);
            self.send(
                index,
                Request::Create {
                    flow_type: flow_type.clone(),
                    state: state.clone(),
                    run_at_ms,
                    priority,
                    items,
                    done: Box::new(move |result| part.part(result)),
                },
            );
        }
    }

    fn claim(&self, request: ClaimFlows, reply: Reply<Vec<ClaimedFlow>>) {
        let partitions: Option<Arc<[Partition]>> = request
            .partitions
            .as_ref()
            .map(|names| names.iter().map(|name| Partition::parse(name)).collect());
        let shares = match &partitions {
            Some(partitions) => self.partition_shares(partitions, request.limit),
            None => self.ready_shares(&request),
        };
        if shares.is_empty() {
            reply(Ok(Vec::new()));
            return;
        }
        let gather = Arc::new(ClaimGather::new(shares.len(), reply));
        let request = Arc::new(request);
        for (index, limit) in shares {
            let part = Arc::clone(&gather);
            self.send(
                index,
                Request::Claim {
                    request: Arc::clone(&request),
                    limit,
                    partitions: partitions.clone(),
                    done: Box::new(move |result| part.part(result)),
                },
            );
        }
    }

    fn complete(&self, completions: Vec<CompleteFlow>, reply: Reply<Vec<ItemOutcome>>) {
        let total = completions.len();
        let mut per_shard = vec![Vec::new(); self.shards.len()];
        for (position, completion) in completions.into_iter().enumerate() {
            let partition =
                Partition::for_new_flow(&completion.id, completion.partition.as_deref());
            if let Some(items) = per_shard.get_mut(self.route(&partition)) {
                items.push((position, completion, partition));
            }
        }
        let parts = per_shard.iter().filter(|items| !items.is_empty()).count();
        if parts == 0 {
            reply(Ok(Vec::new()));
            return;
        }
        let gather = Arc::new(ItemGather::new(total, parts, reply));
        for (index, items) in per_shard
            .into_iter()
            .enumerate()
            .filter(|(_, items)| !items.is_empty())
        {
            let part = Arc::clone(&gather);
            self.send(
                index,
                Request::Complete {
                    items,
                    done: Box::new(move |result| part.part(result)),
                },
            );
        }
    }
}

impl QueueNode {
    /// Every shard owning one of the partitions may claim up to `limit`;
    /// the shards together never return more than `limit`.
    fn partition_shares(&self, partitions: &[Partition], limit: usize) -> Vec<(usize, usize)> {
        let mut owners: Vec<usize> = partitions
            .iter()
            .map(|partition| self.route(partition))
            .collect();
        owners.sort_unstable();
        owners.dedup();
        split(
            limit,
            &owners
                .iter()
                .map(|owner| (*owner, 1_u64))
                .collect::<Vec<_>>(),
        )
    }

    /// Shards with indexed Flows the claim may take, each asked for a share
    /// of the limit proportional to its count: due-indexed Flows, plus leased
    /// ones when the claim reclaims.
    fn ready_shares(&self, request: &ClaimFlows) -> Vec<(usize, usize)> {
        let running = request.state.as_deref() == Some(b"running".as_slice());
        let weights: Vec<(usize, u64)> = self
            .shards
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
            .collect();
        split(request.limit, &weights)
    }
}

/// Splits `limit` over weighted shards: proportional floors, then the rest
/// one at a time in order. Shards with a zero share are left out.
fn split(limit: usize, weights: &[(usize, u64)]) -> Vec<(usize, usize)> {
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

impl Drop for QueueNode {
    fn drop(&mut self) {
        for shard in &mut self.shards {
            shard.requests = None;
        }
        for shard in &mut self.shards {
            if let Some(thread) = shard.thread.take() {
                // A shard thread only returns; a join error has nothing to report.
                let _joined = thread.join();
            }
        }
        if let Some(completer) = self.completer.take() {
            let _joined = completer.join();
        }
    }
}
