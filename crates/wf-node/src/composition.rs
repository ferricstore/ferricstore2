//! The queue node (ADR-011, ADR-013).
//!
//! The data directory holds `log/` and `snapshots/`. Opening the node loads
//! the newest snapshot, replays the log changes it does not hold, then starts
//! one thread per shard, the completion thread, the waiter thread and the
//! snapshot thread. The node implements the queue service by routing each
//! Flow to the shard that owns its partition; claims run in the private
//! `claim` module.

mod admission;
mod claim;
mod cleanup;
mod cold;
mod credit;
mod error;
mod flow;
mod gather;
mod governance;
mod info;
mod policy;
mod queue;
mod replay;
mod router;
mod schedule;
mod snapshot;

use std::path::Path;
use std::sync::atomic::{AtomicU64, AtomicUsize};
use std::sync::{Arc, OnceLock, mpsc};
use std::thread::JoinHandle;
use std::time::Duration;

use wf_application::commands::queue::{Partition, SLOTS};
use wf_domain::retention::ResolvedRetention;
use wf_storage::log::{GroupLog, GroupLogLimits, SegmentStorage};
use wf_storage::snapshot::files::SnapshotFiles;

use crate::supervision::shard::state::ShardState;
use crate::supervision::shard::{self, Links};
use crate::supervision::waiters::Waiters;
use crate::supervision::{self, completer, scheduler, snapshots};
pub use admission::Admission;
pub use error::NodeError;
use replay::replay;
use router::{Router, ShardLink};
use snapshot::Snapshots;

/// How long the schedule thread waits idle before polling, as Elixir's
/// scheduler sleeps between empty polls.
const SCHEDULE_POLL: Duration = Duration::from_secs(1);

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
    /// How long the schedule thread waits idle before firing due schedules
    /// at wall time; `None` fires them only on `FLOW.SCHEDULE.FIRE_DUE`
    /// (ADR-014 step 13).
    pub schedule_poll: Option<Duration>,
    /// Per-shard backlog limits for client requests (ADR-017).
    pub admission: Admission,
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
            schedule_poll: Some(SCHEDULE_POLL),
            admission: Admission::default(),
        }
    }
}

/// The running queue node. Dropping it stops the shards, releases their
/// pending replies, fails waiting claims and closes the log.
pub struct QueueNode {
    router: Option<Arc<Router>>,
    /// Credits spent ahead for approximate limits (ADR-014 step 15).
    credit_cache: credit::Cache,
    waiters: Waiters,
    shards: Vec<JoinHandle<()>>,
    completer: Option<JoinHandle<()>>,
    waiter: Option<JoinHandle<()>>,
    snapshots: Arc<Snapshots>,
    snapshot_stop: Option<mpsc::Sender<()>>,
    snapshotter: Option<JoinHandle<()>>,
    /// Schedule commands for the schedule thread (ADR-014 step 13).
    schedule_jobs: Option<mpsc::Sender<scheduler::Job>>,
    scheduling: Option<JoinHandle<()>>,
}

/// Starts the schedule thread; dropping the returned sender stops it.
fn start_schedules(
    router: &Arc<Router>,
    poll: Option<Duration>,
) -> Result<(mpsc::Sender<scheduler::Job>, JoinHandle<()>), NodeError> {
    let (jobs_sender, jobs) = mpsc::channel();
    let router = Arc::downgrade(router);
    let thread = supervision::spawn("wf-schedules".into(), move || {
        let tick = poll.map(|_| {
            move || {
                router
                    .upgrade()
                    .is_some_and(|router| schedule::poll(&router))
            }
        });
        scheduler::run(&jobs, poll.unwrap_or(Duration::from_secs(3_600)), tick);
    })
    .ok_or(NodeError::Thread)?;
    Ok((jobs_sender, thread))
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
        Self::opened(directory, config, None)
    }

    /// Opens the node as [`Self::open`] does, with its log kept in `log`.
    ///
    /// # Errors
    /// As [`Self::open`].
    pub fn open_with_log(
        directory: &Path,
        config: QueueNodeConfig,
        log: Arc<dyn SegmentStorage>,
    ) -> Result<Self, NodeError> {
        Self::opened(directory, config, Some(log))
    }

    fn opened(
        directory: &Path,
        config: QueueNodeConfig,
        log: Option<Arc<dyn SegmentStorage>>,
    ) -> Result<Self, NodeError> {
        if config.shards == 0 || config.shards > SLOTS {
            return Err(NodeError::InvalidShards);
        }
        let files = SnapshotFiles::open(&directory.join("snapshots"))?;
        let (cold, mut states) = cold::shards(directory, &config)?;
        let mut cut = match files.newest()? {
            Some((id, body)) => Some(snapshot::load(&mut states, id, &body, config.shards)?),
            None => None,
        };
        // The snapshot files above created the data directory.
        let (log, recovered) = match log {
            Some(storage) => GroupLog::open_with(storage, config.log)?,
            None => GroupLog::open(&directory.join("log"), config.log)?,
        };
        let log = Arc::new(log);
        let newest_marker = replay(&mut states, &recovered.records, config.shards, cut.as_mut())?;
        cold::tidy(&cold, &mut states)?;
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
        let release_router = Arc::new(OnceLock::new());
        let running = credit::running::opened(states.first());
        let (links, threads) = start_shards(
            states,
            &Links {
                index: 0,
                log: Arc::clone(&log),
                completions: completion_sender,
                ready: Arc::new(AtomicU64::new(0)),
                leased: Arc::new(AtomicU64::new(0)),
                waiters: waiters.clone(),
                max_append_bytes: config.log.max_block_bytes.checked_div(2).unwrap_or(0),
                release: credit::releaser(Arc::clone(&release_router)),
                backlog: Arc::new(AtomicUsize::new(0)),
            },
        )?;
        let router = Router {
            shards: links,
            shard_count: config.shards,
            waiters: waiters.clone(),
            running,
            admission: config.admission,
        };
        let router = Arc::new(router);
        let _set = release_router.set(Arc::downgrade(&router));
        let (snapshot_stop, snapshotter) = start_snapshots(&snapshots, &router)?;
        let (schedule_jobs, scheduling) = start_schedules(&router, config.schedule_poll)?;
        Ok(Self {
            credit_cache: credit::Cache::default(),
            router: Some(router),
            waiters,
            shards: threads,
            completer: Some(completer),
            waiter: Some(waiter),
            snapshots,
            snapshot_stop: Some(snapshot_stop),
            snapshotter: Some(snapshotter),
            schedule_jobs: Some(schedule_jobs),
            scheduling: Some(scheduling),
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

/// Starts one thread per shard; each gets `shared`'s log, completions,
/// waiters, append size and releases, and its own index and counters.
fn start_shards(
    states: Vec<ShardState>,
    shared: &Links,
) -> Result<(Vec<ShardLink>, Vec<JoinHandle<()>>), NodeError> {
    let mut links = Vec::with_capacity(states.len());
    let mut threads = Vec::with_capacity(states.len());
    for (index, state) in states.into_iter().enumerate() {
        let (sender, receiver) = mpsc::channel();
        let ready = Arc::new(AtomicU64::new(state.ready()));
        let leased = Arc::new(AtomicU64::new(state.leased()));
        let backlog = Arc::new(AtomicUsize::new(0));
        let shard_links = Links {
            index: u16::try_from(index).map_err(|_| NodeError::InvalidShards)?,
            log: Arc::clone(&shared.log),
            completions: shared.completions.clone(),
            ready: Arc::clone(&ready),
            leased: Arc::clone(&leased),
            waiters: shared.waiters.clone(),
            max_append_bytes: shared.max_append_bytes,
            release: Arc::clone(&shared.release),
            backlog: Arc::clone(&backlog),
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
            backlog,
        });
    }
    Ok((links, threads))
}

impl Drop for QueueNode {
    /// Closes the shards' channels, waits for the shards and the completion
    /// thread, then stops the waiter thread, which fails what still waits.
    fn drop(&mut self) {
        self.schedule_jobs = None;
        if let Some(scheduling) = self.scheduling.take() {
            // A runtime thread only returns; a join error has nothing to report.
            let _joined = scheduling.join();
        }
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
