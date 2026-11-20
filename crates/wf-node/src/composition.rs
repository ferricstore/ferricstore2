//! The queue node (ADR-011, ADR-013).
//!
//! Opening the node opens the group log, replays every logged change into
//! the shards, then starts one thread per shard, the completion thread and
//! the waiter thread. The node implements the queue service by routing each
//! Flow to the shard that owns its partition; claims run in
//! [`claim`](self::claim).

mod claim;
mod gather;
mod router;

use std::fmt;
use std::path::Path;
use std::sync::atomic::AtomicU64;
use std::sync::{Arc, mpsc};
use std::thread::JoinHandle;

use wf_application::commands::queue::{
    ClaimFlows, ClaimedFlow, CompleteFlow, CreateFlows, ItemOutcome, Partition, QueueService,
    Reply, SLOTS, ServiceError,
};
use wf_domain::retention::ResolvedRetention;
use wf_domain::workflow::Outcome;
use wf_storage::log::{GroupLog, GroupLogError, GroupLogLimits};

use crate::supervision::shard::command::Logged;
use crate::supervision::shard::state::ShardState;
use crate::supervision::shard::{self, Links, Request};
use crate::supervision::waiters::Waiters;
use crate::supervision::{self, completer};
use gather::ItemGather;
use router::{Router, ShardLink, shard_index};

/// Node settings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct QueueNodeConfig {
    /// Shard threads, 1 through 1024.
    pub shards: u16,
    /// Group log limits; a record must hold the largest protocol body.
    pub log: GroupLogLimits,
    /// How long completed Flows are kept before removal (ADR-013).
    pub retention_ms: u64,
}

impl Default for QueueNodeConfig {
    fn default() -> Self {
        Self {
            shards: 16,
            log: GroupLogLimits {
                max_record_bytes: 32 * 1024 * 1024,
                ..GroupLogLimits::default()
            },
            retention_ms: ResolvedRetention::default().ttl_ms,
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

/// The running queue node. Dropping it stops the shards, releases their
/// pending replies, fails waiting claims and closes the log.
pub struct QueueNode {
    router: Option<Arc<Router>>,
    waiters: Waiters,
    shards: Vec<JoinHandle<()>>,
    completer: Option<JoinHandle<()>>,
    waiter: Option<JoinHandle<()>>,
}

fn replay(states: &mut [ShardState], records: &[Vec<u8>], shards: u16) -> Result<(), NodeError> {
    for (position, record) in records.iter().enumerate() {
        let failed = NodeError::Replay { record: position };
        let logged = Logged::decode(record).ok_or(failed)?;
        for (index, change) in by_shard(logged, shards).ok_or(failed)? {
            let state = states.get_mut(index).ok_or(failed)?;
            if state.apply(&change) != Ok(Outcome::Changed) {
                return Err(failed);
            }
        }
    }
    Ok(())
}

/// Routes a logged change to the shards owning its Flows; a sweep is split
/// into one sweep per shard.
fn by_shard(logged: Logged, shards: u16) -> Option<Vec<(usize, Logged)>> {
    let Logged::Expire { now, keys } = logged else {
        let index = shard_index(logged.partition()?, shards)?;
        return Some(vec![(index, logged)]);
    };
    let mut parts: Vec<(usize, Logged)> = Vec::new();
    for key in keys {
        let index = shard_index(&key.0, shards)?;
        match parts.iter_mut().find(|(owner, _)| *owner == index) {
            Some((_, Logged::Expire { keys, .. })) => keys.push(key),
            _ => parts.push((
                index,
                Logged::Expire {
                    now,
                    keys: vec![key],
                },
            )),
        }
    }
    Some(parts)
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
        let mut states: Vec<ShardState> = (0..config.shards)
            .map(|_| ShardState::new(config.retention_ms))
            .collect();
        replay(&mut states, &recovered.records, config.shards)?;
        let (waiter_sender, waiter_receiver) = mpsc::channel();
        let waiters = Waiters::new(waiter_sender);
        let waiter_handle = waiters.clone();
        let waiter = supervision::spawn("wf-waiters".into(), move || {
            waiter_handle.run(&waiter_receiver);
        })
        .ok_or(NodeError::Thread)?;
        let (completion_sender, completion_receiver) = mpsc::channel();
        let completer_log = Arc::clone(&log);
        let completer = supervision::spawn("wf-completer".into(), move || {
            completer::run(&completer_log, &completion_receiver);
        })
        .ok_or(NodeError::Thread)?;
        let mut links = Vec::with_capacity(states.len());
        let mut threads = Vec::with_capacity(states.len());
        for (index, state) in states.into_iter().enumerate() {
            let (sender, receiver) = mpsc::channel();
            let ready = Arc::new(AtomicU64::new(state.ready()));
            let leased = Arc::new(AtomicU64::new(state.leased()));
            let shard_links = Links {
                log: Arc::clone(&log),
                completions: completion_sender.clone(),
                ready: Arc::clone(&ready),
                leased: Arc::clone(&leased),
                waiters: waiters.clone(),
                max_append_bytes: config.log.max_block_bytes.checked_div(2).unwrap_or(0),
            };
            threads.push(
                supervision::spawn(format!("wf-shard-{index}"), move || {
                    shard::run(state, &receiver, &shard_links);
                })
                .ok_or(NodeError::Thread)?,
            );
            links.push(ShardLink {
                requests: sender,
                ready,
                leased,
            });
        }
        let router = Router {
            shards: links,
            shard_count: config.shards,
            waiters: waiters.clone(),
        };
        Ok(Self {
            router: Some(Arc::new(router)),
            waiters,
            shards: threads,
            completer: Some(completer),
            waiter: Some(waiter),
        })
    }

    fn per_shard<T>(
        router: &Router,
        items: impl IntoIterator<Item = (Vec<u8>, Option<Vec<u8>>, T)>,
    ) -> Vec<Vec<(usize, T, Partition)>> {
        let mut per_shard: Vec<Vec<(usize, T, Partition)>> =
            (0..router.shards.len()).map(|_| Vec::new()).collect();
        for (position, (id, partition, item)) in items.into_iter().enumerate() {
            let partition = Partition::for_new_flow(&id, partition.as_deref());
            if let Some(items) = per_shard.get_mut(router.route(&partition)) {
                items.push((position, item, partition));
            }
        }
        per_shard
    }
}

impl QueueService for QueueNode {
    fn create(&self, request: CreateFlows, reply: Reply<Vec<ItemOutcome>>) {
        let Some(router) = self.router.as_ref() else {
            return reply(Err(ServiceError::Unavailable));
        };
        let CreateFlows {
            flow_type,
            state,
            run_at_ms,
            priority,
            flows,
        } = request;
        let total = flows.len();
        let per_shard = Self::per_shard(
            router,
            flows
                .into_iter()
                .map(|flow| (flow.id.clone(), flow.partition.clone(), flow)),
        );
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
            router.send(
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
        match self.router.as_ref() {
            Some(router) => claim::start(router, request, reply),
            None => reply(Err(ServiceError::Unavailable)),
        }
    }

    fn complete(&self, completions: Vec<CompleteFlow>, reply: Reply<Vec<ItemOutcome>>) {
        let Some(router) = self.router.as_ref() else {
            return reply(Err(ServiceError::Unavailable));
        };
        let total = completions.len();
        let per_shard = Self::per_shard(
            router,
            completions.into_iter().map(|completion| {
                (
                    completion.id.clone(),
                    completion.partition.clone(),
                    completion,
                )
            }),
        );
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
            router.send(
                index,
                Request::Complete {
                    items,
                    done: Box::new(move |result| part.part(result)),
                },
            );
        }
    }
}

impl Drop for QueueNode {
    /// Closes the shards' channels, waits for the shards and the completion
    /// thread, then stops the waiter thread, which fails what still waits.
    fn drop(&mut self) {
        self.router = None;
        for thread in self.shards.drain(..) {
            // A runtime thread only returns; a join error has nothing to report.
            let _joined = thread.join();
        }
        if let Some(completer) = self.completer.take() {
            let _joined = completer.join();
        }
        self.waiters.stop();
        if let Some(waiter) = self.waiter.take() {
            let _joined = waiter.join();
        }
    }
}
