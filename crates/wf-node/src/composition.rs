//! The queue node (ADR-011, ADR-013).
//!
//! The data directory holds `log/` and `snapshots/`. Opening the node loads
//! the newest snapshot, replays the log changes it does not hold, then starts
//! one thread per shard, the completion thread, the waiter thread and the
//! snapshot thread. The node implements the queue service by routing each
//! Flow to the shard that owns its partition; claims run in the private
//! `claim` module.

mod claim;
mod cleanup;
mod flow;
mod gather;
mod policy;
mod replay;
mod router;
mod snapshot;

use std::fmt;
use std::path::Path;
use std::sync::atomic::AtomicU64;
use std::sync::{Arc, mpsc};
use std::thread::JoinHandle;
use std::time::Duration;

use wf_application::commands::queue::{
    ClaimFlows, ClaimedFlow, CompleteFlow, CreateFlows, ItemOutcome, Partition, QueueService,
    Reply, SLOTS, ServiceError,
};
use wf_domain::retention::ResolvedRetention;
use wf_storage::log::{GroupLog, GroupLogError, GroupLogLimits};
use wf_storage::snapshot::files::{SnapshotError, SnapshotFiles};

use crate::supervision::shard::state::ShardState;
use crate::supervision::shard::{self, Links, Request};
use crate::supervision::waiters::Waiters;
use crate::supervision::{self, completer, snapshots};
use gather::ItemGather;
use replay::replay;
use router::{Router, ShardLink};
use snapshot::Snapshots;

/// How often the snapshot thread checks whether a snapshot is due.
const SNAPSHOT_CHECK: Duration = Duration::from_millis(250);

/// Node settings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct QueueNodeConfig {
    /// Shard threads, 1 through 1024.
    pub shards: u16,
    /// Group log limits; a record must hold the largest protocol body.
    pub log: GroupLogLimits,
    /// How long completed Flows are kept before removal (ADR-013).
    pub retention_ms: u64,
    /// Log segments between automatic snapshots; 0 takes none (ADR-013).
    pub snapshot_segments: u64,
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
            snapshot_segments: 2,
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
    /// Snapshot storage refused a read, write or removal.
    Snapshot(SnapshotError),
    /// A snapshot body or shard image does not decode.
    SnapshotImage,
    /// A shard of the loaded snapshot never logged its marker, so the log
    /// does not match the snapshot.
    MissingMarker {
        /// The shard, under the snapshot's shard count.
        shard: u16,
    },
    /// A shard did not capture its records for a snapshot.
    Capture,
    /// The node is stopping.
    Stopped,
}

impl fmt::Display for NodeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidShards => formatter.write_str("shard count must be 1 through 1024"),
            Self::Log(error) => write!(formatter, "group log: {error}"),
            Self::Replay { record } => write!(formatter, "logged change {record} does not replay"),
            Self::Thread => formatter.write_str("a runtime thread did not start"),
            Self::Snapshot(error) => write!(formatter, "snapshot: {error}"),
            Self::SnapshotImage => formatter.write_str("a snapshot does not decode"),
            Self::MissingMarker { shard } => {
                write!(formatter, "the log lacks shard {shard}'s snapshot marker")
            }
            Self::Capture => formatter.write_str("a shard did not capture its snapshot"),
            Self::Stopped => formatter.write_str("the node is stopping"),
        }
    }
}

impl std::error::Error for NodeError {}

impl From<GroupLogError> for NodeError {
    fn from(error: GroupLogError) -> Self {
        Self::Log(error)
    }
}

impl From<SnapshotError> for NodeError {
    fn from(error: SnapshotError) -> Self {
        Self::Snapshot(error)
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
    snapshots: Arc<Snapshots>,
    snapshot_stop: Option<mpsc::Sender<()>>,
    snapshotter: Option<JoinHandle<()>>,
}

/// Starts the snapshot thread; dropping the returned sender stops it.
fn start_snapshots(
    snapshots: &Arc<Snapshots>,
    router: &Arc<Router>,
) -> Result<(mpsc::Sender<()>, JoinHandle<()>), NodeError> {
    let (stop_sender, stop) = mpsc::channel();
    let snapshots = Arc::clone(snapshots);
    let router = Arc::downgrade(router);
    let thread = supervision::spawn("wf-snapshots".into(), move || {
        snapshots::run(&stop, SNAPSHOT_CHECK, || snapshots.check(&router));
    })
    .ok_or(NodeError::Thread)?;
    Ok((stop_sender, thread))
}

impl QueueNode {
    /// Opens the node in `directory`: loads the newest snapshot, replays the
    /// log after it and starts the threads.
    ///
    /// # Errors
    /// Refuses an invalid shard count, a log or snapshot that does not open
    /// or decode, a logged change that does not replay, a missing snapshot
    /// marker and a thread that does not start.
    pub fn open(directory: &Path, config: QueueNodeConfig) -> Result<Self, NodeError> {
        if config.shards == 0 || config.shards > SLOTS {
            return Err(NodeError::InvalidShards);
        }
        let files = SnapshotFiles::open(&directory.join("snapshots"))?;
        let mut states: Vec<ShardState> = (0..config.shards)
            .map(|_| ShardState::new(config.retention_ms))
            .collect();
        let mut cut = match files.newest()? {
            Some((id, body)) => Some(snapshot::load(&mut states, id, &body, config.shards)?),
            None => None,
        };
        let (log, recovered) = GroupLog::open(&directory.join("log"), config.log)?;
        let log = Arc::new(log);
        let newest_marker = replay(&mut states, &recovered.records, config.shards, cut.as_mut())?;
        let (next, base) = match &cut {
            Some(cut) => (cut.id.max(newest_marker), cut.base),
            None => (
                newest_marker,
                recovered.segments.first().copied().unwrap_or(0),
            ),
        };
        let snapshots = Arc::new(Snapshots::new(
            files,
            Arc::clone(&log),
            config.snapshot_segments,
            next.saturating_add(1),
            base,
        ));
        if let Some(cut) = &cut {
            snapshots.tidy(cut)?;
        }
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
                index: u16::try_from(index).map_err(|_| NodeError::InvalidShards)?,
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
        let router = Arc::new(router);
        let (snapshot_stop, snapshotter) = start_snapshots(&snapshots, &router)?;
        Ok(Self {
            router: Some(router),
            waiters,
            shards: threads,
            completer: Some(completer),
            waiter: Some(waiter),
            snapshots,
            snapshot_stop: Some(snapshot_stop),
            snapshotter: Some(snapshotter),
        })
    }

    /// Takes a snapshot now and removes the log before it; returns its id.
    ///
    /// # Errors
    /// Refuses when the log cannot rotate or remove segments, a shard does
    /// not capture, or snapshot storage fails; the log then stays as it was.
    pub fn snapshot(&self) -> Result<u64, NodeError> {
        let router = self.router.as_ref().ok_or(NodeError::Stopped)?;
        self.snapshots.take(router)
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
        self.snapshot_stop = None;
        if let Some(snapshotter) = self.snapshotter.take() {
            // A runtime thread only returns; a join error has nothing to report.
            let _joined = snapshotter.join();
        }
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
