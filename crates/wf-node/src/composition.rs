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
mod reads;
mod replay;
mod replica;
mod router;
mod schedule;
mod snapshot;
mod started;

use std::path::Path;
use std::sync::{Arc, mpsc};
use std::thread::JoinHandle;
use std::time::Duration;

use wf_application::commands::queue::{Partition, SLOTS};
use wf_domain::retention::ResolvedRetention;
use wf_storage::log::{GroupLog, GroupLogLimits, SegmentStorage};
use wf_storage::snapshot::files::SnapshotFiles;

use crate::supervision::shard::role::Role;
use crate::supervision::shard::sink::Sink;
use crate::supervision::waiters::Waiters;
use crate::supervision::{self, completer, scheduler, snapshots};
pub use admission::Admission;
pub use error::NodeError;
use replay::replay;
pub use replica::Replica;
use router::Router;
use snapshot::Snapshots;
use started::Started;

/// How long the schedule thread waits idle before polling, as Elixir's
/// scheduler sleeps between empty polls.
const SCHEDULE_POLL: Duration = Duration::from_secs(1);

/// Bytes of replaced value versions a running Flow keeps in memory by
/// default (ADR-016 phase D): a dozen or so typical payloads.
const KEPT_MEMORY_BYTES: u64 = 16 * 1024;

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
    /// Bytes of replaced value versions a running Flow keeps in memory;
    /// beyond them its versions go to disk (ADR-016 phase D).
    pub kept_memory_bytes: u64,
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
            kept_memory_bytes: KEPT_MEMORY_BYTES,
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
    /// `None` for a replicated node (ADR-018 phase 3 adds its snapshots).
    snapshots: Option<Arc<Snapshots>>,
    snapshot_stop: Option<mpsc::Sender<()>>,
    snapshotter: Option<JoinHandle<()>>,
    /// Schedule commands for the schedule thread (ADR-014 step 13).
    schedule_jobs: Option<mpsc::Sender<scheduler::Job>>,
    scheduling: Option<JoinHandle<()>>,
    /// A replicated node's group member and its runtime (ADR-018).
    group: Option<replica::Group>,
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
        // Replay from the last snapshot reads the cold files it names.
        cold.keep_for_snapshots();
        let mut cut = match files.newest()? {
            Some((id, body)) => Some(snapshot::load(&mut states, id, &body, config.shards)?),
            None => None,
        };
        let named = cold.held();
        // The snapshot files above created the data directory.
        let (log, recovered) = match log {
            Some(storage) => GroupLog::open_with(storage, config.log)?,
            None => GroupLog::open(&directory.join("log"), config.log)?,
        };
        let log = Arc::new(log);
        let newest_marker = replay(&mut states, &recovered.records, config.shards, cut.as_mut())?;
        cold::tidy(&cold, &mut states, &named)?;
        let (next, base) = match &cut {
            Some(cut) => (cut.id.max(newest_marker), cut.base),
            None => (
                newest_marker,
                recovered.segments.first().copied().unwrap_or(0),
            ),
        };
        let snapshots = Arc::new(Snapshots::new(
            files,
            (Arc::clone(&log), cold),
            config.snapshot_segments,
            next.saturating_add(1),
            base,
        ));
        if let Some(cut) = &cut {
            snapshots.tidy(cut)?;
        }
        let (completion_sender, completion_receiver) = mpsc::channel();
        let completer_log = Arc::clone(&log);
        let completer = supervision::spawn("wf-completer".into(), move || {
            completer::run(&completer_log, &completion_receiver);
        })
        .ok_or(NodeError::Thread)?;
        let sink = Sink::Local {
            log: Arc::clone(&log),
            completions: completion_sender,
            // Half a block, and no more than one record takes: a run of
            // after-images sharing a payload is one record (perf note P1).
            max_append_bytes: config
                .log
                .max_block_bytes
                .checked_div(2)
                .unwrap_or(0)
                .min(config.log.max_record_bytes),
        };
        let started = Started::new(states, &config, sink, Role::single())?;
        let router = Arc::clone(&started.router);
        let (snapshot_stop, snapshotter) = start_snapshots(&snapshots, &router)?;
        let (schedule_jobs, scheduling) = start_schedules(&router, config.schedule_poll)?;
        Ok(Self {
            credit_cache: credit::Cache::default(),
            router: Some(started.router),
            waiters: started.waiters,
            shards: started.shards,
            completer: Some(completer),
            waiter: Some(started.waiter),
            snapshots: Some(snapshots),
            snapshot_stop: Some(snapshot_stop),
            snapshotter: Some(snapshotter),
            schedule_jobs: Some(schedule_jobs),
            scheduling: Some(scheduling),
            group: None,
        })
    }

    /// Takes a snapshot now and removes the log before it; returns its id.
    ///
    /// # Errors
    /// Refuses when the log cannot rotate or remove segments, a shard does
    /// not capture, or snapshot storage fails; the log then stays as it was.
    pub fn snapshot(&self) -> Result<u64, NodeError> {
        let router = self.router.as_ref().ok_or(NodeError::Stopped)?;
        let snapshots = self.snapshots.as_ref().ok_or(NodeError::Group(
            "a replicated node does not take snapshots yet",
        ))?;
        snapshots.take(router)
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

impl Drop for QueueNode {
    /// Closes the shards' channels, waits for the shards and the completion
    /// thread, then stops the waiter thread, which fails what still waits.
    fn drop(&mut self) {
        // A replicated node stops its member first, so nothing applies to
        // shards that are stopping.
        if let Some(mut group) = self.group.take() {
            group.stop();
        }
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
